use crate::config;
use crate::fs::reader;
use crate::prompt::{build_context, Engine};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool, copy: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found. Run: zf task import {}", task_id, task_id))?;

    ts.require(State::Imported)?;

    // Validate task.md fields
    let task_path = tasks_dir.join(task_id).join("task.md");
    if let Ok(md) = reader::MarkdownFile::read(&task_path) {
        if md.get_str("title").unwrap_or("").is_empty() {
            eprintln!("{} Warning: task.md missing 'title'", "⚠".yellow());
        }
        if md.get_str("domain").unwrap_or("").is_empty() {
            eprintln!("{} Warning: task.md missing 'domain'", "⚠".yellow());
        }
    }

    if done {
        let spec_path = tasks_dir.join(task_id).join("spec.md");
        if !reader::artifact_exists(&tasks_dir, task_id, "spec.md") {
            anyhow::bail!(
                "spec.md not found or empty. Generate content before marking done."
            );
        }
        drop(spec_path);
        ts.advance(State::SpecDone, "spec generated")?;
        ts.save(&tasks_dir)?;
        println!("{} spec.md validated", "✓".green());
        println!("{} State advanced: Imported → SpecDone", "✓".green());
        println!();
        println!("Next: zf testspec {}", task_id);
        return Ok(());
    }

    let mut ctx = build_context(&config, task_id)?;
    ctx.output_file = format!(".zforge/tasks/{}/spec.md", task_id);
    ctx.next_command = format!("zf spec {} --done", task_id);

    let engine = Engine::new(&config.agents_dir());

    if copy {
        engine.render_and_copy("spec", &ctx)?;
    } else {
        engine.dispatch("spec", &ctx)?;
    }

    Ok(())
}
