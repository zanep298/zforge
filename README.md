# zForge

TDD-first AI development workflow CLI. Orchestrates a gated pipeline —
spec → testspec → plan → code → verify → review — where each phase
produces a markdown artifact and requires human approval before proceeding.

Works with **Claude Code**, **OpenCode**, and **Codex**.

## Install

### Homebrew (macOS & Linux) — recommended

```bash
brew install zanep298/tap/zforge
zforge --version
```

### Install script (macOS & Linux)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/zanep298/zforge/main/install.sh | sh
```

To install to a custom directory:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/zanep298/zforge/main/install.sh | ZFORGE_INSTALL=$HOME/bin sh
```

### Prebuilt binaries

Download the latest release for your platform from the [releases page](https://github.com/zanep298/zforge/releases), extract it, and move the `zforge` binary to a directory in your `PATH` (e.g. `~/.local/bin` or `/usr/local/bin`).

### From source (requires Rust)

```bash
cargo install --git https://github.com/zanep298/zforge
zforge --version
```

## Quick start

### With Claude Code

```bash
# 1. Scaffold + wire Claude Code
zforge init

# 2. Open project in Claude Code — zforge MCP server loads automatically

# 3. Run your first task
zforge task import TASK-001 --title "Your task title"
# edit tasks/TASK-001/task.md, then ask Claude Code:
# "run zforge spec TASK-001" → review → "zforge approve TASK-001 spec" → ...
zforge spec TASK-001
zforge approve TASK-001 spec
zforge testspec TASK-001
zforge approve TASK-001 testspec
zforge plan TASK-001
zforge code TASK-001
zforge verify TASK-001
zforge review TASK-001
```

**Session starter prompt for Claude Code:**

```
We're using zforge for this project. Run `zforge status` to see current task
progress, then help me work through the pipeline. For each phase, run the zforge
command, wait for my approval before proceeding, and follow the artifacts in
tasks/<ID>/. Use the zforge MCP tools when available.
```

### With OpenCode

```bash
# 1. Scaffold shared project files
zforge init

# 2. Open project in OpenCode and register local MCP command `zforge mcp`

# 3. Run your first task (same pipeline)
zforge task import TASK-001 --title "Your task title"
# edit tasks/TASK-001/task.md, then:
zforge spec TASK-001
zforge approve TASK-001 spec
zforge testspec TASK-001
zforge approve TASK-001 testspec
zforge plan TASK-001
zforge code TASK-001
zforge verify TASK-001
zforge review TASK-001
```

**Session starter prompt for OpenCode:**

```
We're using zforge for this project. Run `zforge status` to see current task
progress, then help me work through the pipeline. For each phase, run the zforge
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
| `zforge init` | `.zforge/`, `.mcp.json`, `CLAUDE.md`, `.claude/settings.json`, `.claude/agents/`, `.claude/rules/` |

Agents live in `.zforge/agents/` and are linked into `.claude/agents/`.
The project-local MCP entrypoints use `zforge mcp`.

## All commands

| Command | Description |
|---------|-------------|
| `zforge init` | Scaffold `.zforge/`, Claude Code files, and local MCP registration |
| `zforge task import <ID>` | Create a new task (supports `--jira`, `--figma`, `--figma-context`) |
| `zforge spec <ID>` | Generate spec prompt |
| `zforge approve <ID> spec` | Approve spec (human gate) |
| `zforge testspec <ID>` | Generate test spec prompt |
| `zforge approve <ID> testspec` | Approve testspec (human gate) |
| `zforge plan <ID>` | Generate implementation plan prompt |
| `zforge code <ID>` | Generate coding prompt (AI writes tests first) |
| `zforge verify <ID>` | Run tests, record results |
| `zforge review <ID>` | Generate review prompt, extract patterns |
| `zforge status` | Show all task progress |
| `zforge status <ID>` | Show single task status |
| `zforge retry <ID> --from <phase>` | Reset and retry from a phase |
| `zforge mcp` | Start MCP server (stdio) — called by AI tools automatically |

## Documentation

| Guide | Description |
|-------|-------------|
| [docs/getting-started.md](docs/getting-started.md) | Install, init, first task |
| [docs/workflow.md](docs/workflow.md) | Pipeline phases explained |
| [docs/commands.md](docs/commands.md) | Full command reference |
| [docs/opencode.md](docs/opencode.md) | Using zforge with OpenCode |
