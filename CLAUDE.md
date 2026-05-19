# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                          # build
cargo test                           # all tests
cargo test <name>                    # single test (substring match)
cargo test -- --nocapture            # tests with println output
cargo clippy -- -D warnings          # lint (CI-enforced, warnings are errors)
cargo fmt                            # format
cargo install --path . --force       # install binary locally as `zforge`
```

## Architecture

zforge is a single Rust binary (`src/main.rs`) with these modules:

| Module | Purpose |
|--------|---------|
| `cli/` | One file per subcommand. Each `run()` function loads config, checks state, builds context, dispatches to engine |
| `prompt/` | `context.rs` assembles `PromptContext` per phase; `engine.rs` renders `.tmpl` files against context |
| `state/` | `TaskState` + `State` enum — ordered FSM. `Flow` (in `state/flow.rs`) picks which subset of states a task walks. `advance()` consults `flow.next_after(state)`; `require()` guards phase entry |
| `fs/` | `reader.rs` parses markdown frontmatter; `writer.rs` writes/appends files; `tokens.rs` estimates token counts; `scaffold.rs` handles directory scaffolding |
| `runner/` | `run_with_language()` shells out the test command and parses output into `TestResult`. Used by `verify` |
| `mcp/` | stdio JSON-RPC 2.0 server. Exposes `task_import`, `get_prompt`, `approve`, `verify`, `ship`, `status` as MCP tools |
| `config/` | Loads `.zforge/config.yaml`; walks up from cwd to find it |
| `jira/` | Fetches Jira tickets and extracts task key from URLs |
| `embedded.rs` | All prompt templates, agent definitions, and skill bundles baked in via `include_str!()`. Fallback when no disk file found; consumed by `init` and `install` |

### Pipeline state machine

The full pipeline:

```
Imported → SpecDone → TestspecDone → TestspecReviewed → Planned → PlanReviewed → Coded → Verified → Reviewed
```

Human approval gates: `testspec` (TestspecDone → TestspecReviewed) and `plan` (Planned → PlanReviewed). State is persisted per-task in `.zforge/tasks/<ID>/.state.yaml` together with the task's `Flow`.

### Flows (pipeline presets)

`Flow` (in `state/flow.rs`) selects an ordered subset of `State`. Picked at import time with `--flow`; recorded in `.state.yaml` as `flow: Full|Fixbug|Spike|Docs` (`#[serde(default)]` = `Full`, so pre-flow state files keep working).

| Flow | States |
|------|--------|
| `Full` (default) | Imported → SpecDone → TestspecDone → TestspecReviewed → Planned → PlanReviewed → Coded → Verified → Reviewed |
| `Fixbug` | Imported → SpecDone → TestspecDone → Coded → Verified |
| `Spike` | Imported → SpecDone → Coded |
| `Docs` | Imported → Coded |

`advance()` only accepts the state immediately after the current one *in this task's flow*. CLI phase commands gate entry through `cli/flow_guard.rs` (`ensure_phase_in_flow` + `ensure_predecessor_complete`). All "Next:" output goes through `TaskState::next_hint()` so the suggested command always matches the active flow. MCP routing uses `mcp::next_cmd_with_flow` for the same reason.

`zforge ship <ID>` (CLI) wraps `code` + `verify`: dispatches the code sub-agent (if state `< Coded`), advances to `Coded`, then runs the test suite. The MCP `ship` tool is the orchestrator-side counterpart — assumes the LLM has already written code and combines the `Coded` advance + `verify` into a single tool call (saves a round trip vs. invoking them separately). Verify itself never calls an LLM — it shells out via `runner::run_with_language()`.

### Retry

`zforge retry <ID> --from <phase>` rewinds a task to before a phase. It:
1. Prompts for confirmation (bypass with `--yes`)
2. Backs up affected artifacts to `.zforge/tasks/<ID>/.history/<timestamp>/`
3. Deletes those artifacts and resets state

Valid phases: `spec`, `testspec`, `plan`, `code`, `verify`, `review`. Each phase clears all artifacts from that phase onward.

### Global store and `install`

`zforge install` populates `~/.zforge/` with embedded templates, agent definitions, and skills. Run once per machine; rerun after upgrading to refresh. `zforge init` defaults to symlinking/referencing `~/.zforge/` (shared mode). Pass `--local` to copy everything into the project instead — for forks needing per-project customization.

### Multi-agent support

`zforge init --agent <target>` and `zforge mcp register --agent <target>` support `claude` (default), `codex`, `opencode`, or `all`. Agent detection in `cli/init/detect.rs` also auto-detects project language (Rust, Go, Flutter, etc.) to select appropriate skill bundles.

### Prompt / template system

Each pipeline phase has a `.tmpl` file in `.zforge/agents/` (generated at `zforge init` from `templates/`). `build_context_for_phase()` in `context.rs` decides which files to inline vs. emit as `/file` refs — different phases get different context. `Engine::render()` handles `{{variable}}` substitution and `{{if var}}...{{end}}` conditionals. `Engine::dispatch()` auto-detects Claude Code or OpenCode binary and launches the appropriate agent.

Memory files (`.zforge/memory/patterns.md`, `anti-patterns.md`, `domain-glossary.md`) are injected as `/file` refs only when they contain at least one non-heading, non-empty line — empty scaffolds with just `#` headers are skipped. `has_content_ref()` in `context.rs` enforces this. Pattern extraction from `review-summary.md` runs automatically at `zforge review <ID> --done` via `append_unique_lines()`, which deduplicates by the key portion (text before `:`) of each `- key: description` line, both against existing file content and within the same batch.

### `zforge init` scaffolding

Templates are embedded via `embedded.rs`. Init writes `.zforge/`, `CLAUDE.md`, `.mcp.json`, `.claude/settings.json`, `.claude/agents/` (symlinks → `.zforge/agents/`), and `.claude/rules/` (copies from `~/.claude/rules/` or bundled fallbacks).

### MCP server

`zforge mcp` runs as a stdio JSON-RPC server. It delegates directly to the same `cli::*` functions the CLI uses — no separate code paths. Registered via project-local `.mcp.json` so Claude Code picks it up automatically. `zforge mcp register` writes the registration into the agent's config (supports Claude Code, Codex, OpenCode).

## Key invariants

- `State` derives `PartialOrd`/`Ord` — comparisons (`>=`, `<`) express "at least this far in the pipeline". Use `require(State::X)` for absolute gates; use `flow_guard::ensure_predecessor_complete` when the gate depends on the active flow.
- `Flow` is immutable once a task is imported. There is no "change flow" command — import a new task if you picked wrong.
- All paths go through `Config::resolve_path()` — always absolute, always relative to the project root (parent of `.zforge/`), never to cwd.
- Memory is injected only via `/file` refs in `context_files` — there are no inline `{{patterns}}` / `{{anti_patterns}}` template variables. Variable names listed in `VAR_NAMES` (`engine.rs`) must match arms in `get_var()`.
- `approve.rs` handles testspec and plan approvals. Review approval (`zforge review <ID> --done`) is handled in `review.rs` and triggers memory extraction.
- `embedded.rs` is the single source of truth for all bundled assets. `cli::install` and `cli::init` both consume it; `prompt::engine` falls back to it when no disk `.tmpl` is found.
