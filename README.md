# zForge

Agree on the work, then let an agent build it. zForge keeps the agreement in
an **intake** — outcome, behavior, solution, breakdown and one contract per
task, each reviewed and accepted by you — and builds it in **runs**: an agent
in its own git worktree, tests first, verified against your test command,
never touching your checkout.

Runs execute with **Claude Code**; **Codex** and **OpenCode** can prepare
intakes and follow runs through the MCP server.

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

```bash
# 1. Scaffold the project for Claude Code and register zforge's MCP server
zforge init
zforge mcp register --agent claude

# 2. Set the test command and a budget in .zforge/config.yaml
#    project.test_command, execution.budget_usd

# 3. Agree on the work — Claude can draft the files (skill zforge-intake)
zforge intake new FEAT-1
zforge intake task FEAT-1 TASK-001
zforge intake review FEAT-1 01-outcome.md      # … every file
zforge intake accept FEAT-1 01-outcome.md      # you, in a terminal

# 4. Hand over and build
zforge readiness FEAT-1
zforge handover FEAT-1                         # you, in a terminal
zforge run HANDOVER-001                        # every task, then the integration check

# Where things stand, and what to run next
zforge status
```

Accepting, asking for changes and handing over are yours: they need an
interactive terminal and a typed confirmation, and no MCP tool offers them.

## How a run works

```
intake (accepted) → handover → per task, in dependency order:
    worktree from the outputs it depends on → agent writes tests + code
    → test command → protected tests unchanged? → [optional review agent]
    → output sealed as a commit
  → integration check on the merged outputs → merge → zforge knowledge index
```

Each run records everything in `.zforge/runs/RUN-nnn/events.jsonl` — the only
source of truth; `zforge run status` and `zforge status` derive from it. A run
stops `blocked` when the budget runs out or the agent asks to amend the
contract; you amend the intake and hand over again.

## Init

`zforge init` scaffolds per client (default: `claude`).

| Command | What it creates | MCP auto-register? |
|---------|----------------|--------------------|
| `zforge init` (default) | `.zforge/`, `CLAUDE.md`, `.claude/settings.json`, `.claude/agents/`, `.claude/skills/`, `.claude/rules/` | No — run `zforge mcp register --agent claude` |
| `zforge init --agent codex` | `.zforge/`, `AGENTS.md`, `.codex/agents/`, `.codex/README.md` | Yes — `~/.codex/config.toml` |
| `zforge init --agent opencode` | `.zforge/`, `AGENTS.md`, `.opencode/agents/` | Yes — `~/.config/opencode/opencode.json` |
| `zforge init --agent all` | All of the above | Yes — codex + opencode |

The agents are `code-agent` (every attempt of a run) and `review-agent`
(optional review of passing work). Their models default to Claude tiers —
`sonnet` for code, `opus` for review — and `zforge models set` changes them.

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
| `zforge init` / `zforge install` | Scaffold a project; populate the global store `~/.zforge/` |
| `zforge status [--global]` | Every intake, its handovers and runs, and the next step |
| `zforge intake new\|task\|status\|review <ID> …` | Prepare an intake and send files for review |
| `zforge intake accept\|revise <ID> <file>` | Record your decision (interactive terminal only) |
| `zforge readiness <ID>` | Can the accepted files be handed over, and why not |
| `zforge handover <ID>` | Hand the accepted contract over (interactive terminal only) |
| `zforge run <HANDOVER> [--task T] [--async]` | Build a handover, or one task of it |
| `zforge run status\|list\|log\|wait\|cancel\|retry\|clean` | Follow and manage runs |
| `zforge knowledge index` | Rebuild `.zforge/knowledge/` — each requirement, decided and built |
| `zforge models [set\|unset]` | Which model runs each phase |
| `zforge doctor` | Check the Claude Code setup actually works |
| `zforge project …` | The registry of zforge projects on this machine |
| `zforge mcp` / `zforge mcp register` | MCP server (stdio); register it with a client |
| `zforge update` | Install the latest release |

## Documentation

| Guide | Description |
|-------|-------------|
| [docs/v1.5/README.md](docs/v1.5/README.md) | v1.5: intake, handover, runs — the current workflow |
| [docs/v1.5/usage.md](docs/v1.5/usage.md) | v1.5 step by step, from a request to verified code |
| [docs/v1.5/workflow.md](docs/v1.5/workflow.md) | v1.5 design: states, records, runs |
| [docs/v1.5/decisions.md](docs/v1.5/decisions.md) | v1.5 decisions |
| [docs/v1/README.md](docs/v1/README.md) | v1 task pipeline — removed; kept for history |
| [docs/v2/README.md](docs/v2/README.md) | v2 documentation index and recommended reading order |
| [docs/v2/product-direction.md](docs/v2/product-direction.md) | Quality-first product objective, target users, autonomy boundary, and accepted outcomes |
| [docs/v2/fleet-and-human-attention.md](docs/v2/fleet-and-human-attention.md) | Work batches, preflight, Decision Inbox, quality-first scheduling, and consolidated decisions |
| [docs/v2/flow.md](docs/v2/flow.md) | Proposed autonomous flow with decomposition and risk-based supervision |
| [docs/v2/cost.md](docs/v2/cost.md) | Proposed v2 agent token accounting, forecasting, and budget enforcement |
| [docs/v2/agents.md](docs/v2/agents.md) | Proposed v2 agent roles, independence, assignment, and composition |
| [docs/v2/model-routing.md](docs/v2/model-routing.md) | Automatic quality-first runtime-agent/model selection, catalog, fallback, and evaluation |
| [docs/v2/deterministic-runtime.md](docs/v2/deterministic-runtime.md) | Proposed v2 orchestrator, policy, workspace, gates, state, Git, and delivery runtime |
| [docs/v2/skills.md](docs/v2/skills.md) | Proposed v2 skill packages, resolution, policy boundaries, versioning, and evaluation |
| [docs/v2/data-model.md](docs/v2/data-model.md) | Normative draft for v2 records, IDs, relationships, persistence, and invariants |
| [docs/v2/state-and-events.md](docs/v2/state-and-events.md) | Normative draft for lifecycle states, event streams, transition guards, idempotency, and recovery |
| [docs/v2/artifacts-and-traceability.md](docs/v2/artifacts-and-traceability.md) | Normative draft for artifacts, evidence validity, trace graphs, coverage, invalidation, and evidence bundles |
| [docs/v2/execution-dag.md](docs/v2/execution-dag.md) | Normative draft for graph compilation, validation, scheduling, concurrency, correction, and completion |
| [docs/v2/policy-and-risk.md](docs/v2/policy-and-risk.md) | Normative draft for risk classification, protected actions, scoped grants, approval, and exceptions |
| [docs/v2/configuration.md](docs/v2/configuration.md) | Normative draft for configuration layers, typed merge, references, secrets, and immutable run snapshots |
| [docs/v2/workspace-and-git.md](docs/v2/workspace-and-git.md) | Normative draft for isolated workspaces, scoped changes, Git reconciliation, commits, and cleanup |
| [docs/v2/quality-gates.md](docs/v2/quality-gates.md) | Normative draft for gate selection, execution, parsing, evidence, baselines, flakiness, and reuse |
| [docs/v2/errors-and-recovery.md](docs/v2/errors-and-recovery.md) | Normative draft for error taxonomy, recovery routing, retries, reconciliation, and terminal failure |
| [docs/v2/cli-and-mcp.md](docs/v2/cli-and-mcp.md) | Normative draft for v2 CLI/MCP parity, commands, queries, jobs, identity, and compatibility |
| [docs/v2/security-threat-model.md](docs/v2/security-threat-model.md) | Threat model for assets, trust boundaries, attack paths, controls, residual risk, and incident response |
| [docs/v2/evaluation.md](docs/v2/evaluation.md) | Normative draft for benchmarks, metrics, evaluator integrity, release gates, and continuous evaluation |

V2 is a design target, separate from the current v1.5 workflow above. It stops at
reviewable development results and excludes application deployment and production
access/debugging. See [Development Handoff and Review](docs/v2/delivery-and-review.md)
and [Project Onboarding and Local Testing](docs/v2/project-onboarding-and-local-testing.md).
