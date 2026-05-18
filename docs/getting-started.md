# Getting Started

zforge is a TDD-first AI development workflow CLI. It orchestrates a gated
pipeline — spec → testspec → plan → code → verify → review — where each phase
produces a markdown artifact, with explicit human gates before planning and coding.

## Install

```bash
git clone <repo>
cd zforge
cargo install --path .

# verify
zforge --version
```

## Initialize a project

Run inside any Rust, Go, TypeScript, Python, Android, iOS, or Flutter project:

```bash
zforge init
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
└── tasks/               # task artifacts live here
```

Edit `.zforge/config.yaml` and set `project.name` before running any tasks.

## Wire up your AI tool

Run `zforge init` after install:

### Claude Code

```bash
zforge init
```

Creates:
- `.mcp.json` — registers `zforge mcp` as a project-scoped MCP server (auto-discovered)
- `CLAUDE.md` — project instructions loaded automatically
- `.claude/settings.json` — pre-approves `zforge` commands and MCP tools
- `.claude/agents/*.md` — symlinks to `.zforge/agents/` (no duplication)

Open the project in Claude Code. MCP tools (`task_import`, `get_prompt`, `approve`,
`verify`, `status`) are available immediately.

### OpenCode

Register the local MCP command as `zforge mcp` in your OpenCode project config.
See [opencode.md](./opencode.md) for the full guide.

### Using both

Both tools can call the same local MCP server command: `zforge mcp`.

## Your first task

```bash
# 1. Import — ID auto-generated (TASK-001, TASK-002, ...)
zforge task import --title "Add health check endpoint"
# or supply an explicit ID: zforge task import AUTH-42 --title "..."

# 2. Fill in the task description (use the printed path)
$EDITOR .zforge/tasks/TASK-001/task.md

# 3. Run the pipeline
zforge spec TASK-001        # generates prompt → paste into AI → AI writes spec.md
zforge spec TASK-001 --done

zforge testspec TASK-001    # AI writes testspec.md
zforge testspec TASK-001 --done
zforge approve TASK-001 testspec

zforge plan TASK-001        # AI writes plan.md
zforge plan TASK-001 --done
zforge approve TASK-001 plan
zforge code TASK-001        # AI implements, writes tests first
zforge code TASK-001 --done

zforge verify TASK-001      # runs your test command automatically
zforge review TASK-001      # AI reviews, extracts patterns into memory
zforge review TASK-001 --done
```

## Next steps

- [commands.md](./commands.md) — full command reference
- [workflow.md](./workflow.md) — pipeline phases explained
- [opencode.md](./opencode.md) — using zforge with OpenCode
