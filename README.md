# zForge

TDD-first AI development workflow CLI. Orchestrates a gated pipeline —
spec → testspec → plan → code → verify → review — where each phase
produces a markdown artifact and requires human approval before proceeding.

Works with **Claude Code**, **OpenCode**, and **Codex**.

## Install

### From source (requires Rust)

```bash
cargo install zf
zf --version
```

Or install the latest from git:

```bash
cargo install --git https://github.com/zanep298/zforge
```

### Prebuilt binaries

Download the latest release for your platform from the [releases page](https://github.com/zanep298/zforge/releases), extract it, and move the `zf` binary to a directory in your `PATH` (e.g. `~/.local/bin` or `/usr/local/bin`).

### Install script (macOS & Linux)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/zanep298/zforge/main/install.sh | sh
```

To install to a custom directory:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/zanep298/zforge/main/install.sh | ZF_INSTALL=$HOME/bin sh
```

## Quick start

### With Claude Code

```bash
# 1. Scaffold + wire Claude Code
zf init
zf init claudecode    # creates .mcp.json, CLAUDE.md, .claude/ agent symlinks

# 2. Open project in Claude Code — zf MCP server loads automatically

# 3. Run your first task
zf task import TASK-001 --title "Your task title"
# edit tasks/TASK-001/task.md, then ask Claude Code:
# "run zf spec TASK-001" → review → "zf approve TASK-001 spec" → ...
zf spec TASK-001
zf approve TASK-001 spec
zf testspec TASK-001
zf approve TASK-001 testspec
zf plan TASK-001
zf code TASK-001
zf verify TASK-001
zf review TASK-001
```

**Session starter prompt for Claude Code:**

```
We're using zforge (zf) for this project. Run `zf status` to see current task
progress, then help me work through the pipeline. For each phase, run the zf
command, wait for my approval before proceeding, and follow the artifacts in
tasks/<ID>/. Use the zf MCP tools when available.
```

### With OpenCode

```bash
# 1. Scaffold + wire OpenCode
zf init
zf init opencode      # creates .opencode/ referencing .zforge/agents/

# 2. Open project in OpenCode — agents and skills load automatically

# 3. Run your first task (same pipeline)
zf task import TASK-001 --title "Your task title"
# edit tasks/TASK-001/task.md, then:
zf spec TASK-001
zf approve TASK-001 spec
zf testspec TASK-001
zf approve TASK-001 testspec
zf plan TASK-001
zf code TASK-001
zf verify TASK-001
zf review TASK-001
```

**Session starter prompt for OpenCode:**

```
We're using zforge (zf) for this project. Run `zf status` to see current task
progress, then help me work through the pipeline. For each phase, run the zf
command, wait for my approval before proceeding, and follow the artifacts in
tasks/<ID>/. Use the zforge agents in .opencode/ for phase-specific guidance.
```

## Pipeline

```
task import → spec → [approve] → testspec → [approve] → plan → code → verify → review
```

For UI tasks, attach Figma design context at import time — it flows through spec and code automatically:

```
Figma MCP → figma_context → task import → figma.md → spec prompt
                                                    → code prompt
```

## Init commands

| Command | What it creates |
|---------|----------------|
| `zf init` | `.zforge/` — canonical agents, skills, memory, config |
| `zf init claudecode` | `.mcp.json`, `CLAUDE.md`, `.claude/settings.json`, `.claude/agents/` (symlinks → `.zforge/agents/`) |
| `zf init opencode` | `.opencode/opencode.json`, `.opencode/instructions.md`, `.opencode/skills/`, `.opencode/rules/` |

Agents live in `.zforge/agents/` — single source of truth for all tools.
`init claudecode` and `init opencode` reference them without copying.

## All commands

| Command | Description |
|---------|-------------|
| `zf init` | Scaffold `.zforge/` (canonical agents, skills, memory) |
| `zf init claudecode` | Wire up Claude Code (MCP, CLAUDE.md, agent symlinks) |
| `zf init opencode` | Wire up OpenCode (.opencode/ referencing .zforge/) |
| `zf task import <ID>` | Create a new task (supports `--jira`, `--figma`, `--figma-context`) |
| `zf spec <ID>` | Generate spec prompt |
| `zf approve <ID> spec` | Approve spec (human gate) |
| `zf testspec <ID>` | Generate test spec prompt |
| `zf approve <ID> testspec` | Approve testspec (human gate) |
| `zf plan <ID>` | Generate implementation plan prompt |
| `zf code <ID>` | Generate coding prompt (AI writes tests first) |
| `zf verify <ID>` | Run tests, record results |
| `zf review <ID>` | Generate review prompt, extract patterns |
| `zf status` | Show all task progress |
| `zf status <ID>` | Show single task status |
| `zf retry <ID> --from <phase>` | Reset and retry from a phase |
| `zf mcp` | Start MCP server (stdio) — called by AI tools automatically |

## Documentation

| Guide | Description |
|-------|-------------|
| [docs/getting-started.md](docs/getting-started.md) | Install, init, first task |
| [docs/workflow.md](docs/workflow.md) | Pipeline phases explained |
| [docs/commands.md](docs/commands.md) | Full command reference |
| [docs/opencode.md](docs/opencode.md) | Using zforge with OpenCode |
