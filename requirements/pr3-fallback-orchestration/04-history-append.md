# PR3-04 — Append fallback history to task state

## Goal

When a fallback fires, persist an immutable audit entry and swap `active_agent` — without mutating `assigned_agent`.

## Files touched

- `src/orchestrator/history.rs` (new)
- `src/orchestrator/mod.rs` — `pub mod history;`

## Implementation

```rust
use crate::orchestrator::fallback::FallbackReason;
use crate::state::{FallbackEntry, TaskState};
use anyhow::{anyhow, Result};
use chrono::Utc;

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
        return Err(anyhow!(
            "refusing to fall back from {from:?} to itself"
        ));
    }

    let entry = FallbackEntry {
        timestamp: Utc::now(),
        from: from.clone(),
        to: to.clone(),
        reason: reason.as_log_str(),
        phase: phase.to_string(),
    };
    state.fallback_history.push(entry.clone());
    state.active_agent = Some(to);
    // assigned_agent intentionally untouched.
    Ok(entry)
}

pub fn fallback_count(state: &TaskState) -> usize {
    state.fallback_history.len()
}
```

## Acceptance criteria

- [ ] `record_fallback` pushes an entry with `from`, `to`, `reason`, `phase`, and a timestamp.
- [ ] `record_fallback` sets `active_agent = fallback_agent`.
- [ ] `record_fallback` does NOT modify `assigned_agent`.
- [ ] Calling `record_fallback` twice on the same state appends two entries.
- [ ] `record_fallback` errors when `fallback_agent` is `None`.
- [ ] `record_fallback` errors when `from == to` (defensive — should never happen if PR2-02 validation is in place).
- [ ] `fallback_count` returns the number of entries.

## Depends on

- PR2-01 (TaskState fields), PR3-03 (FallbackReason).

## Notes

- Persistence to disk (atomic write of `.state.yaml`) is the caller's job — this function mutates the in-memory struct only. The caller (PR3-05) decides when to flush.
- Audit trail rule: `assigned_agent` is immutable for the lifetime of the task. Any debugging tool can answer "who was originally assigned?" from this field.

---

```yaml
status: done
owner: ""
pr: ""
```
