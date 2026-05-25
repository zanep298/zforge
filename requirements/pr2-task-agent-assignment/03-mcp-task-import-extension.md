# PR2-03 — Extend MCP `task_import` tool

## Goal

Allow MCP callers (Hermes, other agents) to pass `agent` and `fallback` when importing tasks.

## Files touched

- `src/mcp/mod.rs` — extend the `task_import` tool handler

## Implementation

Locate the existing `task_import` handler. Add to the parameter parsing:

```rust
let agent: Option<String> = params.get("agent")
    .and_then(|v| v.as_str())
    .map(String::from);
let fallback: Option<String> = params.get("fallback")
    .and_then(|v| v.as_str())
    .map(String::from);
```

Pass these into the underlying `cli::task::import` call. Because the CLI handler now performs registry validation, MCP gets the same error paths automatically.

### Updated MCP tool schema

```json
{
  "name": "task_import",
  "description": "Import a task into the active project",
  "inputSchema": {
    "type": "object",
    "properties": {
      "id": { "type": "string" },
      "title": { "type": "string" },
      "flow": {
        "type": "string",
        "enum": ["full", "fixbug", "spike", "docs"]
      },
      "agent": {
        "type": "string",
        "description": "Primary agent name from registry's agents{} map"
      },
      "fallback": {
        "type": "string",
        "description": "Fallback agent name from registry's agents{} map"
      }
    },
    "required": ["id"]
  }
}
```

## Acceptance criteria

- [ ] MCP `task_import` with `{"id": "T1", "agent": "claude"}` creates a task with `assigned_agent: claude`.
- [ ] MCP `task_import` with an unknown agent returns an MCP error containing the registry hint.
- [ ] MCP `task_import` with matching `agent` and `fallback` returns an MCP error.
- [ ] MCP `task_import` without agent fields preserves prior behavior.
- [ ] Schema discovery (`tools/list`) shows the new fields.

## Depends on

- PR2-02.

## Notes

- Reuse `cli::task::import` — do not duplicate validation logic in the MCP layer.
- This task is small but blocks orchestrator-side automation in PR 3 — agents need to assign teammates at task creation.

---

```yaml
status: done
owner: ""
pr: ""
```
