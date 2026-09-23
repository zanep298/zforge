//! Readable views of a run (MOC-B TASK-005): the timeline and the result,
//! used by `zforge run status` and written to `progress.md` / `result.md`;
//! and of a whole handover (`feature`).
//! Generated from `run.yaml` and `events.jsonl`; editing them changes
//! nothing.

use super::feature::{FeatureState, Progress};
use super::record::{Run, RunEvent, RunMeta, RunState};
use anyhow::Result;
use std::fmt::Write as _;
use std::path::Path;

pub const PROGRESS: &str = "progress.md";
pub const RESULT: &str = "result.md";

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

/// One line per task and one for the integration check (MOC-C TASK-004).
pub fn feature(f: &FeatureState) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{} of {}", f.handover, f.intake);
    let width = f
        .tasks
        .iter()
        .map(|t| t.task.len())
        .chain([super::integrate::TASK.len()])
        .max()
        .unwrap_or(0);
    let rows = f
        .tasks
        .iter()
        .map(|t| (t.task.as_str(), &t.progress))
        .chain([(super::integrate::TASK, &f.integration)]);
    for (name, p) in rows {
        let _ = writeln!(
            out,
            "  {name:<width$}  {:<9} {}",
            p.label(),
            progress_detail(p)
        );
    }
    let _ = writeln!(
        out,
        "feature: {}",
        match &f.integration {
            Progress::Verified { run, .. } => format!("verified ({run})"),
            _ => "not verified".to_string(),
        }
    );
    out
}

fn progress_detail(p: &Progress) -> String {
    match p {
        Progress::Waiting { on } if on.is_empty() => String::new(),
        Progress::Waiting { on } => format!("on {}", on.join(", ")),
        Progress::Running { run } => run.clone(),
        Progress::Verified {
            run,
            commit,
            from,
            outdated,
            reused_from,
        } => {
            let mut d = run.clone();
            if let Some(r) = reused_from {
                let _ = write!(d, "  reuses {r}");
            }
            if let Some(c) = commit {
                let _ = write!(d, "  output {}", short(c));
            }
            if !from.is_empty() {
                let from: Vec<String> = from
                    .iter()
                    .map(|s| format!("{} ({})", s.task, s.run))
                    .collect();
                let _ = write!(d, "  from {}", from.join(" + "));
            }
            if !outdated.is_empty() {
                let _ = write!(d, "  — built on an older output of {}", outdated.join(", "));
            }
            d
        }
        Progress::Stopped { run, reason, .. } => match reason {
            Some(r) => format!("{run} — {}", r.lines().next().unwrap_or_default()),
            None => run.clone(),
        },
        Progress::Blocked { by } => format!("by {}", by.join(", ")),
    }
}

/// One line per event.
pub fn event_line(e: &RunEvent) -> String {
    let t = e.at().format("%H:%M:%S");
    match e {
        RunEvent::Started { pid, .. } => format!("{t} started (worker {pid})"),
        RunEvent::AttemptStarted {
            n, allotted_usd, ..
        } => {
            format!("{t} attempt {n} started (up to ${allotted_usd:.2})")
        }
        RunEvent::Attempt {
            n,
            exit_code,
            cost_usd,
            allotted_usd,
            ..
        } => format!(
            "{t} attempt {n} finished, exit {exit_code}, {}",
            match cost_usd {
                Some(c) => format!("${c:.2}"),
                None => format!("cost unknown (counted ${allotted_usd:.2})"),
            }
        ),
        RunEvent::VerifyStarted { .. } => format!("{t} verifying"),
        RunEvent::Verified {
            candidate, commit, ..
        } => format!(
            "{t} verified — candidate {}{}",
            short(candidate),
            output_note(commit.as_deref())
        ),
        RunEvent::VerifyFailed {
            candidate,
            failed_tests,
            ..
        } => format!(
            "{t} verification failed{}{}",
            candidate
                .as_deref()
                .map(|c| format!(" — candidate {}", short(c)))
                .unwrap_or_default(),
            if failed_tests.is_empty() {
                String::new()
            } else {
                format!(": {}", failed_tests.join(", "))
            }
        ),
        RunEvent::Blocked { reason, .. } => format!("{t} blocked — {reason}"),
        RunEvent::Failed { reason, .. } => format!("{t} failed — {reason}"),
        RunEvent::Cancelled { reason, .. } => format!("{t} cancelled — {reason}"),
        RunEvent::Reused {
            from_run,
            from_handover,
            candidate,
            commit,
            ..
        } => format!(
            "{t} reused {from_run} of {from_handover} — candidate {}, output {}",
            short(candidate),
            short(commit)
        ),
    }
}

fn output_note(commit: Option<&str>) -> String {
    commit
        .map(|c| format!(", output {}", short(c)))
        .unwrap_or_default()
}

/// Header lines: what the run is and where it stands.
pub fn summary(meta: &RunMeta, state: &RunState) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} — {} of {}/{}: {}{}",
        meta.id,
        meta.task,
        meta.intake,
        meta.handover,
        state.status.as_str(),
        state
            .reason
            .as_deref()
            .map(|r| format!(" ({r})"))
            .unwrap_or_default()
    );
    let _ = writeln!(out, "  branch {}", meta.branch);
    let _ = writeln!(out, "  worktree {}", meta.worktree.display());
    if let Some(start) = &meta.start {
        let from: Vec<String> = start
            .from
            .iter()
            .map(|s| format!("{} ({}, {})", s.task, s.run, short(&s.commit)))
            .collect();
        let _ = writeln!(out, "  starts from {}", from.join(" + "));
    }
    if let Some(checks) = &meta.checks {
        let from = format!("{:?}", checks.from).to_lowercase();
        let _ = writeln!(out, "  checks (from {from}):");
        for c in &checks.commands {
            let _ = writeln!(out, "    $ {c}");
        }
    }
    let _ = writeln!(
        out,
        "  baseline {}, manifest {}",
        short(&meta.baseline_commit),
        short(&meta.manifest_sha256)
    );
    let _ = writeln!(
        out,
        "  budget ${:.2}, spent ${:.2}{}; {} attempt(s), {} verification(s)",
        meta.budget_usd,
        state.cost_usd,
        state
            .in_flight_usd
            .map(|u| format!(" (incl. ${u:.2} for an attempt in flight)"))
            .unwrap_or_default(),
        state.attempts,
        state.verifications
    );
    if let Some(c) = &state.last_candidate {
        let _ = writeln!(out, "  last candidate {c}");
    }
    if let Some(o) = &state.output {
        let _ = writeln!(out, "  output {o} on {}", meta.branch);
    }
    if let Some(r) = &meta.retry_of {
        let _ = writeln!(out, "  retry of {r}");
    }
    out
}

pub fn timeline(events: &[RunEvent]) -> String {
    events
        .iter()
        .map(|e| format!("  {}\n", event_line(e)))
        .collect()
}

/// The contract the run was given, as the handover pins it. Read from the
/// manifest for display only — a run's own verification of those hashes
/// happened before it started.
fn contract_files(project_root: &Path, meta: &RunMeta) -> Vec<String> {
    let path = project_root
        .join(".zforge/intakes")
        .join(&meta.intake)
        .join(".records/handovers")
        .join(format!("{}.json", meta.handover));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<crate::intake::handover::Manifest>(&text) else {
        return Vec::new();
    };
    manifest
        .files
        .iter()
        .map(|f| format!("{} rev {} {}", f.file, f.revision, short(&f.sha256)))
        .collect()
}

/// Verifications, newest last, with the candidate each tested.
fn verifications(events: &[RunEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::Verified {
                candidate, commit, ..
            } => Some(format!(
                "passed — candidate {}{}",
                short(candidate),
                output_note(commit.as_deref())
            )),
            RunEvent::VerifyFailed {
                candidate,
                failed_tests,
                ..
            } => Some(format!(
                "failed{}{}",
                candidate
                    .as_deref()
                    .map(|c| format!(" — candidate {}", short(c)))
                    .unwrap_or_default(),
                if failed_tests.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", failed_tests.join(", "))
                }
            )),
            _ => None,
        })
        .collect()
}

/// Regenerate `progress.md` and `result.md`.
pub fn write(project_root: &Path, run: &Run, state: &RunState) -> Result<()> {
    let meta = run.meta()?;
    let events = run.events()?;
    let note =
        "<!-- Generated from run.yaml and events.jsonl by zforge; editing it changes nothing. -->";
    let progress = format!(
        "# {} progress\n\n{note}\n\n```text\n{}```\n",
        meta.id,
        timeline(&events)
    );
    crate::state::write_atomic(&run.dir.join(PROGRESS), progress.as_bytes())?;
    let mut result = format!(
        "# {} result\n\n{note}\n\n```text\n{}```\n\n## Contract\n\n",
        meta.id,
        summary(&meta, state)
    );
    for f in contract_files(project_root, &meta) {
        result.push_str(&format!("- {f}\n"));
    }
    result.push_str("\n## Verifications\n\n");
    let runs = verifications(&events);
    if runs.is_empty() {
        result.push_str("None.\n");
    }
    for (i, v) in runs.iter().enumerate() {
        result.push_str(&format!("{}. {v}\n", i + 1));
    }
    crate::state::write_atomic(&run.dir.join(RESULT), result.as_bytes())
}
