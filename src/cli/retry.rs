use crate::config;
use crate::note;
use crate::state::{Flow, State, TaskState};
use anyhow::Result;
use chrono::Local;
use colored::Colorize;
use std::io::{self, Write as IoWrite};

const VALID_PHASES: &[&str] = &["spec", "testspec", "plan", "code", "verify", "review"];

/// What a retry will do, computed and validated before anything is touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetryPlan {
    /// State the task is rewound to — the predecessor, *in this task's flow*,
    /// of the state the retried phase produces.
    pub target: State,
    /// Artifacts produced by the retried phase and every later phase in the
    /// flow. Backed up, then deleted.
    pub artifacts: Vec<&'static str>,
}

pub fn run(task_id: &str, from: &str, yes: bool) -> Result<()> {
    if !VALID_PHASES.contains(&from) {
        anyhow::bail!(
            "Invalid phase '{}'. Valid: spec, testspec, plan, code, verify, review",
            from
        );
    }

    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    // Retry deletes artifacts and rewrites state. Before FIX-004 it did both
    // with no lock, so a `retry` issued while `ship` was running tests wiped
    // the artifacts under it, and `ship` then saved its stale snapshot back
    // as Verified. Own the task for the whole rewind, or report Busy before
    // touching anything.
    let _task_lock = crate::state::lock_task(&tasks_dir, task_id)?;

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    // Validate before prompting, backing up or deleting: an invalid rewind
    // must leave both state and artifacts exactly as they were.
    let plan = plan_retry(&ts.flow, &ts.state, from)?;

    if !yes {
        note!("? Retry {} from [{}] phase?", task_id, from);
        note!("  Will backup and reset:");
        for a in &plan.artifacts {
            note!("  • {}", a);
        }
        let backup_ts = Local::now().to_rfc3339();
        note!("  Backup to: tasks/{}/.history/{}/", task_id, backup_ts);
        note!("  State reset to: {}", plan.target.as_str());
        note!();
        print!("  Continue? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            note!("Aborted.");
            return Ok(());
        }
    }

    let backup_ts = Local::now().to_rfc3339().replace(':', "-");
    let backup_dir = tasks_dir.join(task_id).join(".history").join(&backup_ts);
    std::fs::create_dir_all(&backup_dir)?;

    let mut backed_up = 0usize;
    for artifact in &plan.artifacts {
        let src = tasks_dir.join(task_id).join(artifact);
        if src.exists() {
            let dst = backup_dir.join(artifact);
            std::fs::copy(&src, &dst)?;
            backed_up += 1;
        }
    }

    for artifact in &plan.artifacts {
        let path = tasks_dir.join(task_id).join(artifact);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }

    let prev_state = ts.state.as_str().to_string();
    ts.reset_to(plan.target.clone(), &format!("retry from {} phase", from))?;
    ts.save(&tasks_dir)?;

    note!("{} Backed up {} artifacts", "✓".green(), backed_up);
    note!(
        "{} Reset state: {} → {}",
        "✓".green(),
        prev_state,
        plan.target.as_str()
    );
    if !plan.artifacts.is_empty() {
        note!("{} Cleared: {}", "✓".green(), plan.artifacts.join(", "));
    }
    note!();
    note!("Next: zf {} {}", from, task_id);

    Ok(())
}

/// State a phase produces when it completes.
fn produced_state(phase: &str) -> Option<State> {
    Some(match phase {
        "spec" => State::SpecDone,
        "testspec" => State::TestspecDone,
        "plan" => State::Planned,
        "code" => State::Coded,
        "verify" => State::Verified,
        "review" => State::Reviewed,
        _ => return None,
    })
}

/// Artifact written by the phase that produces `state`, if any. Approval
/// states record into the artifact they approve; `Coded` lives in the repo.
fn artifact_for(state: &State) -> Option<&'static str> {
    match state {
        State::SpecDone => Some("spec.md"),
        State::TestspecDone => Some("testspec.md"),
        State::Planned => Some("plan.md"),
        State::Verified => Some("verify.md"),
        State::Reviewed => Some("review-summary.md"),
        _ => None,
    }
}

/// Work out the rewind for `phase` on a task in `flow` currently at `current`.
///
/// Before FIX-005 the target and artifact list were a fixed table written
/// for the full flow, applied to every flow and every starting state. A
/// fixbug task retried from `code` was reset to `PlanReviewed` — a state its
/// flow does not contain — and a full-flow task at `Imported` retried from
/// `review` was "reset" forward to `Verified`.
pub(crate) fn plan_retry(flow: &Flow, current: &State, phase: &str) -> Result<RetryPlan> {
    let produced =
        produced_state(phase).ok_or_else(|| anyhow::anyhow!("Invalid phase '{phase}'"))?;

    if !flow.contains(&produced) {
        anyhow::bail!(
            "the {} flow has no {phase} phase — nothing to retry",
            flow.as_str()
        );
    }

    let target = flow.previous_of(&produced).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "the {phase} phase has no predecessor in the {} flow",
            flow.as_str()
        )
    })?;

    if target > *current {
        anyhow::bail!(
            "cannot retry {phase}: task is at {}, which is before the {phase} phase \
             (retrying would rewind to {}, i.e. move the task forward). Run the \
             earlier phases first.",
            current.as_str(),
            target.as_str()
        );
    }

    let artifacts = flow
        .states()
        .iter()
        .skip_while(|s| **s != produced)
        .filter_map(artifact_for)
        .collect();

    Ok(RetryPlan { target, artifacts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(flow: Flow, current: State, phase: &str) -> RetryPlan {
        plan_retry(&flow, &current, phase)
            .unwrap_or_else(|e| panic!("{flow:?} at {current:?} retry {phase}: {e}"))
    }

    // Full-flow results match the table this replaced, so existing users see
    // no change on the default flow.
    #[test]
    fn full_flow_matches_the_previous_table() {
        let at = State::Reviewed;
        let cases: &[(&str, State, &[&str])] = &[
            (
                "spec",
                State::Imported,
                &[
                    "spec.md",
                    "testspec.md",
                    "plan.md",
                    "verify.md",
                    "review-summary.md",
                ],
            ),
            (
                "testspec",
                State::SpecDone,
                &["testspec.md", "plan.md", "verify.md", "review-summary.md"],
            ),
            (
                "plan",
                State::TestspecReviewed,
                &["plan.md", "verify.md", "review-summary.md"],
            ),
            (
                "code",
                State::PlanReviewed,
                &["verify.md", "review-summary.md"],
            ),
            ("verify", State::Coded, &["verify.md", "review-summary.md"]),
            ("review", State::Verified, &["review-summary.md"]),
        ];
        for (phase, target, artifacts) in cases {
            let p = plan(Flow::Full, at.clone(), phase);
            assert_eq!(&p.target, target, "phase {phase}");
            assert_eq!(p.artifacts, artifacts.to_vec(), "phase {phase}");
        }
    }

    // The FIX-005 repro: fixbug has no PlanReviewed.
    #[test]
    fn fixbug_code_retry_rewinds_within_the_flow() {
        let p = plan(Flow::Fixbug, State::Coded, "code");
        assert_eq!(p.target, State::TestspecDone);
        assert!(Flow::Fixbug.contains(&p.target));
        assert_eq!(p.artifacts, vec!["verify.md"]);
    }

    #[test]
    fn fixbug_rejects_phases_it_does_not_have() {
        for phase in ["plan", "review"] {
            let err = plan_retry(&Flow::Fixbug, &State::Verified, phase).unwrap_err();
            assert!(err.to_string().contains("fixbug"), "{phase}: {err}");
        }
    }

    #[test]
    fn spike_and_docs_rewind_to_their_own_predecessors() {
        assert_eq!(
            plan(Flow::Spike, State::Coded, "code").target,
            State::SpecDone
        );
        assert_eq!(
            plan(Flow::Spike, State::Coded, "spec").target,
            State::Imported
        );
        assert_eq!(
            plan(Flow::Docs, State::Coded, "code").target,
            State::Imported
        );
    }

    #[test]
    fn docs_and_spike_have_no_verify_or_review_to_retry() {
        for flow in [Flow::Docs, Flow::Spike] {
            for phase in ["verify", "review", "testspec", "plan"] {
                assert!(
                    plan_retry(&flow, &State::Coded, phase).is_err(),
                    "{flow:?} should reject retry {phase}"
                );
            }
        }
    }

    // The other FIX-005 repro: a rewind that would move the task forward.
    #[test]
    fn retry_cannot_move_a_task_forward() {
        let err = plan_retry(&Flow::Full, &State::Imported, "review").unwrap_err();
        assert!(err.to_string().contains("move the task forward"), "{err}");
    }

    #[test]
    fn every_target_is_in_the_flow_and_not_ahead() {
        for flow in [Flow::Full, Flow::Fixbug, Flow::Spike, Flow::Docs] {
            let last = flow.states().last().unwrap().clone();
            for phase in VALID_PHASES {
                if let Ok(p) = plan_retry(&flow, &last, phase) {
                    assert!(flow.contains(&p.target), "{flow:?} {phase}");
                    assert!(p.target <= last, "{flow:?} {phase}");
                }
            }
        }
    }

    #[test]
    fn unknown_phase_is_rejected() {
        assert!(plan_retry(&Flow::Full, &State::Reviewed, "nonsense").is_err());
    }
}
