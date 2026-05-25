use crate::orchestrator::fallback::FallbackReason;
use crate::state::{FallbackEntry, TaskState};
use anyhow::{anyhow, Result};
use chrono::Utc;

/// Append a `FallbackEntry`, swap `active_agent` to the configured fallback,
/// and leave `assigned_agent` untouched. Persistence is the caller's job —
/// this only mutates the in-memory struct.
pub fn record_fallback(
    state: &mut TaskState,
    reason: &FallbackReason,
    phase: &str,
) -> Result<FallbackEntry> {
    let from = state
        .active_agent
        .clone()
        .or_else(|| state.assigned_agent.clone())
        .ok_or_else(|| anyhow!("task has no active_agent or assigned_agent"))?;
    let to = state
        .fallback_agent
        .clone()
        .ok_or_else(|| anyhow!("task has no fallback_agent configured"))?;

    if from == to {
        return Err(anyhow!("refusing to fall back from {from:?} to itself"));
    }

    let entry = FallbackEntry {
        timestamp: Utc::now(),
        from,
        to: to.clone(),
        reason: reason.as_log_str(),
        phase: phase.to_string(),
    };
    state.fallback_history.push(entry.clone());
    state.active_agent = Some(to);
    // assigned_agent intentionally untouched — audit trail invariant.
    Ok(entry)
}

pub fn fallback_count(state: &TaskState) -> usize {
    state.fallback_history.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TaskState;

    fn task_with_agents(primary: &str, fallback: Option<&str>) -> TaskState {
        let mut ts = TaskState::new("TASK-1");
        ts.assigned_agent = Some(primary.into());
        ts.active_agent = Some(primary.into());
        ts.fallback_agent = fallback.map(str::to_string);
        ts
    }

    #[test]
    fn records_swap_and_preserves_assigned_agent() {
        let mut ts = task_with_agents("claude", Some("codex"));
        let entry = record_fallback(&mut ts, &FallbackReason::ExitCode(124), "code").unwrap();
        assert_eq!(entry.from, "claude");
        assert_eq!(entry.to, "codex");
        assert_eq!(entry.reason, "exit_code:124");
        assert_eq!(entry.phase, "code");
        assert_eq!(ts.assigned_agent.as_deref(), Some("claude"));
        assert_eq!(ts.active_agent.as_deref(), Some("codex"));
        assert_eq!(ts.fallback_history.len(), 1);
    }

    #[test]
    fn record_twice_appends_two_entries() {
        let mut ts = task_with_agents("claude", Some("codex"));
        record_fallback(&mut ts, &FallbackReason::ExitCode(124), "code").unwrap();
        // Second swap would normally need a new fallback target — schema only
        // supports one-step fallback. For this test we only check append.
        ts.fallback_agent = Some("opencode".into());
        record_fallback(&mut ts, &FallbackReason::ExitCode(137), "code").unwrap();
        assert_eq!(ts.fallback_history.len(), 2);
        assert_eq!(ts.active_agent.as_deref(), Some("opencode"));
    }

    #[test]
    fn errors_without_fallback_agent() {
        let mut ts = task_with_agents("claude", None);
        let err = record_fallback(&mut ts, &FallbackReason::ExitCode(124), "code").unwrap_err();
        assert!(err.to_string().contains("fallback_agent"));
    }

    #[test]
    fn errors_when_from_equals_to() {
        let mut ts = task_with_agents("claude", Some("claude"));
        let err = record_fallback(&mut ts, &FallbackReason::ExitCode(124), "code").unwrap_err();
        assert!(err.to_string().contains("itself"));
    }

    #[test]
    fn fallback_count_matches_history_len() {
        let mut ts = task_with_agents("claude", Some("codex"));
        assert_eq!(fallback_count(&ts), 0);
        record_fallback(&mut ts, &FallbackReason::ExitCode(124), "code").unwrap();
        assert_eq!(fallback_count(&ts), 1);
    }
}
