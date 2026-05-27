use crate::cli::flow_guard;
use crate::cli::{artifact_metadata, dispatch_helper::run_phase_for_task};
use crate::config;
use crate::fs::{reader, tokens};
use crate::prompt::{build_context_for_phase, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    flow_guard::ensure_phase_in_flow(&ts, State::Planned, "plan")?;

    if ts.state < State::TestspecReviewed {
        eprintln!("{} BLOCKED: testspec.md requires human review", "⛔".red());
        eprintln!();
        eprintln!("  Review the test specification:");
        eprintln!("    .zforge/tasks/{}/testspec.md", task_id);
        eprintln!();
        eprintln!("  When done, run: zf approve {} testspec", task_id);
        return Ok(());
    }

    if done {
        if !reader::artifact_exists(&tasks_dir, task_id, "plan.md") {
            anyhow::bail!("plan.md not found or empty. Generate content before marking done.");
        }
        let plan_path = tasks_dir.join(task_id).join("plan.md");
        let plan_content = std::fs::read_to_string(&plan_path).unwrap_or_default();
        let plan_tokens =
            artifact_metadata::set_llm_metadata(&config, &ts, "plan", &plan_path, &plan_content)?;

        ts.advance(State::Planned, "plan generated")?;
        ts.save(&tasks_dir)?;
        println!(
            "{} plan.md validated  ({} tokens)",
            "✓".green(),
            tokens::fmt(plan_tokens)
        );
        println!("{} State advanced: TestspecReviewed → Planned", "✓".green());
        println!();
        println!("Next: {}", ts.next_hint());
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Plan)?;
    ctx.output_file = format!(".zforge/tasks/{}/plan.md", task_id);
    ctx.next_command = format!("zf plan {} --done", task_id);

    run_phase_for_task(&config, &ts, "plan", &ctx)?;

    Ok(())
}
