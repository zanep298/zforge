//! Readable views of a run (MOC-B TASK-005): the timeline and the result,
//! used by `zforge run status` and written to `progress.md` / `result.md`.
//! Generated from `run.yaml` and `events.jsonl`; editing them changes
//! nothing.

use super::record::{Run, RunEvent, RunMeta, RunState};
use anyhow::Result;
use std::fmt::Write as _;

pub const PROGRESS: &str = "progress.md";
pub const RESULT: &str = "result.md";

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
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
        RunEvent::Verified { candidate, .. } => {
            format!("{t} verified — candidate {}", short(candidate))
        }
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
    }
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

/// Regenerate `progress.md` and `result.md`.
pub fn write(run: &Run, state: &RunState) -> Result<()> {
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
    let result = format!(
        "# {} result\n\n{note}\n\n```text\n{}```\n",
        meta.id,
        summary(&meta, state)
    );
    crate::state::write_atomic(&run.dir.join(RESULT), result.as_bytes())
}
