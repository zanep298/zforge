# .codex/

Project-scoped artifacts for the OpenAI Codex CLI.

## Files

| File | Purpose |
|------|---------|
| `agents/*.md` | Codex-specific workflow agent prompts materialized from zforge templates |

## Configuration

Codex CLI is configured globally at `~/.codex/config.toml`. zforge registers its
MCP server there with `zforge mcp register --agent codex`, which adds:

```toml
[mcp_servers.zforge]
command = "zforge"
args = ["mcp"]
```

Once registered, Codex sessions can call zforge tools (`task_import`,
`get_prompt`, `approve`, `status`, `verify`, `ship`).

## Permissions / Allowlists

Codex CLI controls command execution through its sandbox and approval policy in
`~/.codex/config.toml` (e.g. `approval_policy`, `[shell]` block). There is no
project-scoped permissions file equivalent to `.claude/settings.json` at this
time. Check the current Codex CLI docs for the supported keys before relying on
any specific approval flow.

## Quick Reference

```bash
zforge mcp register --agent codex   # add/refresh MCP entry
zforge status <TASK-ID>             # check pipeline state
zforge spec <TASK-ID>               # run spec phase
```

See `AGENTS.md` at the project root for the full workflow.
