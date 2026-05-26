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
use crate::state::TaskState;
use anyhow::Result;
use chrono::Utc;
use std::time::Instant;

/// Either spawn the task's assigned agent via the orchestrator, or fall back
/// to `Engine::dispatch` for legacy tasks. `template_name` is the phase name
/// (`"spec"`, `"testspec"`, `"plan"`, `"code"`).
pub fn run_phase_for_task(
    config: &Config,
    ts: &TaskState,
    template_name: &str,
    ctx: &PromptContext,
) -> Result<()> {
    let engine = Engine::new(&config.agents_dir());
    if ts.effective_agent().is_none() {
        // Legacy path — task imported without `--agent`. Preserves prior
        // behavior exactly: prompt rendered + handed to whichever LLM binary
        // is on $PATH. We can't capture stdout (engine.dispatch streams to
        // TTY), so cost telemetry is input-only.
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
        return result;
    }

    // Render once; orchestrator reuses the same string across every retry
    // attempt so the fallback agent sees identical context.
    let rendered = engine.render(template_name, ctx)?;
    crate::orchestrator::run_phase(
        &ts.task_id,
        template_name,
        &config.project_root(),
        &rendered,
    )
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
