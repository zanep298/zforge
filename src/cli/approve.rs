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
    writer::set_frontmatter(&filepath, "reviewed_at", serde_yaml::Value::String(now))?;
    if let Some(n) = note {
        writer::set_frontmatter(&filepath, "reviewed_note", serde_yaml::Value::String(n))?;
    }

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    let transition = artifact_transition(artifact, task_id).expect("validated above");
    let prev_state = transition.prev_state;
    let next_state = transition.next_state;

    // Reject approvals whose target state isn't part of the task's flow.
    // E.g. `approve testspec` on a fixbug task — fixbug skips TestspecReviewed.
    if let Some(next) = &next_state {
        if !ts.flow.contains(next) {
            anyhow::bail!(
                "approve '{}' is not part of the '{}' flow for task {}",
                artifact,
                ts.flow.as_str(),
                task_id
            );
        }
    }

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
            println!("{} State advanced: {} → {}", "✓".green(), from_str, to_str);
        }
    }

    println!();
    println!("Next: {}", ts.next_hint());

    Ok(())
}

/// State transition triggered by approving an artifact. `next_state` is `None`
/// for terminal approvals (e.g. `verify`, which only stamps `reviewed` without
/// advancing the FSM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApproveTransition {
    pub prev_state: State,
    pub next_state: Option<State>,
    pub next_cmd: String,
}

pub(crate) fn artifact_transition(artifact: &str, task_id: &str) -> Option<ApproveTransition> {
    match artifact {
        "testspec" => Some(ApproveTransition {
            prev_state: State::TestspecDone,
            next_state: Some(State::TestspecReviewed),
            next_cmd: format!("zf plan {}", task_id),
        }),
        "plan" => Some(ApproveTransition {
            prev_state: State::Planned,
            next_state: Some(State::PlanReviewed),
            next_cmd: format!("zf code {}", task_id),
        }),
        "verify" => Some(ApproveTransition {
            prev_state: State::Verified,
            next_state: None,
            next_cmd: format!("zf review {}", task_id),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testspec_transition_advances_to_reviewed() {
        let t = artifact_transition("testspec", "TASK-1").unwrap();
        assert_eq!(t.prev_state, State::TestspecDone);
        assert_eq!(t.next_state, Some(State::TestspecReviewed));
        assert_eq!(t.next_cmd, "zf plan TASK-1");
    }

    #[test]
    fn plan_transition_advances_to_reviewed() {
        let t = artifact_transition("plan", "PROJ-42").unwrap();
        assert_eq!(t.prev_state, State::Planned);
        assert_eq!(t.next_state, Some(State::PlanReviewed));
        assert_eq!(t.next_cmd, "zf code PROJ-42");
    }

    #[test]
    fn verify_transition_does_not_advance() {
        let t = artifact_transition("verify", "TASK-9").unwrap();
        assert_eq!(t.prev_state, State::Verified);
        assert!(t.next_state.is_none());
        assert_eq!(t.next_cmd, "zf review TASK-9");
    }

    #[test]
    fn unknown_artifact_returns_none() {
        assert!(artifact_transition("nonsense", "TASK-1").is_none());
        assert!(artifact_transition("", "TASK-1").is_none());
    }

    #[test]
    fn valid_artifacts_match_transition_map() {
        for artifact in VALID_ARTIFACTS {
            assert!(
                artifact_transition(artifact, "TASK-1").is_some(),
                "VALID_ARTIFACTS lists {artifact} but artifact_transition does not handle it"
            );
        }
    }

    #[test]
    fn rejects_invalid_artifact_at_entry() {
        let err = run("TASK-1", "frobnicate", None, true).unwrap_err();
        assert!(err.to_string().contains("Unknown artifact"));
    }
}
