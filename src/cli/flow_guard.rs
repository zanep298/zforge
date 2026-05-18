use crate::state::{Flow, State, TaskState};
use anyhow::Result;

/// Refuse to dispatch a phase whose target state is not part of this task's
/// flow. E.g. `zf plan` on a fixbug task. Returned error names the flow so
/// users get a clear hint without inspecting `.state.yaml`.
pub fn ensure_phase_in_flow(ts: &TaskState, target: State, phase_name: &str) -> Result<()> {
    if !ts.flow.contains(&target) {
        anyhow::bail!(
            "phase '{}' is not part of the '{}' flow for task {}. \
             Run `zf status {}` to see the active flow",
            phase_name,
            ts.flow.as_str(),
            ts.task_id,
            ts.task_id
        );
    }
    Ok(())
}

/// Gate that requires the FSM to have reached the immediate predecessor of
/// `target` within the task's flow. Replaces hardcoded `state >= X` checks
/// that were written for the legacy full flow.
pub fn ensure_predecessor_complete(ts: &TaskState, target: State) -> Result<()> {
    let Some(prev) = ts.flow.previous_of(&target) else {
        return Ok(()); // target is the first state in the flow
    };
    if ts.state < *prev {
        anyhow::bail!(
            "state is {}, need {} (predecessor of {} in '{}' flow). Next: {}",
            ts.state.as_str(),
            prev.as_str(),
            target.as_str(),
            ts.flow.as_str(),
            Flow::Full
                .next_after(&ts.state)
                .map(|s| crate::state::dispatch_command(s, &ts.task_id))
                .unwrap_or_else(|| "task complete".into())
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_with(flow: Flow, state: State) -> TaskState {
        let mut ts = TaskState::new_with_flow("TASK-1", flow);
        // Walk to the requested state via flow-aware advance.
        while ts.state != state {
            let Some(next) = ts.flow.next_after(&ts.state) else {
                break;
            };
            ts.advance(next.clone(), "test").unwrap();
        }
        ts
    }

    #[test]
    fn ensure_phase_rejects_phase_outside_flow() {
        let ts = task_with(Flow::Fixbug, State::Imported);
        let err = ensure_phase_in_flow(&ts, State::Planned, "plan").unwrap_err();
        assert!(err.to_string().contains("'plan'"));
        assert!(err.to_string().contains("fixbug"));
    }

    #[test]
    fn ensure_phase_accepts_phase_in_flow() {
        let ts = task_with(Flow::Fixbug, State::Imported);
        ensure_phase_in_flow(&ts, State::SpecDone, "spec").unwrap();
        ensure_phase_in_flow(&ts, State::Coded, "code").unwrap();
    }

    #[test]
    fn predecessor_complete_passes_when_at_predecessor() {
        let ts = task_with(Flow::Fixbug, State::TestspecDone);
        ensure_predecessor_complete(&ts, State::Coded).unwrap();
    }

    #[test]
    fn predecessor_complete_fails_when_behind() {
        let ts = task_with(Flow::Fixbug, State::Imported);
        let err = ensure_predecessor_complete(&ts, State::Coded).unwrap_err();
        assert!(err.to_string().contains("need TestspecDone"));
    }

    #[test]
    fn predecessor_complete_passes_for_first_state() {
        let ts = task_with(Flow::Docs, State::Imported);
        ensure_predecessor_complete(&ts, State::Imported).unwrap();
    }

    #[test]
    fn fixbug_code_gate_passes_after_testspec() {
        let ts = task_with(Flow::Fixbug, State::TestspecDone);
        ensure_predecessor_complete(&ts, State::Coded).unwrap();
    }

    #[test]
    fn spike_code_gate_passes_after_spec() {
        let ts = task_with(Flow::Spike, State::SpecDone);
        ensure_predecessor_complete(&ts, State::Coded).unwrap();
    }
}
