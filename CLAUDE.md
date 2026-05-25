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
| `mcp/` | stdio JSON-RPC 2.0 server. Exposes per-task tools (`task_import`, `get_prompt`, `approve`, `verify`, `ship`, `status`) plus global registry tools (`project_list`, `project_add`, `project_remove`, `switch_project`, `global_status`) |
| `registry/` | Global project registry at `~/.zforge/registry.yaml`. `schema.rs` types + `Registry::resolved_agent`, `io.rs` atomic load/save, `lock.rs` advisory `flock`, `validate.rs` name/path checks, `auto.rs` insert-or-update used by `init`. `paths.rs` honors `ZFORGE_HOME` for tests |
| `orchestrator/` | PR3 fallback loop + PR4 verifier loop + PR6 model routing + PR10 spawn timeout + PR11 headless bypass. `spawn.rs` pipes prompt to agent stdin in a writer thread, drains stdout/stderr in reader threads, enforces `spawn_timeout_secs` via `wait_timeout` (kill → synthesize exit 124); `fallback.rs` compiles the policy regex set once, scans combined stdout+stderr; `history.rs` records swaps + flips `active_agent`; `run.rs` (`run_phase`) is the entry point CLI phase commands call when a task has an `assigned_agent`; `model_args.rs` maps `(agent_name, model)` → CLI flags (`--model X` for claude/opencode/agy; empty for codex which uses profiles); `headless_args.rs` returns per-agent permission-bypass flags prepended when `ZFORGE_HEADLESS=1` is set (claude `--dangerously-skip-permissions`, codex `-a never -s workspace-write` top-level before `exec`). `verifier_loop.rs::iterate` is the pure SWE-bench-style code → verify → retry-with-feedback loop, driven by closures so callers (`cli::ship`) provide real agent dispatch + test runner |
| `job/` | PR5 background mode. `schema.rs` `Job` + `JobStatus`; `store.rs` per-project `.zforge/jobs/<JOB-ID>/{job.yaml,log}` with atomic tmp+rename writes; `lifecycle.rs` state transitions + `pid_alive`/`reconcile_dead_worker` for crash detection; `spawn.rs` detaches a `zforge worker --job-id <ID>` subprocess with `process_group(0)`; `worker.rs` is the re-entry point that runs ship and marks success/failed |
| `cost/` | PR13 spawn-level telemetry. `schema.rs` `CostEntry` (timestamp + task + phase + agent + tokens + USD); `prices.rs` static per-(agent,model) USD/1M tokens table; `log.rs` JSONL append at `<project>/.zforge/cost-log.jsonl`; `report.rs` aggregator + table formatter for `zforge cost report --by task\|phase\|agent\|model [--since TS] [--task T1] [--json]` |
| `state/task_lock.rs` | PR12 per-task `flock` mutex at `<tasks_dir>/<TASK_ID>/.task.lock`. `try_acquire` non-blocking, returns `Busy{owner_pid}` when held elsewhere. Worker holds for its lifetime; foreground `ship::run` also acquires (skipped when `ZFORGE_HEADLESS=1` is set — worker already owns outer lock) |
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

### Global project registry

`~/.zforge/registry.yaml` lists every zforge project on the machine. Schema: `current_project`, `projects[]` (`name`, `path`, `registered_at`, `registered_by: init|manual`, `agent_overrides`), `agents{}` (validated by PR2 task imports), `fallback_policy` (consumed by PR3 orchestrator). Mutations go through `registry::lock::with_lock` (advisory `flock` on `~/.zforge/registry.lock`) + `registry::io::save_atomic` (tmp + fsync + rename). `paths.rs` honors `ZFORGE_HOME` so integration tests can redirect to a tempdir — combine with `#[serial_test::serial]` because the env var is process-global.

`zforge init` calls `registry::auto::auto_register` at the tail of `run()` after local scaffold succeeds. Failure is non-fatal: scaffold is the contract, registry is best-effort. Flags: `--no-register`, `--name <NAME>`, `--switch`. Same-path/same-name = no-op; same-path/different-name = rename (preserves `registered_at`, `registered_by`, `agent_overrides`); different-path/same-name = auto-suffix `-2`, `-3`, ….

`zforge project` (CLI) and the matching MCP tools share `cli::project::{add_entry, remove_entry, purge_zforge_dir, switch_to, list_data}` — no parallel code paths. `zforge status --global [--timeout-ms N] [--json]` spawns one thread per project, applies a per-project `recv_timeout`, and skips slow / corrupt / missing-path projects with stderr warnings instead of crashing.

### Task-level agent assignment (PR2)

`TaskState` carries four agent fields: `assigned_agent` (immutable audit trail), `fallback_agent`, `active_agent` (mutated by PR3 orchestrator on retryable failures), and `fallback_history: Vec<FallbackEntry>` (append-only swap log). All four are `#[serde(default, skip_serializing_if = ...)]` so legacy `.state.yaml` files load with `None`/empty values and freshly-created states without agent flags serialize without noise.

`zforge task import --agent NAME --fallback NAME` and the MCP `task_import` tool both delegate to `cli::task::run_import`, which calls `cli::task::validate_agent_args` before any scaffold write. Validation rejects unknown agents (not in `registry.agents{}`) and `--agent == --fallback`. `--fallback` alone is allowed — primary stays `None`, leaving the PR3 orchestrator to fall back to the init-detected default. `TaskState::effective_agent()` returns `active_agent`, falling back to `assigned_agent`, falling back to `None`.

### Fallback orchestration (PR3 + PR9)

`orchestrator::run_phase(task_id, phase, project_root, prompt)` is the loop: resolve `effective_agent` → `Registry::resolved_agent` (project `agent_overrides` win over global `agents{}`) → `spawn::spawn_agent` (stdin-piped prompt, captured stderr+stdout+exit) → concat stderr + stdout into `combined_output` → `CompiledPolicy::should_fallback(exit, combined_output)` (retryable exit code OR pattern regex match — exit code wins ties) → on hit, `history::record_fallback` swaps `active_agent` (NEVER `assigned_agent`), persists via tmp+rename, sleeps `cooldown_ms`, retries. Budget is `max_retries` swaps total; exhaustion is a terminal error. Non-retryable failures (e.g. exit 1 with no matching pattern) propagate immediately without recording a swap.

**Critical PR9 invariant** (verified against real claude + codex binaries, see `docs/agent-contracts.md`): the orchestrator scans **stdout + stderr concatenated** against `retryable_stderr_patterns`. Field name kept for backward compat with existing `registry.yaml`. Reason: real binaries print failure text to stdout — claude emits model errors there, codex emits `ERROR: {"type":"error","status":429,...}` JSON there AND exits 0. Scanning stderr alone misses both. Three outcome buckets:

| exit code | output matches retryable | result |
|-----------|--------------------------|--------|
| 0 | no | clean success |
| 0 | yes (e.g. codex 429 JSON) | swap to fallback (exit-0-but-still-failed) |
| ≠0 | yes | normal fallback |
| ≠0 | no | non-retryable bail |

`FallbackReason::OutputMatch(pattern)` replaced PR3's `StderrMatch`. Log strings change from `stderr_match:` to `output_match:`. `fallback_history` entries written after PR9 use the new prefix.

CLI phase commands (`spec`, `testspec`, `plan`, `code`, the code half of `ship`) route through `cli::dispatch_helper::run_phase_for_task`: tasks with `effective_agent()` go to the orchestrator; legacy/pre-PR2 tasks stay on `Engine::dispatch()` (auto-detect `claude`/`opencode` on `$PATH`). `verify` never touches the orchestrator — it shells out to the test runner directly. Integration tests use the `examples/fake_agent.rs` stub driven by a JSON config path passed as the agent's args (`{exit_code, stderr, stdout, sleep_ms}`).

### Verifier-driven loop (PR4 / SWE-bench pattern)

`zforge ship <ID> --max-iterations N` (and MCP `ship` with `max_iterations > 1`) runs the code → verify cycle up to N times. On test failure, `verify.md` content + failed test names are injected into the next code prompt via `PromptContext.failed_tests` + `verify_file` + `verify_ref` in `context_files`. The `code.tmpl` template gates a "Verifier Feedback" section behind `{{if failed_tests}}...{{end}}` so retries get explicit failure context while first attempts get the clean template.

`orchestrator::verifier_loop::iterate(max_iterations, code_runner, verify_runner)` is the pure control function — closures inject the actual spawn + verify so the loop is unit-testable without real agents. `ship.rs` calls `iterate` with closures that dispatch the assigned agent through the PR3 orchestrator and call `verify::run_with_outcome` respectively. Single-shot mode (`--max-iterations 1`, default) bypasses the loop and preserves pre-PR4 behavior. Budget exhaustion = terminal error with failing test names included.

### Per-phase model routing (PR6)

`models.yaml` defines per-agent → per-phase model overrides. Two-layer precedence:

1. **Local** `<project>/.zforge/models.yaml` — wins per (agent, phase) field
2. **Global** `$ZFORGE_HOME/models.yaml` (default `~/.zforge/models.yaml`) — fallback

`ModelsConfig::overlay(local)` merges per-field: every `Some` value in local replaces the corresponding entry in global. Local can override `claude.plan` while inheriting `claude.code` from global. Missing both layers → no injection.

```yaml
# ~/.zforge/models.yaml         <project>/.zforge/models.yaml
claude:                          claude:
  plan: opus                       code: haiku    # overrides global sonnet
  code: sonnet
# Result: claude.plan=opus (global), claude.code=haiku (local)
```

Schema is `BTreeMap<String, PhaseModels>` via `#[serde(flatten)]` — any top-level YAML key becomes an agent entry. Pre-PR7 fixed `claude/codex/opencode` shape still parses unchanged. User adds custom agents (e.g. `agy`, `mystery_agent`) by appending entries to either file.

`orchestrator::run::with_model_args(base_spec, agent_name, phase, models)` reads the model name from `ModelsConfig::for_assistant(agent_name, phase)` and appends `model_args_for_agent(agent_name, model)` to `AgentSpec.args` before spawn. Per-agent CLI conventions live in `model_args.rs`:

| Agent | Args appended |
|-------|---------------|
| `claude`, `opencode`, `agy` | `--model <X>` |
| `codex` | (none — uses `~/.codex/config.toml` profiles selected by `--profile` baked into `AgentSpec.args`) |
| unknown | (none — user bakes flags into registered `AgentSpec.args`) |

User-registered `args` stay FIRST in argv; model args are appended LAST so duplicate-flag CLI parsers (left-to-right wins) honor `models.yaml` over any hardcoded model arg the user put in the registry. `models.yaml` is loaded ONCE per `run_phase` invocation (outside the retry loop) — file IO between attempts is wasted work. Missing `models.yaml`, missing phase entry, missing agent entry → no injection; behavior identical to pre-PR6.

The legacy `Engine::dispatch` path (used by tasks without `assigned_agent`) honors models.yaml via `reader::agent_model_for_dispatch` — separate code path, predates PR6, kept as-is for backward compat.

### Per-task lock + cost telemetry (PR13)

**Per-task lock** (`src/state/task_lock.rs`): one `flock` per task at `<tasks_dir>/<TASK_ID>/.task.lock`. Acquired by `job::worker::run` for the worker's lifetime; also acquired by foreground `ship::run` UNLESS `ZFORGE_HEADLESS=1` is set (worker already owns the outer lock — inner re-acquire would deadlock against self). `try_acquire` is non-blocking; conflict returns `TaskLockError::Busy { owner_pid }` so the caller exits with a clear message instead of stomping `.state.yaml`.

**Cost telemetry** (`src/cost/`): orchestrator writes one `CostEntry` per `spawn_agent` call into `<project>/.zforge/cost-log.jsonl`. Token counts are ~4-chars-per-token estimates unless an agent emits a parseable usage line (codex's `tokens used N` is captured into `reported_total_tokens`). Costs come from `prices.rs` static table — Anthropic public list price for claude haiku/sonnet/opus; codex ChatGPT Plus is $0/token (flat subscription); unknown agent/model returns 0 (treat as "missing data"). Logging is best-effort: failure does NOT block orchestration. `zforge cost report` aggregates by task / phase / agent / model with `--since` and `--task` filters and JSON export.

### Spawn timeout + headless bypass + cancel propagation (PR10–PR12)

**PR10 — spawn timeout**: every `spawn_agent` call enforces `FallbackPolicy.spawn_timeout_secs` (default 600s). Implementation uses three threads — stdin writer + stdout reader + stderr reader — wrapped by `wait_timeout` for portable timer support. Exceed → kill child, synthesize exit code 124 (GNU `timeout` convention). 124 is already in `default_retryable_exit_codes` so the fallback fires automatically. `SpawnOutcome.timed_out` flag exposed to callers that want to distinguish agent-emitted 124 from zforge-injected.

**PR11 — headless permission bypass**: `job::worker::run` sets `ZFORGE_HEADLESS=1` at entry. `orchestrator::run::is_headless` reads it; when true, `with_headless_args` PREPENDS each agent's bypass flags from `headless_args_for_agent`:

| Agent | Prepended flags |
|-------|----------------|
| claude | `--dangerously-skip-permissions` |
| codex | `-a never -s workspace-write` (top-level, must precede `exec`) |
| opencode / agy / unknown | (none — user configures via registry args) |

Foreground invocations (`zforge ship T1` without `--async`, or `zforge code T1`) do NOT set the env var → no bypass injection → user answers prompts normally.

**PR12 — cancel propagation**: worker spawned with `process_group(0)` (job::spawn). Worker's PID == its pgid. CLI `zforge job cancel` and MCP `job_cancel` send `kill(-pid, SIGTERM)` — negative arg means "signal whole process group", killing worker + any child agent subprocess together. Without this, cancelling a worker mid-spawn left orphan claude/codex subprocesses consuming API quota.

### Background jobs (PR5)

`zforge ship --async` spawns a detached `zforge worker --job-id <ID>` subprocess that runs the same `ship::run` code path; the controller exits immediately after printing the job ID. Job records live at `<project>/.zforge/jobs/<JOB-ID>/{job.yaml,log}` and are written via the same tmp+rename atomic pattern as `.state.yaml`. The worker is detached via Unix `process_group(0)` so closing the controller's terminal does not kill it; stdout+stderr are redirected into the per-job `log` file.

Polling: `zforge job {list,status,log [--follow] [--tail N],wait,cancel <ID>}` and the matching MCP tools (`ship_async`, `job_status`, `job_wait`, `job_log`, `job_cancel`, `job_list`). `reconcile_dead_worker` runs on every status read: if `job.yaml` says `running` but `kill(pid, 0)` returns ESRCH, the job is flipped to `failed` so callers don't poll forever. Cancellation sends SIGTERM, waits 5s grace, then SIGKILL, and writes `cancelled` regardless.

`spawn::spawn_worker` honors `ZFORGE_WORKER_BIN` env var so integration tests can point the spawn at `CARGO_BIN_EXE_zforge` instead of the test binary. Production runs ignore the override and use `std::env::current_exe()`. Worker entry point is `Commands::Worker { job_id }` in `main.rs` (`#[command(hide = true)]`) — internal use only, never document for end users.

## Key invariants

- `State` derives `PartialOrd`/`Ord` — comparisons (`>=`, `<`) express "at least this far in the pipeline". Use `require(State::X)` for absolute gates; use `flow_guard::ensure_predecessor_complete` when the gate depends on the active flow.
- `Flow` is immutable once a task is imported. There is no "change flow" command — import a new task if you picked wrong.
- All paths go through `Config::resolve_path()` — always absolute, always relative to the project root (parent of `.zforge/`), never to cwd.
- Memory is injected only via `/file` refs in `context_files` — there are no inline `{{patterns}}` / `{{anti_patterns}}` template variables. Variable names listed in `VAR_NAMES` (`engine.rs`) must match arms in `get_var()`.
- `approve.rs` handles testspec and plan approvals. Review approval (`zforge review <ID> --done`) is handled in `review.rs` and triggers memory extraction.
- `embedded.rs` is the single source of truth for all bundled assets. `cli::install` and `cli::init` both consume it; `prompt::engine` falls back to it when no disk `.tmpl` is found.
- Registry mutations always pair `with_lock` (advisory `flock`) with `save_atomic` (tmp + fsync + rename). Never write `registry.yaml` directly. Reads (`io::load`) are lock-free and may observe a snapshot a few ms stale — acceptable for `status --global`.
- Tests touching the registry must set `ZFORGE_HOME` to a tempdir AND use `#[serial_test::serial]`. The env var is process-global; parallel tests will clobber each other.
- `assigned_agent` is IMMUTABLE for the lifetime of a task. Orchestrator fallbacks mutate `active_agent` only — any debugging tool answers "who was originally assigned?" from `assigned_agent`. Never touch it after import.
- `run_phase` persists `.state.yaml` via tmp+rename *between* spawn attempts. Killing the process mid-fallback leaves the state file pointing at the new `active_agent` so a subsequent invocation picks up where this one stopped.
- Background jobs always write `job.yaml` atomically (tmp+rename). The polling-side `reconcile_dead_worker` is the only path that flips a stale `running` to `failed` — never mutate `job.yaml` outside `job::lifecycle::*` helpers.
- `Commands::Worker` is internal. End-user docs reference `ship --async` + `job *` only; the worker subcommand is hidden in clap output to discourage direct invocation.
