# PR3-06 — Route phase commands through the orchestrator

## Goal

Replace direct calls to `Engine::dispatch()` in the LLM-driven phase commands with `orchestrator::run_phase`. `verify` stays as-is (it shells out to a test runner, never an LLM).

## Files touched

- `src/cli/spec.rs`
- `src/cli/testspec.rs`
- `src/cli/plan.rs`
- `src/cli/code.rs`
- `src/cli/ship.rs`

## Implementation

For each of `spec.rs`, `testspec.rs`, `plan.rs`, `code.rs`:

1. Find the existing dispatch site (look for `Engine::dispatch`, `prompt::engine::dispatch`, or equivalent).
2. Replace with:

```rust
let project_root = config.project_root()?;
crate::orchestrator::run_phase(&task_id, "<phase_name>", &project_root)
    .with_context(|| format!("run {phase} for task {task_id}"))?;
```

Where `<phase_name>` is `"spec"`, `"testspec"`, `"plan"`, or `"code"`.

For `ship.rs`: only the implicit `code` step routes through the orchestrator. The subsequent `verify` step continues to call `runner::run_with_language` directly.

### Backward compat for tasks with no `assigned_agent`

If `effective_agent()` returns `None`, the orchestrator currently errors out. Two options:

1. Error out — forces users to migrate by re-importing.
2. Fall back to the agent detected at `init` time (recorded in `.zforge/config.yaml`).

Recommended: **option 2** for backward compatibility. Add a helper in the orchestrator:

```rust
fn resolve_agent_name(state: &TaskState, config: &Config) -> Result<String> {
    if let Some(name) = state.effective_agent() {
        return Ok(name.to_string());
    }
    config
        .default_agent()
        .ok_or_else(|| anyhow!("no agent assigned and no init-time default available"))
}
```

`Config::default_agent` reads the agent recorded in `.zforge/config.yaml` (existing field; if missing, hard error with a clear migration message).

## Acceptance criteria

- [ ] `zforge spec T1` for a task with `assigned_agent: claude` spawns the `claude` binary via the orchestrator.
- [ ] `zforge code T1` triggers the fallback path on a retryable error (verified with the fake_agent stub).
- [ ] `zforge ship T1` runs the orchestrator for the code step and the test runner for the verify step.
- [ ] Tasks created before PR 2 (no agent fields) still run via the init-time default agent.
- [ ] `zforge verify T1` does NOT touch the orchestrator.
- [ ] Errors from the orchestrator propagate with a non-zero exit code and a clear stderr message.

## Depends on

- PR3-05.

## Notes

- Avoid touching `flow_guard.rs` — the orchestrator is invoked AFTER flow gating in each CLI command, not instead of it.
- Keep the `run_phase` call narrow — pass only `task_id`, `phase`, `project_root`. The orchestrator pulls everything else from disk.

---

```yaml
status: done
owner: ""
pr: ""
```
