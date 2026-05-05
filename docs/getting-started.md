# Getting Started

zforge (`zf`) is a TDD-first AI development workflow CLI. It orchestrates a gated
pipeline — spec → testspec → plan → code → verify → review — where each phase
produces a markdown artifact and requires explicit human approval before the next
phase begins.

## Install

```bash
git clone <repo>
cd zforge
cargo install --path .

# verify
zf --version
```

## Initialize a project

Run inside any Rust, Go, TypeScript, or Python project:

```bash
zf init
```

Creates the canonical project structure:

```
.zforge/
├── config.yaml          # project settings (language, test command, model)
├── agents/              # workflow agents — single source for all AI tools
│   ├── spec-agent.md
│   ├── testspec-agent.md
│   ├── plan-agent.md
│   ├── code-agent.md
│   └── review-agent.md
├── skills/              # coding patterns and testing conventions
└── memory/
    ├── patterns.md
    ├── domain-glossary.md
    └── anti-patterns.md
tasks/                   # task artifacts live here
```

Edit `.zforge/config.yaml` and set `project.name` before running any tasks.

## Wire up your AI tool

Run one (or both) after `zf init`:

### Claude Code

```bash
zf init claudecode
```

Creates:
- `.mcp.json` — registers `zf mcp` as a project-scoped MCP server (auto-discovered)
- `CLAUDE.md` — project instructions loaded automatically
- `.claude/settings.json` — pre-approves `zf` commands and MCP tools
- `.claude/agents/*.md` — symlinks to `.zforge/agents/` (no duplication)

Open the project in Claude Code. MCP tools (`task_import`, `get_prompt`, `approve`,
`verify`, `status`) are available immediately.

### OpenCode

```bash
zf init opencode
```

Creates `.opencode/` with `opencode.json` that references `.zforge/agents/` as
instructions and `.zforge/skills/` as a skills path. No files duplicated.
See [opencode.md](./opencode.md) for the full guide.

### Using both

```bash
zf init
zf init claudecode
zf init opencode
```

Both tools read agents from `.zforge/agents/`. Edit once, all tools see it.

## Your first task

```bash
# 1. Import — ID auto-generated (TASK-001, TASK-002, ...)
zf task import --title "Add health check endpoint"
# or supply an explicit ID: zf task import AUTH-42 --title "..."

# 2. Fill in the task description (use the printed path)
$EDITOR tasks/TASK-001/task.md

# 3. Run the pipeline
zf spec TASK-001        # generates prompt → paste into AI → AI writes spec.md
zf approve TASK-001 spec

zf testspec TASK-001    # AI writes testspec.md
zf approve TASK-001 testspec

zf plan TASK-001        # AI writes plan.md
zf code TASK-001        # AI implements, writes tests first

zf verify TASK-001      # runs your test command automatically
zf review TASK-001      # AI reviews, extracts patterns into memory
```

## Next steps

- [commands.md](./commands.md) — full command reference
- [workflow.md](./workflow.md) — pipeline phases explained
- [opencode.md](./opencode.md) — using zforge with OpenCode
