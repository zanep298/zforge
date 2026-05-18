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

    let mut ts = TaskState::load(&tasks_dir, task_id)
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
    ts.reset_to(reset_state.clone(), &format!("retry from {} phase", from))?;
    ts.save(&tasks_dir)?;

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
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "testspec" => (
            State::SpecDone,
            vec![
                "testspec.md".into(),
                "plan.md".into(),
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "plan" => (
            State::TestspecReviewed,
            vec![
                "plan.md".into(),
                "verify.md".into(),
                "review-summary.md".into(),
            ],
        ),
        "code" => (
            State::PlanReviewed,
            vec!["verify.md".into(), "review-summary.md".into()],
        ),
        "verify" => (
            State::Coded,
            vec!["verify.md".into(), "review-summary.md".into()],
        ),
        "review" => (State::Verified, vec!["review-summary.md".into()]),
        _ => (State::Imported, vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_from_spec_clears_all_artifacts() {
        let (state, artifacts) = phase_to_reset("spec");
        assert_eq!(state, State::Imported);
        assert_eq!(
            artifacts,
            vec![
                "spec.md",
                "testspec.md",
                "plan.md",
                "verify.md",
                "review-summary.md",
            ]
        );
    }

    #[test]
    fn reset_from_testspec_keeps_spec() {
        let (state, artifacts) = phase_to_reset("testspec");
        assert_eq!(state, State::SpecDone);
        assert!(!artifacts.iter().any(|a| a == "spec.md"));
        assert!(artifacts.iter().any(|a| a == "testspec.md"));
    }

    #[test]
    fn reset_from_plan_targets_testspec_reviewed() {
        let (state, artifacts) = phase_to_reset("plan");
        assert_eq!(state, State::TestspecReviewed);
        assert_eq!(artifacts, vec!["plan.md", "verify.md", "review-summary.md"]);
    }

    #[test]
    fn reset_from_code_targets_plan_reviewed() {
        let (state, _) = phase_to_reset("code");
        assert_eq!(state, State::PlanReviewed);
    }

    #[test]
    fn reset_from_verify_targets_coded() {
        let (state, artifacts) = phase_to_reset("verify");
        assert_eq!(state, State::Coded);
        assert_eq!(artifacts, vec!["verify.md", "review-summary.md"]);
    }

    #[test]
    fn reset_from_review_keeps_verify() {
        let (state, artifacts) = phase_to_reset("review");
        assert_eq!(state, State::Verified);
        assert_eq!(artifacts, vec!["review-summary.md"]);
    }

    #[test]
    fn unknown_phase_falls_back_to_imported() {
        let (state, artifacts) = phase_to_reset("nonsense");
        assert_eq!(state, State::Imported);
        assert!(artifacts.is_empty());
    }

    #[test]
    fn artifact_lists_are_monotonically_shrinking() {
        // Earlier reset phases must clear at least as many artifacts as later ones.
        let order = ["spec", "testspec", "plan", "code", "verify", "review"];
        let mut prev = usize::MAX;
        for phase in order {
            let (_, artifacts) = phase_to_reset(phase);
            assert!(
                artifacts.len() <= prev,
                "{phase} reset clears more than its predecessor"
            );
            prev = artifacts.len();
        }
    }
}
