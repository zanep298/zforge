# zForge

TDD-first AI development workflow CLI. Orchestrates a gated pipeline —
spec → testspec → plan → code → verify → review — where each phase
produces a markdown artifact, with human gates before planning and coding.

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
# 1. Scaffold project files for Claude Code (default agent)
zforge init                    # same as: zforge init --agent claude

# 2. Register zforge MCP server with Claude Code (local scope)
zforge mcp register --agent claude

# 3. Open project in Claude Code — zforge MCP tools are available

# 4. Run your first task
zforge task import TASK-001 --title "Your task title"
# edit .zforge/tasks/TASK-001/task.md, then ask Claude Code:
# "run zforge spec TASK-001" → review → "zforge spec TASK-001 --done" → ...
zforge spec TASK-001
zforge spec TASK-001 --done
zforge testspec TASK-001
zforge testspec TASK-001 --done
zforge approve TASK-001 testspec
zforge plan TASK-001
zforge plan TASK-001 --done
zforge approve TASK-001 plan
zforge code TASK-001
zforge code TASK-001 --done
zforge verify TASK-001
zforge review TASK-001
zforge review TASK-001 --done
```

**Session starter prompt for Claude Code:**

```
We're using zforge for this project. Run `zforge status` to see current task
progress, then help me work through the pipeline. For each phase, run the zforge
command, wait for my approval before proceeding, and follow the artifacts in
.zforge/tasks/<ID>/. Use the zforge MCP tools when available.
```

### With Codex

```bash
# 1. Scaffold project files for Codex CLI
#    (writes AGENTS.md, .codex/agents/, and auto-registers MCP + profiles
#     in ~/.codex/config.toml)
zforge init --agent codex

# 2. Start a Codex session in the project — zforge MCP tools are available

# 3. Run your first task (same pipeline)
zforge task import TASK-001 --title "Your task title"
# edit .zforge/tasks/TASK-001/task.md, then:
zforge spec TASK-001
zforge spec TASK-001 --done
zforge testspec TASK-001
zforge testspec TASK-001 --done
zforge approve TASK-001 testspec
zforge plan TASK-001
zforge plan TASK-001 --done
zforge approve TASK-001 plan
zforge code TASK-001
zforge ship TASK-001
zforge review TASK-001
zforge review TASK-001 --done
```

Codex picks the right model per phase via profiles in `~/.codex/config.toml`:

```bash
codex --profile zforge_spec     "run zforge spec TASK-001"
codex --profile zforge_testspec "run zforge testspec TASK-001"
codex --profile zforge_plan     "run zforge plan TASK-001"
codex --profile zforge_code     "run zforge code TASK-001"
codex --profile zforge_review   "run zforge review TASK-001"
```

| Profile | Phase | Default model |
|---------|-------|---------------|
| `zforge_spec` | spec | `gpt-5.4-mini` |
| `zforge_testspec` | testspec | `gpt-5.4-mini` |
| `zforge_plan` | plan | `gpt-5.4` |
| `zforge_code` | code | `gpt-5.3-codex` |
| `zforge_review` | review | `gpt-5.4` |

**Session starter prompt for Codex:**

```
We're using zforge for this project. Run `zforge status` to see current task
progress, then help me work through the pipeline. For each phase, run the zforge
command, wait for my approval before proceeding, and follow the artifacts in
.zforge/tasks/<ID>/. Use the zforge MCP tools (task_import, get_prompt, approve, verify,
ship, status) when available.
```

### With OpenCode

```bash
# 1. Scaffold project files for OpenCode
#    (writes AGENTS.md, .opencode/agents/, and auto-registers MCP
#     in ~/.config/opencode/opencode.json)
zforge init --agent opencode

# 2. Open project in OpenCode — zforge MCP tools are available

# 3. Run your first task (same pipeline)
zforge task import TASK-001 --title "Your task title"
# edit .zforge/tasks/TASK-001/task.md, then:
zforge spec TASK-001
zforge spec TASK-001 --done
zforge testspec TASK-001
zforge testspec TASK-001 --done
zforge approve TASK-001 testspec
zforge plan TASK-001
zforge plan TASK-001 --done
zforge approve TASK-001 plan
zforge code TASK-001
zforge code TASK-001 --done
zforge verify TASK-001
zforge review TASK-001
zforge review TASK-001 --done
```

**Session starter prompt for OpenCode:**

```
We're using zforge for this project. Run `zforge status` to see current task
progress, then help me work through the pipeline. For each phase, run the zforge
command, wait for my approval before proceeding, and follow the artifacts in
.zforge/tasks/<ID>/. Use the zforge agents in .opencode/ for phase-specific guidance.
```

## Pipeline

```
task import → spec → testspec → [approve testspec] → plan → [approve plan] → code → verify → review
```

`zforge ship <ID>` is a shortcut for `code` + `verify` back-to-back — saves a tool round trip when chained via MCP, and is idempotent (skips the code phase if state is already `Coded`).

## Flows (pipeline presets)

The full nine-phase pipeline above is overkill for bug fixes, spikes, or doc edits.
Pick a shorter preset at import time with `--flow`:

| Flow | Phases | Use when |
|------|--------|----------|
| `full` (default) | spec → testspec → approve → plan → approve → code → verify → review | Cross-cutting feature work, anything risky |
| `fixbug` | spec → testspec → code → verify | Bug fix with a clear reproducer (<50 LOC) |
| `spike` | spec → code | Research, prototypes, throwaway exploration |
| `docs` | code | Docs / README / comments only |

```bash
zforge task import BUG-42 --flow fixbug --title "Login crashes on empty email"
zforge task import SPIKE-1 --flow spike --title "Try GraphQL adapter"
zforge task import DOC-9   --flow docs  --title "Document MCP setup"
```

The flow is persisted in `.zforge/tasks/<ID>/.state.yaml`. Phases not in the chosen
flow are rejected — e.g. `zforge plan TASK-1` fails on a `fixbug` task. `zforge status`
displays the active flow and the correct next command.

For UI tasks, attach Figma design context at import time — it flows through spec and code automatically:

```
Figma MCP → figma_context → task import → figma.md → spec prompt
                                                    → code prompt
```

## Init commands

`zforge init` scaffolds per agent. Pick exactly one (default: `claude`).

| Command | What it creates | MCP auto-register? |
|---------|----------------|--------------------|
| `zforge init` (default) | `.zforge/`, `CLAUDE.md`, `.claude/settings.json`, `.claude/agents/`, `.claude/rules/` | No — run `zforge mcp register --agent claude` |
| `zforge init --agent codex` | `.zforge/`, `AGENTS.md`, `.codex/agents/`, `.codex/README.md` | Yes — `~/.codex/config.toml` + per-phase profiles |
| `zforge init --agent opencode` | `.zforge/`, `AGENTS.md`, `.opencode/agents/` | Yes — `~/.config/opencode/opencode.json` |
| `zforge init --agent all` | All of the above | Yes — codex + opencode |

`.zforge/agents/` is the single source of truth — `.claude/agents/`,
`.codex/agents/`, and `.opencode/agents/` are symlinks into it.

## Register the MCP server

```bash
zforge mcp register                    # register with all detected agents (default)
zforge mcp register --agent claude     # Claude Code only (uses `claude mcp add`)
zforge mcp register --agent codex      # Codex (writes ~/.codex/config.toml)
zforge mcp register --agent opencode   # OpenCode (writes ~/.config/opencode/opencode.json)
zforge mcp register --force            # re-register, overwriting any existing entry
```

Each agent writes to its own user-scoped config:

| Agent | Config location | Method |
|-------|----------------|--------|
| Claude Code | local MCP registry | `claude mcp add zforge -- zforge mcp` |
| Codex | `~/.codex/config.toml` | `[mcp_servers.zforge]` block |
| OpenCode | `~/.config/opencode/opencode.json` | `mcp.zforge` entry |

Agents that are not installed are skipped, not failed.

## All commands

| Command | Description |
|---------|-------------|
| `zforge init` | Scaffold `.zforge/`, `CLAUDE.md`, `.claude/` files |
| `zforge mcp register` | Register zforge MCP server with Claude Code, Codex, and/or OpenCode |
| `zforge task import <ID>` | Create a new task (supports `--jira`, `--figma`, `--figma-context`, `--flow`) |
| `zforge spec <ID>` | Generate spec prompt |
| `zforge testspec <ID>` | Generate test spec prompt |
| `zforge approve <ID> testspec` | Approve testspec (human gate) |
| `zforge plan <ID>` | Generate implementation plan prompt |
| `zforge approve <ID> plan` | Approve implementation plan (human gate) |
| `zforge code <ID>` | Generate coding prompt (AI writes tests first) |
| `zforge verify <ID>` | Run tests, record results |
| `zforge ship <ID>` | Run `code` + `verify` in one step (idempotent — skips code if already Coded) |
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
| [docs/flows.md](docs/flows.md) | Flow presets: full, fixbug, spike, docs |
| [docs/retry.md](docs/retry.md) | Rewinding and redoing a phase |
| [docs/memory.md](docs/memory.md) | How pattern memory accumulates across tasks |
| [docs/mcp.md](docs/mcp.md) | MCP server tools and orchestrator integration |
| [docs/install-global.md](docs/install-global.md) | Global store, shared vs local mode |
| [docs/opencode.md](docs/opencode.md) | Using zforge with OpenCode |
