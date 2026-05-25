# PR2-01 — Extend `TaskState` with agent fields

## Goal

Add `assigned_agent`, `fallback_agent`, `active_agent`, and `fallback_history` to the per-task state file. All optional/defaulted for backward compatibility with existing `.state.yaml` files.

## Files touched

- `src/state/mod.rs`

## Implementation

### New types

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackEntry {
    pub timestamp: DateTime<Utc>,
    pub from: String,
    pub to: String,
    pub reason: String,
    pub phase: String,
}
```

### Extend `TaskState`

Locate the existing `TaskState` struct in `src/state/mod.rs` and add (all with `#[serde(default)]`):

```rust
#[serde(default)]
pub assigned_agent: Option<String>,

#[serde(default)]
pub fallback_agent: Option<String>,

#[serde(default)]
pub active_agent: Option<String>,

#[serde(default)]
pub fallback_history: Vec<FallbackEntry>,
```

### Helper

```rust
impl TaskState {
    /// The agent that should be invoked right now — `active_agent` if set,
    /// otherwise `assigned_agent`, otherwise `None`.
    pub fn effective_agent(&self) -> Option<&str> {
        self.active_agent
            .as_deref()
            .or(self.assigned_agent.as_deref())
    }
}
```

## Acceptance criteria

- [ ] An existing `.state.yaml` without any of the new fields still loads successfully and yields `None` / empty values.
- [ ] A `.state.yaml` with all new fields populated round-trips losslessly.
- [ ] `effective_agent` returns `active_agent` when set, falls back to `assigned_agent` when not, returns `None` when both absent.
- [ ] `cargo test` passes existing state tests after the schema change.
- [ ] `cargo clippy -- -D warnings` passes.

## Depends on

- PR1-01 (chrono dependency added).

## Notes

- Do not gate new field reads on a feature flag — `#[serde(default)]` makes them always-on for new tasks and harmless for old.
- `effective_agent` lives on `TaskState` itself; the orchestrator (PR 3) will call it.
- This task does not add any logic that reads the fields — purely schema. CLI changes come in PR2-02.

---

```yaml
status: done
owner: ""
pr: ""
```
