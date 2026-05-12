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

    ts.require(State::Verified)?;

    if done {
        if !reader::artifact_exists(&tasks_dir, task_id, "review-summary.md") {
            anyhow::bail!(
                "review-summary.md not found or empty. Generate content before marking done."
            );
        }

        // Extract patterns from review-summary.md
        let summary_path = tasks_dir.join(task_id).join("review-summary.md");
        let summary = std::fs::read_to_string(&summary_path)?;
        let review_tokens = tokens::estimate(&summary);
        writer::set_frontmatter(&summary_path, "tokens", serde_yaml::Value::Number(review_tokens.into()))?;
        let (patterns_count, anti_count) = extract_and_update_memory(&config, &summary)?;

        ts.advance(State::Reviewed, "review complete")?;
        ts.save(&tasks_dir)?;

        println!("{} review-summary.md approved", "✓".green());
        if patterns_count > 0 {
            println!(
                "{} Extracted {} patterns → .zforge/memory/patterns.md",
                "✓".green(),
                patterns_count
            );
        }
        if anti_count > 0 {
            println!(
                "{} Extracted {} anti-pattern(s) → .zforge/memory/anti-patterns.md",
                "✓".green(),
                anti_count
            );
        }
        println!("{} State: Verified → Reviewed", "✓".green());
        println!();
        println!("{} {} complete!", "🎉".bold(), task_id);
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Review)?;
    ctx.output_file = format!(".zforge/tasks/{}/review-summary.md", task_id);
    ctx.next_command = format!("zf review {} --done", task_id);

    let engine = Engine::new(&config.agents_dir());
    engine.dispatch("review", &ctx)?;

    Ok(())
}

fn extract_and_update_memory(
    config: &crate::config::Config,
    summary: &str,
) -> Result<(usize, usize)> {
    let memory_dir = config.memory_dir();
    let mut patterns_count = 0usize;
    let mut anti_count = 0usize;

    let mut in_patterns = false;
    let mut in_anti = false;
    let mut patterns_buf = String::new();
    let mut anti_buf = String::new();

    for line in summary.lines() {
        if line.contains("New approved patterns") {
            in_patterns = true;
            in_anti = false;
            continue;
        }
        if line.contains("Anti-patterns discovered") {
            in_anti = true;
            in_patterns = false;
            continue;
        }
        if line.starts_with("## ") && (in_patterns || in_anti) {
            in_patterns = false;
            in_anti = false;
        }

        if in_patterns && line.trim_start().starts_with("- ") {
            patterns_buf.push_str(line);
            patterns_buf.push('\n');
            patterns_count += 1;
        }
        if in_anti && line.trim_start().starts_with("- ") {
            anti_buf.push_str(line);
            anti_buf.push('\n');
            anti_count += 1;
        }
    }

    if !patterns_buf.is_empty() {
        writer::append_to_file(
            &memory_dir.join("patterns.md"),
            &format!("\n{}", patterns_buf),
        )?;
    }
    if !anti_buf.is_empty() {
        writer::append_to_file(
            &memory_dir.join("anti-patterns.md"),
            &format!("\n{}", anti_buf),
        )?;
    }

    Ok((patterns_count, anti_count))
}
