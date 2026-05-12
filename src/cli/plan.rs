use crate::config;
use crate::fs::{reader, tokens, writer};
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

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
        let plan_tokens =
            tokens::estimate(&std::fs::read_to_string(&plan_path).unwrap_or_default());
        writer::set_frontmatter(
            &plan_path,
            "tokens",
            serde_yaml::Value::Number(plan_tokens.into()),
        )?;
        let model = reader::agent_model(&config.agents_dir(), "plan");
        writer::set_frontmatter(&plan_path, "model", serde_yaml::Value::String(model))?;

        ts.advance(State::Planned, "plan generated")?;
        ts.save(&tasks_dir)?;
        println!(
            "{} plan.md validated  ({} tokens)",
            "✓".green(),
            tokens::fmt(plan_tokens)
        );
        println!("{} State advanced: TestspecReviewed → Planned", "✓".green());
        println!();
        println!("Next: zf approve {} plan", task_id);
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Plan)?;
    ctx.output_file = format!(".zforge/tasks/{}/plan.md", task_id);
    ctx.next_command = format!("zf plan {} --done", task_id);

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("plan", &ctx)?;

    Ok(())
}
