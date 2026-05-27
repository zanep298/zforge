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

    flow_guard::ensure_phase_in_flow(&ts, State::TestspecDone, "testspec")?;
    flow_guard::ensure_predecessor_complete(&ts, State::TestspecDone)?;

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
        let testspec_content = std::fs::read_to_string(&testspec_path).unwrap_or_default();
        let testspec_tokens = artifact_metadata::set_llm_metadata(
            &config,
            &ts,
            "testspec",
            &testspec_path,
            &testspec_content,
        )?;

        ts.advance(State::TestspecDone, "testspec generated")?;
        ts.save(&tasks_dir)?;
        println!(
            "{} testspec.md validated  ({} tokens)",
            "✓".green(),
            tokens::fmt(testspec_tokens)
        );
        println!("{} State advanced: SpecDone → TestspecDone", "✓".green());
        println!();
        println!("Next: {}", ts.next_hint());
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Testspec)?;
    ctx.output_file = format!(".zforge/tasks/{}/testspec.md", task_id);
    ctx.next_command = format!("zf testspec {} --done", task_id);

    run_phase_for_task(&config, &ts, "testspec", &ctx)?;

    Ok(())
}
