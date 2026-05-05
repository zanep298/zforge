use crate::config;
use crate::fs::{reader, writer};
use crate::state::{State, TaskState};
use anyhow::Result;
use chrono::Local;
use colored::Colorize;
use std::io::{self, Write as IoWrite};

const VALID_ARTIFACTS: &[&str] = &["testspec", "plan", "verify"];

pub fn run(task_id: &str, artifact: &str, note: Option<String>, yes: bool) -> Result<()> {
    if !VALID_ARTIFACTS.contains(&artifact) {
        anyhow::bail!(
            "Unknown artifact '{}'. Valid: testspec, plan, verify",
            artifact
        );
    }

    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let filename = format!("{}.md", artifact);
    let filepath = tasks_dir.join(task_id).join(&filename);

    if !filepath.exists() {
        anyhow::bail!(
            "{}.md not found. Run: zf {} {} --done first",
            artifact,
            match artifact {
                "testspec" => "testspec",
                "plan" => "plan",
                "verify" => "verify",
                _ => artifact,
            },
            task_id
        );
    }

    if !reader::artifact_exists(&tasks_dir, task_id, &filename) {
        anyhow::bail!(
            "{}.md appears empty. Generate content before approving.",
            artifact
        );
    }

    if !yes {
        print!("? Approve tasks/{}/{}? [y/N] ", task_id, filename);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let now = Local::now().to_rfc3339();
    writer::set_frontmatter(&filepath, "reviewed", serde_yaml::Value::Bool(true))?;
    writer::set_frontmatter(
        &filepath,
        "reviewed_at",
        serde_yaml::Value::String(now),
    )?;
    if let Some(n) = note {
        writer::set_frontmatter(
            &filepath,
            "reviewed_note",
            serde_yaml::Value::String(n),
        )?;
    }

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    let (prev_state, next_state, next_cmd) = match artifact {
        "testspec" => (
            State::TestspecDone,
            Some(State::TestspecReviewed),
            format!("zf plan {}", task_id),
        ),
        "plan" => (
            State::Planned,
            Some(State::PlanReviewed),
            format!("zf code {}", task_id),
        ),
        "verify" => (State::Verified, None, format!("zf review {}", task_id)),
        _ => unreachable!(),
    };

    println!(
        "{} tasks/{}/{}.md marked as reviewed",
        "✓".green(),
        task_id,
        artifact
    );

    if let Some(next) = next_state {
        let from_str = prev_state.as_str().to_string();
        let to_str = next.as_str().to_string();
        if ts.state >= prev_state {
            let _ = ts.advance(next, "approved");
            ts.save(&tasks_dir)?;
            println!(
                "{} State advanced: {} → {}",
                "✓".green(),
                from_str,
                to_str
            );
        }
    }

    println!();
    println!("Next: {}", next_cmd);

    Ok(())
}
