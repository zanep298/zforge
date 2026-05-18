use crate::cli;
use crate::cli::flow_guard;
use crate::config;
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{dispatch_command, Flow, State, TaskState};
use anyhow::Result;
use colored::Colorize;

/// FSM gate for the `ship` command. Returns `Err` when the task has not yet
/// reached the state immediately preceding `Coded` in its flow (e.g. for
/// the full flow that is `PlanReviewed`; for fixbug it is `TestspecDone`).
pub(crate) fn check_ship_gate(flow: &Flow, state: &State, task_id: &str) -> Result<()> {
    if !flow.contains(&State::Coded) {
        anyhow::bail!(
            "flow '{}' has no code phase — ship is not applicable",
            flow.as_str()
        );
    }
    let Some(prev) = flow.previous_of(&State::Coded) else {
        return Ok(());
    };
    if state < prev {
        anyhow::bail!(
            "BLOCKED: {} required before ship (flow: {}). Run: {}",
            prev.as_str(),
            flow.as_str(),
            dispatch_command(prev, task_id)
        );
    }
    Ok(())
}

/// Idempotent advance to `Coded`. Both the CLI `ship` (after dispatching the
/// code agent) and the MCP `ship` tool (the LLM has already written code)
/// call this before running verify. Returns `Ok(true)` when the FSM was
/// advanced this call, `Ok(false)` when it was already past `Coded`.
pub(crate) fn ensure_coded_state(
    ts: &mut TaskState,
    tasks_dir: &std::path::Path,
    note: &str,
) -> Result<bool> {
    if ts.state < State::Coded {
        ts.advance(State::Coded, note)?;
        ts.save(tasks_dir)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Run code + verify back-to-back. Idempotent: skips code if state already Coded.
///
/// Saves orchestrator round trips when chained via MCP; saves typing when run from a shell.
pub fn run(task_id: &str, command: Option<String>, timeout: u64) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    flow_guard::ensure_phase_in_flow(&ts, State::Coded, "code")?;

    if let Err(e) = check_ship_gate(&ts.flow, &ts.state, task_id) {
        eprintln!("{} {}", "⛔".red(), e);
        return Ok(());
    }

    if ts.state < State::Coded {
        let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Code)?;
        ctx.next_command = format!("(auto) zf verify {}", task_id);

        let engine = Engine::new(&config.agents_dir());
        engine.dispatch("code", &ctx)?;

        let prev = ts.state.as_str().to_string();
        if ensure_coded_state(&mut ts, &tasks_dir, "code phase complete (ship)")? {
            println!("{} State advanced: {} → Coded", "✓".green(), prev);
            println!();
        }
    } else {
        println!(
            "{} State already {}, skipping code phase",
            "ℹ".blue(),
            ts.state.as_str()
        );
    }

    cli::verify::run(task_id, command, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_blocks_states_before_plan_reviewed() {
        for blocked in [
            State::Imported,
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
        ] {
            let err = check_ship_gate(&Flow::Full, &blocked, "TASK-1").unwrap_err();
            assert!(
                err.to_string().contains("BLOCKED"),
                "state {blocked:?} should block ship but message was: {err}"
            );
            assert!(err.to_string().contains("TASK-1"));
        }
    }

    #[test]
    fn fixbug_gate_passes_after_testspec() {
        check_ship_gate(&Flow::Fixbug, &State::TestspecDone, "TASK-1").unwrap();
    }

    #[test]
    fn fixbug_gate_blocks_before_testspec() {
        let err = check_ship_gate(&Flow::Fixbug, &State::SpecDone, "TASK-1").unwrap_err();
        assert!(err.to_string().contains("TestspecDone"));
        assert!(err.to_string().contains("fixbug"));
    }

    #[test]
    fn ensure_coded_advances_when_below() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut ts = TaskState::new("TASK-1");
        for next in [
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
            State::PlanReviewed,
        ] {
            ts.advance(next, "").unwrap();
        }
        ts.save(tmp.path()).unwrap();

        let advanced = ensure_coded_state(&mut ts, tmp.path(), "ship test").unwrap();
        assert!(advanced);
        assert_eq!(ts.state, State::Coded);
        let reloaded = TaskState::load(tmp.path(), "TASK-1").unwrap();
        assert_eq!(reloaded.state, State::Coded);
    }

    #[test]
    fn ensure_coded_is_idempotent_when_already_coded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut ts = TaskState::new("TASK-1");
        for next in [
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
            State::PlanReviewed,
            State::Coded,
        ] {
            ts.advance(next, "").unwrap();
        }
        ts.save(tmp.path()).unwrap();

        let advanced = ensure_coded_state(&mut ts, tmp.path(), "ship test").unwrap();
        assert!(!advanced);
        assert_eq!(ts.state, State::Coded);
    }

    #[test]
    fn gate_allows_states_at_or_past_plan_reviewed() {
        for ok in [
            State::PlanReviewed,
            State::Coded,
            State::Verified,
            State::Reviewed,
        ] {
            check_ship_gate(&Flow::Full, &ok, "TASK-1")
                .unwrap_or_else(|e| panic!("state {ok:?} should pass gate but errored: {e}"));
        }
    }
}
