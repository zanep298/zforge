//! The integration run (MOC-C TASK-003, REQ-004).
//!
//! Every task of a handover can pass on its own branch while the feature
//! does not work as a whole. An integration run checks the whole: a
//! worktree holding the output of every leaf task — tasks no other
//! handed-over task depends on, whose outputs hold all the others' — and
//! the integration check the user accepted, run command by command. No
//! agent is called and no budget is spent: an integration failure is a
//! question about the contract between tasks, answered through an
//! amendment.
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
    let checks = h.checks(&config.project.test_command)?;
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
        }),
        kind: RunKind::Integration,
        checks: Some(checks.clone()),
    })
}

/// The outputs of the leaf tasks, after checking every task has one.
fn leaf_outputs(project_root: &Path, h: &Handover) -> Result<Vec<Source>> {
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
    match check(run, &meta, &checks)? {
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
/// that ends the run.
fn check(run: &Run, meta: &RunMeta, checks: &Checks) -> Result<Option<RunEvent>> {
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.dir.join(CHECKS_LOG))?;
    for cmd in &checks.commands {
        if let Some(end) = interrupted() {
            return Ok(Some(end));
        }
        eprintln!("{}: integration — {cmd}", run.id);
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
        writeln!(
            log,
            "$ {cmd}\n{text}{}\n",
            if verdict.is_empty() { "ok" } else { &verdict }
        )?;
        if let Some(end) = interrupted() {
            return Ok(Some(end));
        }
        if !verdict.is_empty() {
            run.append(&RunEvent::VerifyFailed {
                at: Utc::now(),
                candidate: fingerprint(&meta.worktree),
                failed_tests: vec![cmd.clone()],
            })?;
            return Ok(Some(RunEvent::Failed {
                at: Utc::now(),
                reason: format!(
                    "integration check `{cmd}` failed ({verdict}):\n{}",
                    tail(&text, REASON_TAIL_LINES)
                ),
            }));
        }
    }
    Ok(None)
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
