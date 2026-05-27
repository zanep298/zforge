use crate::cli::flow_guard;
use crate::cli::{artifact_metadata, dispatch_helper::run_phase_for_task};
use crate::config;
use crate::fs::{reader, tokens};
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool, copy: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id).map_err(|_| {
        anyhow::anyhow!(
            "Task {} not found. Run: zf task import {}",
            task_id,
            task_id
        )
    })?;

    flow_guard::ensure_phase_in_flow(&ts, State::SpecDone, "spec")?;
    flow_guard::ensure_predecessor_complete(&ts, State::SpecDone)?;

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
        if !reader::artifact_exists(&tasks_dir, task_id, "spec.md") {
            anyhow::bail!("spec.md not found or empty. Generate content before marking done.");
        }
        let spec_path = tasks_dir.join(task_id).join("spec.md");
        let spec_content = std::fs::read_to_string(&spec_path).unwrap_or_default();
        let spec_tokens =
            artifact_metadata::set_llm_metadata(&config, &ts, "spec", &spec_path, &spec_content)?;

        ts.advance(State::SpecDone, "spec generated")?;
        ts.save(&tasks_dir)?;
        println!(
            "{} spec.md validated  ({} tokens)",
            "✓".green(),
            tokens::fmt(spec_tokens)
        );
        println!("{} State advanced: Imported → SpecDone", "✓".green());
        println!();
        println!("Next: {}", ts.next_hint());
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Spec)?;
    ctx.output_file = format!(".zforge/tasks/{}/spec.md", task_id);
    ctx.next_command = format!("zf spec {} --done", task_id);

    if copy {
        let engine = Engine::new(&config.agents_dir());
        engine.render_and_copy("spec", &ctx)?;
    } else {
        run_phase_for_task(&config, &ts, "spec", &ctx)?;
    }

    Ok(())
}
