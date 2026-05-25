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
use crate::prompt::{Engine, PromptContext};
use crate::state::TaskState;
use anyhow::Result;

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
        // is on $PATH.
        return engine.dispatch(template_name, ctx);
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
