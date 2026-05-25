# PR2-04 — Integration tests

## Goal

Verify task agent assignment behavior end-to-end via CLI and MCP.

## Files touched

- `tests/task_import_agent_test.rs` (new)
- Optionally extend `tests/common/mod.rs` with a `make_registry_with_agents` helper.

## Test cases

### CLI

- [ ] `task_import_with_agent_persists_field` — `cli::task::import("T1", agent=Some("claude"))` → `.state.yaml` has `assigned_agent: "claude"` and `active_agent: "claude"`.
- [ ] `task_import_with_unknown_agent_rejected` — error contains the agent name and the word "registry" or "agents".
- [ ] `task_import_agent_eq_fallback_rejected` — error contains "must differ".
- [ ] `task_import_without_flags_unchanged` — pre-existing tests still pass with new optional fields defaulting to `None`.
- [ ] `task_import_fallback_only_rejected_or_allowed` — decide policy: spec currently allows `--fallback` without `--agent`? Re-read spec; this task assumes allowed (defaults primary to detected agent from `init`). If we want to reject, add the test for the rejection. Recommendation: **allow fallback only**, defaulting primary to the `init`-detected agent.
- [ ] `legacy_state_yaml_loads_without_new_fields` — write a `.state.yaml` without any of the new fields, load it, assert no error and all new fields default sensibly.

### MCP

- [ ] `mcp_task_import_with_agent` — send a JSON-RPC `tools/call` for `task_import` with `agent` set; assert the resulting state file.
- [ ] `mcp_task_import_unknown_agent_returns_error` — assert MCP error response shape.
- [ ] `mcp_tools_list_includes_agent_fields` — assert `tools/list` schema mentions `agent` and `fallback`.

## Acceptance criteria

- [ ] All listed cases exist and pass.
- [ ] `cargo test` clean.
- [ ] `cargo clippy --tests -- -D warnings` clean.

## Depends on

- PR2-01, PR2-02, PR2-03.

## Notes

- Tests share registry setup helpers with PR 1 tests — reuse `ZFORGE_HOME` env override.
- MCP tests should drive the stdio transport rather than calling handler functions directly when feasible, to catch regressions in JSON-RPC plumbing.

---

```yaml
status: done
owner: ""
pr: ""
```
