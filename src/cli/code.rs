use crate::config;
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    if ts.state < State::PlanReviewed {
        eprintln!("{} BLOCKED: plan.md requires human review", "⛔".red());
        eprintln!("Run: zf approve {} plan", task_id);
        return Ok(());
    }

    if done {
        ts.advance(State::Coded, "coding complete")?;
        ts.save(&tasks_dir)?;
        println!("{} State advanced: PlanReviewed → Coded", "✓".green());
        println!();
        println!("Next: zf verify {}", task_id);
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Code)?;
    ctx.next_command = format!("zf verify {}", task_id);

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("code", &ctx)?;

    Ok(())
}
