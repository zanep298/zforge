use crate::config;
use crate::state::{State, TaskState};
use anyhow::Result;
use chrono::Local;
use colored::Colorize;
use std::io::{self, Write as IoWrite};

const VALID_PHASES: &[&str] = &["spec", "testspec", "plan", "code", "verify", "review"];

pub fn run(task_id: &str, from: &str, yes: bool) -> Result<()> {
    if !VALID_PHASES.contains(&from) {
        anyhow::bail!(
            "Invalid phase '{}'. Valid: spec, testspec, plan, code, verify, review",
            from
        );
    }

    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    let (reset_state, artifacts_to_clear) = phase_to_reset(from);

    if !yes {
        println!("? Retry {} from [{}] phase?", task_id, from);
        println!("  Will backup and reset:");
        for a in &artifacts_to_clear {
            println!("  • {}", a);
        }
        let backup_ts = Local::now().to_rfc3339();
        println!("  Backup to: tasks/{}/.history/{}/", task_id, backup_ts);
        println!("  State reset to: {}", reset_state.as_str());
        println!();
        print!("  Continue? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let backup_ts = Local::now().to_rfc3339().replace(':', "-");
    let backup_dir = tasks_dir.join(task_id).join(".history").join(&backup_ts);
    std::fs::create_dir_all(&backup_dir)?;

    let mut backed_up = 0usize;
    for artifact in &artifacts_to_clear {
        let src = tasks_dir.join(task_id).join(artifact);
        if src.exists() {
            let dst = backup_dir.join(artifact);
            std::fs::copy(&src, &dst)?;
            backed_up += 1;
        }
    }

    for artifact in &artifacts_to_clear {
        let path = tasks_dir.join(task_id).join(artifact);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }

    let prev_state = ts.state.as_str().to_string();
    let mut new_ts = TaskState::new(task_id);
    // Reset to the desired state by building history
    new_ts.state = reset_state.clone();
    new_ts.history = vec![crate::state::StateEntry {
        state: reset_state.clone(),
        at: Local::now(),
        note: format!("retry from {} phase", from),
    }];
    new_ts.updated_at = Local::now();
    new_ts.save(&tasks_dir)?;

    println!("{} Backed up {} artifacts", "✓".green(), backed_up);
    println!(
        "{} Reset state: {} → {}",
        "✓".green(),
        prev_state,
        reset_state.as_str()
    );
    if !artifacts_to_clear.is_empty() {
        println!("{} Cleared: {}", "✓".green(), artifacts_to_clear.join(", "));
    }
    println!();
    println!("Next: zf {} {}", from, task_id);

    Ok(())
}

fn phase_to_reset(phase: &str) -> (State, Vec<String>) {
    match phase {
        "spec" => (
            State::Imported,
            vec![
                "spec.md".into(),
                "testspec.md".into(),
                "plan.md".into(),
                "implementation-log.md".into(),
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "testspec" => (
            State::SpecDone,
            vec![
                "testspec.md".into(),
                "plan.md".into(),
                "implementation-log.md".into(),
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "plan" => (
            State::TestspecReviewed,
            vec![
                "plan.md".into(),
                "implementation-log.md".into(),
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "code" => (
            State::PlanReviewed,
            vec![
                "implementation-log.md".into(),
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "verify" => (
            State::Coded,
            vec!["verify.md".into(), "review-summary.md".into()],
        ),
        "review" => (State::Verified, vec!["review-summary.md".into()]),
        _ => (State::Imported, vec![]),
    }
}
