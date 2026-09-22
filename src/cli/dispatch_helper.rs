//! Shared agent-dispatch policy used by every LLM-driven phase command
//! (`spec`, `testspec`, `plan`, `code`, `ship`).
//!
//! Tasks created **with** `--agent` go through the PR3 orchestrator: it
//! resolves the agent from the global registry, spawns it with stdin piping,
//! and applies the fallback policy on retryable failures. Tasks created
//! **without** an agent run on the project's default runner
//! (`runner.default` in `.zforge/config.yaml`, set by `zforge init`).

use crate::config::Config;
use crate::cost::{
    log as cost_log,
    schema::{estimate_tokens, CostEntry},
};
use crate::prompt::{Engine, PromptContext};
use crate::registry;
use crate::state::{TaskLockGuard, TaskState};
use anyhow::Result;
use chrono::Utc;
use std::io::IsTerminal;
use std::time::Instant;

/// Dispatch one LLM phase for a task. Three routes:
///
/// 1. **Explicit agent** (`--agent` at import) → orchestrator ALWAYS, so the
///    chosen runner + fallback policy + model routing are honored. The legacy
///    path auto-detects `claude`/`opencode` on `$PATH` and would silently
///    ignore the selection, so we never send an explicitly-agented task there.
/// 2. **No agent, interactive TTY, runner claude/opencode** → stream the
///    phase through that runner on the terminal. Output can't be captured,
///    so the cost entry is flagged `input-only` (unmeasured) in reports.
/// 3. **No agent, anything else** (async worker / MCP / CI / piped stdout,
///    or a runner without a streaming mode such as codex) → orchestrator
///    with the default runner, so output is captured and costed.
///
/// The runner for 2 and 3 is `config.default_runner()` — the project's
/// choice, recorded at init (FIX-015). It used to be hardcoded to `claude`,
/// and when `claude` was not registered the dispatcher silently fell back
/// to auto-detecting whatever client was on `$PATH`. Now an unregistered
/// default runner is an error that says how to fix it.
///
/// `template_name` is the phase name (`"spec"`, `"testspec"`, `"plan"`,
/// `"code"`).
pub fn run_phase_for_task(
    config: &Config,
    ts: &TaskState,
    template_name: &str,
    ctx: &PromptContext,
) -> Result<()> {
    run_phase_for_task_locked(config, ts, template_name, ctx, None)
}

/// [`run_phase_for_task`] for a caller that already owns the task lock
/// (`ship`). The guard is handed to the orchestrator so its fallback writes
/// happen under the caller's lock instead of trying to re-acquire it.
pub fn run_phase_for_task_locked(
    config: &Config,
    ts: &TaskState,
    template_name: &str,
    ctx: &PromptContext,
    held: Option<&TaskLockGuard>,
) -> Result<()> {
    let engine = Engine::new(&config.agents_dir());

    // Route 1: explicit agent → orchestrator, no default needed.
    if ts.effective_agent().is_some() {
        let rendered = engine.render(template_name, ctx)?;
        return crate::orchestrator::run_phase_with_lock(
            &ts.task_id,
            template_name,
            &config.project_root(),
            &rendered,
            None,
            held,
        );
    }

    let runner = config.default_runner();

    // Route 2: human at a terminal and the runner can stream → live output.
    let interactive = std::io::stdout().is_terminal();
    if interactive && matches!(runner, "claude" | "opencode") {
        let rendered = engine.render(template_name, ctx)?;
        eprintln!(
            "note: task {} has no assigned agent; running on the project default ({runner}). \
             Cost telemetry will be input-only; import with `--agent <name>` for full tracking.",
            ts.task_id
        );
        let started = Instant::now();
        let result = engine.dispatch_with_runner(template_name, ctx, runner);
        record_legacy_cost(
            config,
            &ts.task_id,
            template_name,
            &rendered,
            started,
            &result,
        );
        // The run went to the terminal: say there is no trace for it rather
        // than leave a gap that reads like "nothing happened".
        let named = crate::orchestrator::agent_args::named_agent_for(
            runner,
            template_name,
            &config.project_root(),
        );
        let expected = crate::trace::Expected {
            named_agent: match named {
                crate::orchestrator::agent_args::NamedAgent::Use(name) => Some(name),
                _ => None,
            },
            ..Default::default()
        };
        let entry = crate::trace::uncaptured(&ts.task_id, template_name, runner, expected);
        if let Err(e) = crate::trace::log::append(&config.tasks_dir(), &entry) {
            eprintln!("warning: trace append failed: {e:#}");
        }
        return result;
    }

    // Route 3: orchestrator with the default runner.
    ensure_runner_registered(runner, &config.project_root())?;
    let rendered = engine.render(template_name, ctx)?;
    crate::orchestrator::run_phase_with_lock(
        &ts.task_id,
        template_name,
        &config.project_root(),
        &rendered,
        Some(runner),
        held,
    )
}

/// The orchestrator can only spawn a runner the registry resolves (project
/// `agent_overrides` first, then global `agents{}`).
fn ensure_runner_registered(runner: &str, project_root: &std::path::Path) -> Result<()> {
    let registry = registry::io::load()?;
    if registry.resolved_agent(runner, project_root).is_none() {
        anyhow::bail!(
            "default runner `{runner}` (runner.default in .zforge/config.yaml) is not in the \
             zforge registry. Run `zforge init --agent {runner}` to register it, or set \
             runner.default to a registered agent."
        );
    }
    Ok(())
}

/// Append a minimal CostEntry for the legacy dispatch path. Output is
/// not captured (engine streams to TTY), so `stdout_chars` + output
/// tokens stay 0 — `tokens_source = "input-only"` flags the entry so
/// reports can distinguish "missing data" from "zero output".
fn record_legacy_cost(
    config: &Config,
    task_id: &str,
    phase: &str,
    rendered_prompt: &str,
    started: Instant,
    dispatch_result: &Result<()>,
) {
    let prompt_bytes = rendered_prompt.len();
    let entry = CostEntry {
        timestamp: Utc::now(),
        task_id: task_id.to_string(),
        phase: phase.to_string(),
        agent: "(legacy-dispatch)".into(),
        model: None,
        prompt_chars: prompt_bytes,
        stdout_chars: 0,
        stderr_chars: 0,
        duration_ms: started.elapsed().as_millis(),
        exit_code: if dispatch_result.is_ok() { 0 } else { -1 },
        timed_out: false,
        est_input_tokens: estimate_tokens(prompt_bytes),
        est_output_tokens: 0,
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        reported_total_tokens: None,
        tokens_source: "input-only".into(),
        est_cost_usd: 0.0,
    };
    if let Err(e) = cost_log::record(&config.project_root(), &entry) {
        eprintln!("warning: legacy cost log append failed: {e}");
    }
}
