use crate::config;
use crate::fs::{reader, tokens, writer};
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use chrono::Local;
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
        let log_path = tasks_dir.join(task_id).join("implementation-log.md");
        if log_path.exists() {
            let log_tokens =
                tokens::estimate(&std::fs::read_to_string(&log_path).unwrap_or_default());
            writer::set_frontmatter(
                &log_path,
                "tokens",
                serde_yaml::Value::Number(log_tokens.into()),
            )?;
            let model = reader::agent_model(&config.agents_dir(), "code");
            writer::set_frontmatter(&log_path, "model", serde_yaml::Value::String(model))?;
            println!(
                "{} implementation-log.md  ({} tokens)",
                "✓".green(),
                tokens::fmt(log_tokens)
            );
        }

        ts.advance(State::Coded, "coding complete")?;
        ts.save(&tasks_dir)?;
        println!("{} State advanced: PlanReviewed → Coded", "✓".green());
        println!();
        println!("Next: zf verify {}", task_id);
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Code)?;
    ctx.output_file = format!(".zforge/tasks/{}/implementation-log.md", task_id);
    ctx.next_command = format!("zf verify {}", task_id);

    // Create implementation-log.md before dispatch so the agent can append to it.
    let now = Local::now();
    let log_content = format!(
        r#"---
id: "{}"
type: implementation-log
started_at: "{}"
---

## Implementation Log

### {} — Started
Coding phase initiated.
"#,
        task_id,
        now.to_rfc3339(),
        now.format("%Y-%m-%d %H:%M")
    );
    let log_path = tasks_dir.join(task_id).join("implementation-log.md");
    if !log_path.exists() {
        writer::write_file(&log_path, &log_content)?;
    }

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("code", &ctx)?;

    Ok(())
}
