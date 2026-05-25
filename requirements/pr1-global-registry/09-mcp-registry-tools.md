# PR1-09 — MCP tools for registry

## Goal

Expose registry CRUD and global status as MCP tools so orchestrator agents (Hermes etc.) can drive them via stdio.

## Files touched

- `src/mcp/mod.rs` — register new tools, route to existing `cli::project::*` / `cli::status::*` functions

## Implementation

Register five new tools (no `zforge_` prefix — matches the existing `task_import`, `get_prompt`, `approve`, `verify`, `ship`, `status` naming):

| Tool name        | Args (JSON)                                | Delegates to                                  |
| ---------------- | ------------------------------------------ | --------------------------------------------- |
| `project_list`   | `{ "json": bool? }`                        | `cli::project::run(ProjectCmd::List(...))`    |
| `project_add`    | `{ "name": string, "path": string }`       | `cli::project::run(ProjectCmd::Add(...))`     |
| `project_remove` | `{ "name": string, "purge": bool? }`       | `cli::project::run(ProjectCmd::Remove(...))`  |
| `switch_project` | `{ "name": string }`                       | `cli::project::run(ProjectCmd::Switch(...))`  |
| `global_status`  | `{ "timeout_ms": u64?, "json": bool? }`    | `cli::status::run_global(...)`                |

### Response shape

Each tool should return JSON conforming to MCP tool result spec. For tools that produce structured data (list, global_status), capture stdout to a string and return as `{"content": [{"type": "text", "text": "..."}]}` OR — preferred — call the underlying function in a "library mode" that returns the structured value directly, then serialize to JSON for the MCP response.

Recommended: refactor `cli::project::list` and `cli::status::run_global` to return their data structures, with the CLI side handling the print. The MCP side serializes the data directly.

Example refactor:

```rust
// cli/project.rs
pub fn list_data() -> Result<Registry> {
    crate::registry::io::load()
}

fn list(a: ListArgs) -> Result<()> {
    let r = list_data()?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&r)?);
    } else {
        print_table(&r);
    }
    Ok(())
}
```

Then in MCP:

```rust
"project_list" => {
    let data = cli::project::list_data()?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&data)?
        }]
    }))
}
```

## Acceptance criteria

- [ ] `project_list` MCP tool returns the same JSON shape as `zforge project list --json`.
- [ ] `project_add` MCP tool with invalid path returns an MCP error (not a panic).
- [ ] `project_add` MCP tool with valid args persists to disk; visible on next `project_list` call.
- [ ] `project_remove` with `"purge": true` deletes `.zforge/` (require `yes: true` semantics — MCP always passes `yes`).
- [ ] `switch_project` updates `current_project`; reading `project_list` afterwards reflects the change.
- [ ] `global_status` returns a JSON array of task rows; honors `timeout_ms`.
- [ ] All tools propagate registry validation errors (duplicate name, bad regex) as MCP error responses with descriptive messages.

## Depends on

- PR1-06, PR1-08.

## Notes

- Per the existing CLAUDE.md invariant: MCP tools delegate directly to `cli::*` functions — do not create parallel code paths.
- The `task_import` MCP tool will be extended later in PR 2 (separate task) to accept `agent` and `fallback`.

---

```yaml
status: done
owner: ""
pr: ""
```
