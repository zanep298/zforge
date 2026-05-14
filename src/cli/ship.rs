use crate::cli;
use crate::config;
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

/// Run code + verify back-to-back. Idempotent: skips code if state already Coded.
///
/// Saves orchestrator round trips when chained via MCP; saves typing when run from a shell.
pub fn run(task_id: &str, command: Option<String>, timeout: u64) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    if ts.state < State::PlanReviewed {
        eprintln!("{} BLOCKED: plan.md requires human review", "⛔".red());
        eprintln!("Run: zf approve {} plan", task_id);
        return Ok(());
    }

    if ts.state < State::Coded {
        let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Code)?;
        ctx.next_command = format!("(auto) zf verify {}", task_id);

        let engine = Engine::new(&config.agents_dir());
        engine.dispatch("code", &ctx)?;

        ts.advance(State::Coded, "code phase complete (ship)")?;
        ts.save(&tasks_dir)?;
        println!("{} State advanced: PlanReviewed → Coded", "✓".green());
        println!();
    } else {
        println!(
            "{} State already {}, skipping code phase",
            "ℹ".blue(),
            ts.state.as_str()
        );
    }

    cli::verify::run(task_id, command, timeout)
}
