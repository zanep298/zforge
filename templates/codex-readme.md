# .codex/

Project-scoped artifacts for the OpenAI Codex CLI.

## Files

| File | Purpose |
|------|---------|
| `agents/*.md` | Agent prompts materialized from zforge templates (`code-agent`, `review-agent`) |

## Configuration

Codex CLI is configured globally at `~/.codex/config.toml`. zforge registers its
MCP server there with `zforge mcp register --agent codex`, which adds:

```toml
[mcp_servers.zforge]
command = "zforge"
args = ["mcp"]
```

Once registered, Codex sessions can prepare intakes and follow runs through
zforge's tools (`status`, `intake_*`, `readiness`, `run_*`, …). Runs
themselves execute with Claude Code.

## Permissions / Allowlists

Codex CLI controls command execution through its sandbox and approval policy in
`~/.codex/config.toml`. There is no project-scoped permissions file equivalent
to `.claude/settings.json`. Check the current Codex CLI docs for the supported
keys before relying on any specific approval flow.

## Quick Reference

```bash
zforge mcp register --agent codex   # add/refresh MCP entry
zforge status                       # where each intake stands, and what to do next
zforge intake new <ID>              # start an intake
```

See `AGENTS.md` at the project root for the full workflow.
