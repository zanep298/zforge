use crate::cli::flow_guard;
use crate::config;
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{dispatch_command, State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    flow_guard::ensure_phase_in_flow(&ts, State::Coded, "code")?;

    if let Some(prev) = ts.flow.previous_of(&State::Coded) {
        if ts.state < *prev {
            eprintln!(
                "{} BLOCKED: {} required before code (flow: {})",
                "⛔".red(),
                prev.as_str(),
                ts.flow.as_str()
            );
            eprintln!("Run: {}", dispatch_command(prev, task_id));
            return Ok(());
        }
    }

    if done {
        let from = ts.state.as_str().to_string();
        ts.advance(State::Coded, "coding complete")?;
        ts.save(&tasks_dir)?;
        println!("{} State advanced: {} → Coded", "✓".green(), from);
        println!();
        println!("Next: {}", ts.next_hint());
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Code)?;
    ctx.next_command = format!("zf verify {}", task_id);

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("code", &ctx)?;

    Ok(())
}
