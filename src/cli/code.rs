use crate::config;
use crate::fs::writer;
use crate::prompt::{build_context, Engine};
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
        ts.advance(State::Coded, "coding complete")?;
        ts.save(&tasks_dir)?;
        println!("{} State advanced: PlanReviewed → Coded", "✓".green());
        println!();
        println!("Next: zf verify {}", task_id);
        return Ok(());
    }

    let mut ctx = build_context(&config, task_id)?;
    ctx.output_file = format!(".zforge/tasks/{}/implementation-log.md", task_id);
    ctx.next_command = format!("zf verify {}", task_id);

    // Add context files for opencode
    let files = vec![
        format!("/file .zforge/tasks/{}/task.md", task_id),
        format!("/file .zforge/tasks/{}/spec.md", task_id),
        format!("/file .zforge/tasks/{}/testspec.md", task_id),
        format!("/file .zforge/tasks/{}/plan.md", task_id),
        "/file .zforge/memory/patterns.md".to_string(),
        "/file .zforge/memory/anti-patterns.md".to_string(),
    ];
    ctx.context_files = files;

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("code", &ctx)?;

    // Create implementation-log.md
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
    writer::write_file(&log_path, &log_content)?;

    Ok(())
}
