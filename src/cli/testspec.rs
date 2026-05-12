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

    ts.require(State::SpecDone)?;

    if !reader::artifact_exists(&tasks_dir, task_id, "spec.md") {
        anyhow::bail!(
            "spec.md not found or empty. Run: zf spec {} --done first",
            task_id
        );
    }

    if done {
        if !reader::artifact_exists(&tasks_dir, task_id, "testspec.md") {
            anyhow::bail!("testspec.md not found or empty. Generate content before marking done.");
        }
        let testspec_path = tasks_dir.join(task_id).join("testspec.md");
        let testspec_tokens =
            tokens::estimate(&std::fs::read_to_string(&testspec_path).unwrap_or_default());
        writer::set_frontmatter(
            &testspec_path,
            "tokens",
            serde_yaml::Value::Number(testspec_tokens.into()),
        )?;
        let model = reader::agent_model(&config.agents_dir(), "testspec");
        writer::set_frontmatter(&testspec_path, "model", serde_yaml::Value::String(model))?;

        ts.advance(State::TestspecDone, "testspec generated")?;
        ts.save(&tasks_dir)?;
        println!(
            "{} testspec.md validated  ({} tokens)",
            "✓".green(),
            tokens::fmt(testspec_tokens)
        );
        println!("{} State advanced: SpecDone → TestspecDone", "✓".green());
        println!();
        println!("Next: zf approve {} testspec", task_id);
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Testspec)?;
    ctx.output_file = format!(".zforge/tasks/{}/testspec.md", task_id);
    ctx.next_command = format!("zf testspec {} --done", task_id);

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("testspec", &ctx)?;

    Ok(())
}
