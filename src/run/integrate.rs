//! The integration run (MOC-C TASK-003, REQ-004).
//!
//! Every task of a handover can pass on its own branch while the feature
//! does not work as a whole. An integration run checks the whole: a
//! worktree holding the output of every leaf task — tasks no other
//! handed-over task depends on, whose outputs hold all the others' — and
//! the integration check the user accepted, run command by command. No
//! agent is called and no budget is spent: an integration failure is a
//! question about the contract between tasks, answered through an
//! amendment. A check whose failure output names the environment (Docker,
//! testcontainers, a refused connection — [`INFRA_PATTERNS`] plus
//! `execution.infra_patterns`) is run once more; failing again, the run
//! fails with an [`INFRA`] reason, which the feature loop retries.
//!
//! It is an ordinary run record (`kind: integration`), so it has the same
//! events, states, cancel and retry as a task run. Passing seals the tested
//! tree as the run's output, like a task run.

use super::contract::{self, Handover};
use super::record::{self, Checks, Run, RunEvent, RunKind, RunMeta, RunState, Source, Start};
use super::{output, start, worktree};
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// `RunMeta.task` of an integration run.
pub const TASK: &str = "integration";
/// Every command's output, in order, in the run's directory.
pub const CHECKS_LOG: &str = "checks.log";
/// One integration command may run this long; a whole suite plus lints is
/// slower than one task's tests.
const CHECK_TIMEOUT_SECS: u64 = 1800;
/// Lines of a failing command's output kept in the run's reason.
const REASON_TAIL_LINES: usize = 20;
/// Prefix of the reason of an integration run that failed on the
/// environment — a check failing twice with output matching an
/// infrastructure pattern. The code was not judged, so the feature loop
/// runs it again on its next invocation (`feature_ops::resumable`).
pub const INFRA: &str = "infra: ";
/// Wait before running a check again after an infrastructure failure.
const INFRA_RETRY_DELAY: Duration = Duration::from_secs(5);
/// Output that says a check failed on the environment, not the code
/// (case-insensitive substrings); `execution.infra_patterns` adds more.
pub const INFRA_PATTERNS: &[&str] = &[
    "Cannot connect to the Docker daemon",
    "start postgres container",
    "failed to start container",
    "testcontainers",
    "connection refused",
    "i/o timeout",
    "no space left on device",
    "TLS handshake timeout",
];

/// `zforge/<INTAKE>/integration/<RUN>`.
pub fn branch_name(intake: &str, run: &str) -> String {
    format!("zforge/{intake}/{TASK}/{run}")
}

/// Record an integration run of handover `handover_id`. Refused, creating
/// nothing, while any handed-over task has no verified output.
pub fn create(project_root: &Path, handover_id: &str, retry_of: Option<&str>) -> Result<Run> {
    let h = contract::load_handover(project_root, handover_id)?;
    let from = leaf_outputs(project_root, &h)?;
    let config = crate::config::load_from(&project_root.join(".zforge").join("config.yaml"))?;
    let checks = h.checks(
        config.project.integration_command.as_deref(),
        &config.project.test_command,
    )?;
    let _claim = super::feature_ops::claim(
        project_root,
        &h.intake,
        &h.manifest.id,
        TASK,
        RunKind::Integration,
    )?;
    record::create(project_root, |id| RunMeta {
        id: id.to_string(),
        created_at: Utc::now(),
        intake: h.intake.clone(),
        handover: h.manifest.id.clone(),
        task: TASK.to_string(),
        manifest_sha256: h.manifest_sha256.clone(),
        worktree: worktree::path_for(project_root, id),
        branch: branch_name(&h.intake, id),
        baseline_commit: h.manifest.baseline.commit.clone(),
        budget_usd: 0.0,
        max_iterations: 1,
        retry_of: retry_of.map(str::to_string),
        start: Some(Start {
            commit: from[0].commit.clone(),
            from: from.clone(),
            previous: None,
            rebased: None,
        }),
        kind: RunKind::Integration,
        checks: Some(checks.clone()),
    })
}

/// The outputs of the leaf tasks, after checking every task has one.
pub(crate) fn leaf_outputs(project_root: &Path, h: &Handover) -> Result<Vec<Source>> {
    let id = &h.manifest.id;
    let mut runs = Vec::new();
    for run in record::list(project_root)? {
        let meta = run.meta()?;
        if meta.intake == h.intake && &meta.handover == id {
            let state = run.state()?;
            runs.push((meta, state));
        }
    }
    let mut outputs = std::collections::BTreeMap::new();
    for task in &h.manifest.tasks {
        let src = start::output_of(task, id, &runs)
            .map_err(|why| anyhow!("integration needs every task verified: {why}"))?;
        outputs.insert(task.clone(), src);
    }
    let from: Vec<Source> = h
        .leaves()?
        .into_iter()
        .filter_map(|t| outputs.remove(&t))
        .collect();
    if from.is_empty() {
        bail!("{id} has no tasks to integrate");
    }
    Ok(from)
}

/// Carry out an integration run to its end.
pub fn execute(project_root: &Path, run: &Run) -> Result<RunState> {
    let meta = run.meta()?;
    run.append(&RunEvent::Started {
        at: Utc::now(),
        pid: std::process::id(),
    })?;
    let checks = match prepare(project_root, &meta) {
        Ok(c) => c,
        Err(e) => {
            return run.append(&RunEvent::Failed {
                at: Utc::now(),
                reason: format!("{e:#}"),
            })
        }
    };
    run.append(&RunEvent::VerifyStarted { at: Utc::now() })?;
    match check(run, &meta, &checks, &infra_patterns(project_root))? {
        Some(end) => run.append(&end),
        None => seal(run, &meta),
    }
}

/// The worktree with every leaf output merged, and what to run in it.
fn prepare(project_root: &Path, meta: &RunMeta) -> Result<Checks> {
    let h = contract::load_handover(project_root, &format!("{}/{}", meta.intake, meta.handover))?;
    if h.manifest_sha256 != meta.manifest_sha256 {
        bail!("{} changed after the run was created", meta.handover);
    }
    let checks = meta
        .checks
        .clone()
        .ok_or_else(|| anyhow!("{} records no integration checks", meta.id))?;
    let wt = worktree::create_on(project_root, &meta.id, &meta.branch, meta.start_commit())?;
    if let Some(s) = &meta.start {
        start::apply(&wt.path, &meta.id, s)?;
    }
    Ok(checks)
}

/// Run the commands in order. `None` when all passed; otherwise the event
/// that ends the run. A command failing with output that names the
/// environment ([`infra_match`]) is run once more after [`INFRA_RETRY_DELAY`];
/// failing again, the run fails with an [`INFRA`] reason.
fn check(run: &Run, meta: &RunMeta, checks: &Checks, extra: &[String]) -> Result<Option<RunEvent>> {
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.dir.join(CHECKS_LOG))?;
    for cmd in &checks.commands {
        let mut retried: Option<String> = None;
        loop {
            if let Some(end) = interrupted() {
                return Ok(Some(end));
            }
            eprintln!("{}: integration — {cmd}", run.id);
            let (text, verdict) = run_check(meta, cmd)?;
            writeln!(
                log,
                "$ {cmd}\n{text}{}\n",
                if verdict.is_empty() { "ok" } else { &verdict }
            )?;
            if let Some(end) = interrupted() {
                return Ok(Some(end));
            }
            if verdict.is_empty() {
                if retried.is_some() {
                    writeln!(
                        log,
                        "(passed on retry: the first failure was the environment)\n"
                    )?;
                }
                break;
            }
            // A timeout is the check's own verdict, whatever it printed.
            let infra = (!verdict.starts_with("timed out"))
                .then(|| infra_match(&text, extra))
                .flatten();
            match (infra, &retried) {
                (Some(pattern), None) => {
                    writeln!(
                        log,
                        "(output matches infrastructure pattern \"{pattern}\"; retrying in {}s)\n",
                        INFRA_RETRY_DELAY.as_secs()
                    )?;
                    eprintln!(
                        "{}: integration — `{cmd}` failed on the environment (\"{pattern}\"), retrying in {}s",
                        run.id,
                        INFRA_RETRY_DELAY.as_secs()
                    );
                    std::thread::sleep(INFRA_RETRY_DELAY);
                    retried = Some(pattern);
                }
                (infra, _) => {
                    run.append(&RunEvent::VerifyFailed {
                        at: Utc::now(),
                        candidate: fingerprint(&meta.worktree),
                        failed_tests: vec![cmd.clone()],
                    })?;
                    let reason = match infra {
                        Some(pattern) if retried.is_some() => format!(
                            "{INFRA}integration check `{cmd}` failed twice on the environment ({verdict}, output matches \"{pattern}\"); the code was not judged:\n{}",
                            tail(&text, REASON_TAIL_LINES)
                        ),
                        _ => format!(
                            "integration check `{cmd}` failed ({verdict}{}):\n{}",
                            if retried.is_some() {
                                ", on a retry after an infrastructure failure"
                            } else {
                                ""
                            },
                            tail(&text, REASON_TAIL_LINES)
                        ),
                    };
                    return Ok(Some(RunEvent::Failed {
                        at: Utc::now(),
                        reason,
                    }));
                }
            }
        }
    }
    Ok(None)
}

/// Run one command in the worktree: its output and its verdict (empty when
/// it passed).
fn run_check(meta: &RunMeta, cmd: &str) -> Result<(String, String)> {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(cmd).current_dir(&meta.worktree);
    let out = crate::process::run_bounded(c, None, Duration::from_secs(CHECK_TIMEOUT_SECS))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let verdict = match (out.timed_out, out.status.and_then(|s| s.code())) {
        (true, _) => format!("timed out after {CHECK_TIMEOUT_SECS}s"),
        (false, Some(0)) => String::new(),
        (false, Some(code)) => format!("exit {code}"),
        (false, None) => "killed by a signal".to_string(),
    };
    Ok((text, verdict))
}

/// `execution.infra_patterns` of the project; none when its config cannot
/// be read (the built-in patterns still apply).
fn infra_patterns(project_root: &Path) -> Vec<String> {
    crate::config::load_from(&project_root.join(".zforge").join("config.yaml"))
        .map(|c| c.execution.infra_patterns)
        .unwrap_or_default()
}

/// The first infrastructure pattern — built in, then `extra` — found in
/// `output`, ignoring case.
pub(crate) fn infra_match(output: &str, extra: &[String]) -> Option<String> {
    let output = output.to_lowercase();
    INFRA_PATTERNS
        .iter()
        .map(|p| p.to_string())
        .chain(extra.iter().map(|p| p.trim().to_string()))
        .find(|p| !p.is_empty() && output.contains(&p.to_lowercase()))
}

/// Every command passed: seal the tested tree as the run's output.
fn seal(run: &Run, meta: &RunMeta) -> Result<RunState> {
    let Some(candidate) = fingerprint(&meta.worktree) else {
        return run.append(&RunEvent::Failed {
            at: Utc::now(),
            reason: "checks passed but the worktree could not be fingerprinted; not verified"
                .into(),
        });
    };
    let message = format!("zforge: integration of {} ({})", meta.handover, run.id);
    match output::seal(&meta.worktree, &message, &candidate) {
        Ok(commit) => run.append(&RunEvent::Verified {
            at: Utc::now(),
            candidate,
            commit: Some(commit),
            // An integration run has no agent and no test names to compare
            // against a known-failure list; it simply passes or does not.
            tolerated: Vec::new(),
            known_passing: Vec::new(),
        }),
        Err(e) => run.append(&RunEvent::Failed {
            at: Utc::now(),
            reason: format!("checks passed but the output was not sealed: {e:#}"),
        }),
    }
}

fn fingerprint(dir: &Path) -> Option<String> {
    crate::evidence::fingerprint(dir).hash().map(String::from)
}

/// A caught SIGINT/SIGTERM/SIGHUP ends the run `cancelled`.
fn interrupted() -> Option<RunEvent> {
    crate::process::take_interrupt().map(|sig| RunEvent::Cancelled {
        at: Utc::now(),
        reason: format!("interrupted by signal {sig}"),
    })
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_output_is_recognised_ignoring_case() {
        let docker = "Error: cannot connect to the docker daemon at unix:///var/run/docker.sock";
        assert_eq!(
            infra_match(docker, &[]).as_deref(),
            Some("Cannot connect to the Docker daemon")
        );
        let pg = "start postgres container from image postgres:16-alpine: run postgres c…";
        assert_eq!(
            infra_match(pg, &[]).as_deref(),
            Some("start postgres container")
        );
        assert_eq!(
            infra_match("dial tcp 10.0.0.1:443: I/O TIMEOUT", &[]).as_deref(),
            Some("i/o timeout")
        );
        assert_eq!(infra_match("--- FAIL: TestAdd (0.00s)", &[]), None);
        assert_eq!(infra_match("", &[]), None);
    }

    #[test]
    fn configured_patterns_add_to_the_built_in_ones() {
        let extra = vec![
            "  ".to_string(),
            "Database System Is Starting Up".to_string(),
        ];
        assert_eq!(
            infra_match("FATAL: the database system is starting up", &extra).as_deref(),
            Some("Database System Is Starting Up")
        );
        // A blank entry matches nothing.
        assert_eq!(infra_match("assertion failed", &extra), None);
        assert_eq!(
            infra_match("connection refused", &extra).as_deref(),
            Some("connection refused")
        );
    }
}
