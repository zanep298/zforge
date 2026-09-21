//! Shared agent-dispatch policy used by every LLM-driven phase command
//! (`spec`, `testspec`, `plan`, `code`, `ship`).
//!
//! Tasks created **with** `--agent` go through the PR3 orchestrator: it
//! resolves the agent from the global registry, spawns it with stdin piping,
//! and applies the fallback policy on retryable failures. Tasks created
//! **without** an agent (pre-PR2 imports, or PR2 imports that omitted the
//! flag) fall back to the legacy `Engine::dispatch()` path that auto-detects
//! `claude` / `opencode` on `$PATH`.

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

/// Runner used for agentless tasks when routed through the orchestrator (so
/// cost telemetry is captured). Users select a different runner explicitly
/// with `zforge task import --agent <name>`.
const DEFAULT_RUNNER: &str = "claude";

/// Dispatch one LLM phase for a task. Three routes:
///
/// 1. **Explicit agent** (`--agent` at import) → orchestrator ALWAYS, so the
///    chosen runner + fallback policy + model routing are honored. The legacy
///    path auto-detects `claude`/`opencode` on `$PATH` and would silently
///    ignore the selection, so we never send an explicitly-agented task there.
/// 2. **No agent, non-interactive** (async worker / MCP / CI / piped stdout)
///    → orchestrator with `DEFAULT_RUNNER`. No human is watching, so losing
///    live streaming costs nothing and we gain real cost telemetry.
/// 3. **No agent, interactive TTY** (human at a terminal) → legacy streaming
///    dispatch. Output streams live but can't be captured, so the cost entry
///    is flagged `input-only` (unmeasured) in reports.
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

    // Route 2: agentless + non-interactive + default runner registered →
    // orchestrator with the default runner for full cost telemetry.
    let interactive = std::io::stdout().is_terminal();
    if !interactive && default_runner_available(&config.project_root()) {
        let rendered = engine.render(template_name, ctx)?;
        return crate::orchestrator::run_phase_with_lock(
            &ts.task_id,
            template_name,
            &config.project_root(),
            &rendered,
            Some(DEFAULT_RUNNER),
            held,
        );
    }

    // Route 3: legacy streaming dispatch. Preserves prior behavior — prompt
    // handed to whichever LLM binary is on $PATH, streamed to the TTY. Stdout
    // isn't captured, so cost telemetry is input-only (flagged unmeasured in
    // `zforge cost report`).
    let rendered = engine.render(template_name, ctx)?;
    eprintln!(
        "note: task {} has no assigned agent; cost telemetry will be input-only. \
         Reimport with `zforge task import --agent <name>` for full tracking.",
        ts.task_id
    );
    let started = Instant::now();
    let result = engine.dispatch(template_name, ctx);
    record_legacy_cost(
        config,
        &ts.task_id,
        template_name,
        &rendered,
        started,
        &result,
    );
    result
}

/// True when `DEFAULT_RUNNER` resolves for this project — i.e. the orchestrator
/// could actually spawn it. Uses the same `resolved_agent` path as `run_phase`
/// (project `agent_overrides` first, then global `agents{}`), so a project that
/// defines claude only via an override still routes through Route 2. When it
/// can't resolve (e.g. a project that registered only codex), the caller falls
/// back to legacy dispatch rather than hard-erroring on an unresolvable agent.
fn default_runner_available(project_root: &std::path::Path) -> bool {
    registry::io::load()
        .map(|r| r.resolved_agent(DEFAULT_RUNNER, project_root).is_some())
        .unwrap_or(false)
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
