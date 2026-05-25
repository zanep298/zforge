# PR2-02 — `task import --agent` and `--fallback` flags

## Goal

Allow users to assign a primary and fallback agent at task creation time. Validate against the global registry.

## Files touched

- `src/cli/task.rs`

## Implementation

### New flags

Add to the existing `Import` subcommand args:

```rust
/// Primary agent assigned to this task — must exist in registry's agents{} map.
#[arg(long)]
pub agent: Option<String>,

/// Fallback agent if the primary fails with a retryable error.
#[arg(long)]
pub fallback: Option<String>,
```

### Validation in the import handler

After computing the task ID and before writing `.state.yaml`:

```rust
let registry = crate::registry::io::load()?;

if let Some(name) = args.agent.as_deref() {
    if !registry.agents.contains_key(name) {
        anyhow::bail!(
            "unknown agent {name:?}: not in ~/.zforge/registry.yaml agents{{}} map"
        );
    }
}
if let Some(name) = args.fallback.as_deref() {
    if !registry.agents.contains_key(name) {
        anyhow::bail!(
            "unknown fallback agent {name:?}: not in registry"
        );
    }
}
if let (Some(a), Some(f)) = (args.agent.as_deref(), args.fallback.as_deref()) {
    if a == f {
        anyhow::bail!("--agent and --fallback must differ; both are {a:?}");
    }
}
```

### Persisting to state

When constructing the initial `TaskState`:

```rust
state.assigned_agent = args.agent.clone();
state.fallback_agent = args.fallback.clone();
state.active_agent = args.agent.clone();
```

`active_agent` is initialized to the same value as `assigned_agent`. The orchestrator (PR 3) mutates `active_agent` on fallback; `assigned_agent` stays immutable.

## Acceptance criteria

- [ ] `zforge task import T1 --title "x" --agent claude` writes `.state.yaml` with `assigned_agent: claude` and `active_agent: claude`.
- [ ] `zforge task import T1 --agent unknown` exits non-zero with a clear error mentioning the registry.
- [ ] `zforge task import T1 --agent claude --fallback claude` exits non-zero with "must differ" error.
- [ ] `zforge task import T1` (no flags) still works; all three new fields remain `None`.
- [ ] Importing without `--fallback` but with `--agent` is allowed — `fallback_agent: None`.
- [ ] Existing import behavior (Jira fetch, flow selection) is unaffected.

## Depends on

- PR1-02 (registry schema), PR2-01 (state extension).

## Notes

- Validation deliberately happens before any file mutation — task scaffold should not be created if the agent name is bad.
- An empty `agents{}` in the registry means any `--agent` flag will fail. Document this in the error message.

---

```yaml
status: done
owner: ""
pr: ""
```
