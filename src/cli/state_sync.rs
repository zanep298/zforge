//! Artifact-derived FSM reconciliation for the MCP flow.
//!
//! The CLI drives the pipeline with explicit `--done` commands that advance
//! the FSM. The MCP flow has no such signal: `get_prompt` hands a prompt to
//! the driving LLM, which writes the artifact itself, but nothing marks the
//! generate-phase complete. Without reconciliation a pure-MCP run never leaves
//! `Imported` and `approve` rejects every gate.
//!
//! `sync_from_artifacts` closes that gap: it walks the task's flow and
//! advances the FSM to match the generate-phase artifacts present on disk,
//! stopping before any human-review gate (still crossed by `approve`) and
//! before code/verify/review (driven by `ship`/`verify`/`review`). State
//! becomes a function of artifacts-on-disk plus explicit approvals — the same
//! end state whether the pipeline was driven by CLI or MCP.

use crate::fs::reader;
use crate::state::{State, TaskState};
use anyhow::Result;
use std::path::Path;

/// Artifact whose presence implies a generate-phase state is complete. Review
/// gates and code/verify/review states return `None` — they are not
/// artifact-derived and must be crossed by `approve` / `ship` / `verify` /
/// `review`.
fn artifact_for_state(state: &State) -> Option<&'static str> {
    match state {
        State::SpecDone => Some("spec.md"),
        State::TestspecDone => Some("testspec.md"),
        State::Planned => Some("plan.md"),
        _ => None,
    }
}

/// Advance `ts` through consecutive generate-phase states whose artifacts
/// already exist (and are non-empty) on disk. Returns `true` if the state
/// moved so the caller can persist. Stops at the first state that is a review
/// gate, a non-generate state, or whose artifact is missing.
pub(crate) fn sync_from_artifacts(
    ts: &mut TaskState,
    tasks_dir: &Path,
    task_id: &str,
) -> Result<bool> {
    let mut advanced = false;
    while let Some(next) = ts.flow.next_after(&ts.state) {
        let Some(artifact) = artifact_for_state(next) else {
            break;
        };
        if !reader::artifact_exists(tasks_dir, task_id, artifact) {
            break;
        }
        ts.advance(next.clone(), "auto-advanced from artifact")?;
        advanced = true;
    }
    Ok(advanced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Flow;
    use std::fs;

    fn write_artifact(tasks_dir: &Path, task_id: &str, name: &str) {
        let dir = tasks_dir.join(task_id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), "# content\nreal line\n").unwrap();
    }

    fn task_state(flow: Flow) -> TaskState {
        TaskState::new_with_flow("T1", flow)
    }

    #[test]
    fn advances_through_present_generate_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path();
        write_artifact(tasks, "T1", "spec.md");
        write_artifact(tasks, "T1", "testspec.md");

        let mut ts = task_state(Flow::Full);
        let moved = sync_from_artifacts(&mut ts, tasks, "T1").unwrap();

        assert!(moved);
        // spec.md + testspec.md present → advances Imported → SpecDone →
        // TestspecDone, then stops before the TestspecReviewed gate.
        assert_eq!(ts.state, State::TestspecDone);
    }

    #[test]
    fn stops_at_missing_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path();
        write_artifact(tasks, "T1", "spec.md"); // no testspec.md

        let mut ts = task_state(Flow::Full);
        let moved = sync_from_artifacts(&mut ts, tasks, "T1").unwrap();

        assert!(moved);
        assert_eq!(ts.state, State::SpecDone);
    }

    #[test]
    fn no_op_when_no_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ts = task_state(Flow::Full);
        let moved = sync_from_artifacts(&mut ts, tmp.path(), "T1").unwrap();

        assert!(!moved);
        assert_eq!(ts.state, State::Imported);
    }

    #[test]
    fn stops_before_review_gate_even_with_all_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path();
        for f in ["spec.md", "testspec.md", "plan.md"] {
            write_artifact(tasks, "T1", f);
        }

        let mut ts = task_state(Flow::Full);
        sync_from_artifacts(&mut ts, tasks, "T1").unwrap();

        // testspec.md present would derive TestspecDone, but crossing to
        // TestspecReviewed needs approve — so sync halts at TestspecDone
        // regardless of plan.md also being present.
        assert_eq!(ts.state, State::TestspecDone);
    }

    #[test]
    fn honors_short_flow_without_review_gates() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path();
        write_artifact(tasks, "T1", "spec.md");
        write_artifact(tasks, "T1", "testspec.md");

        // Fixbug: Imported → SpecDone → TestspecDone → Coded → Verified (no
        // review gate between testspec and code).
        let mut ts = task_state(Flow::Fixbug);
        sync_from_artifacts(&mut ts, tasks, "T1").unwrap();

        // Advances to TestspecDone; next is Coded (not artifact-derived) → stop.
        assert_eq!(ts.state, State::TestspecDone);
    }
}
