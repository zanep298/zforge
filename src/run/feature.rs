//! The state of a whole handover (MOC-C TASK-004; REQ-005, REQ-007).
//!
//! There is no feature state file. What each task and the integration check
//! stand at is derived — by [`FeatureState::derive`], a pure function — from
//! the handover's tasks, their dependencies and the records of the runs, so
//! it can never disagree with them.
//!
//! For each task, in manifest order:
//!
//! - a verified run → `verified` (the newest verified run wins over any
//!   later failure: its output stands);
//! - otherwise its latest run: still going → `running`, ended → `stopped`;
//! - no run: a dependency stopped or blocked → `blocked` by the task(s)
//!   where it stopped; dependencies not yet verified → `waiting` on them;
//!   none left → `waiting` on nothing, i.e. ready.
//!
//! A verified task or integration built on an output that has since been
//! replaced by a newer verified run of that dependency says so
//! (`outdated`); nothing reruns it on that account.

use super::record::{RunKind, RunMeta, RunState, RunStatus, Source};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Progress {
    /// Not run; `on` are the dependencies still to be verified (empty: it
    /// can start).
    Waiting {
        on: Vec<String>,
    },
    Running {
        run: String,
    },
    Verified {
        run: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        /// The outputs it started from.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        from: Vec<Source>,
        /// Tasks among `from` whose output has since been replaced.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        outdated: Vec<String>,
        /// `<HANDOVER>/<RUN>` whose output this run reused (TASK-006).
        #[serde(skip_serializing_if = "Option::is_none")]
        reused_from: Option<String>,
    },
    /// The latest run ended without being verified.
    Stopped {
        run: String,
        status: RunStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Cannot run: `by` are the tasks where the chain stopped.
    Blocked {
        by: Vec<String>,
    },
}

impl Progress {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Waiting { on } if on.is_empty() => "ready",
            Self::Waiting { .. } => "waiting",
            Self::Running { .. } => "running",
            Self::Verified {
                reused_from: Some(_),
                ..
            } => "reused",
            Self::Verified { .. } => "verified",
            Self::Stopped { status, .. } => status.as_str(),
            Self::Blocked { .. } => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskProgress {
    pub task: String,
    #[serde(flatten)]
    pub progress: Progress,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureState {
    pub intake: String,
    pub handover: String,
    pub tasks: Vec<TaskProgress>,
    pub integration: Progress,
}

/// What `derive` needs to know about the handover.
pub struct Plan<'a> {
    /// Tasks in manifest (dependency) order.
    pub tasks: &'a [String],
    pub depends_on: &'a BTreeMap<String, Vec<String>>,
}

impl FeatureState {
    /// `runs` may hold runs of other handovers; only this one's count.
    pub fn derive(intake: &str, handover: &str, plan: &Plan, runs: &[(RunMeta, RunState)]) -> Self {
        let mine: Vec<&(RunMeta, RunState)> = runs
            .iter()
            .filter(|(m, _)| m.intake == intake && m.handover == handover)
            .collect();
        let of_task = |kind: RunKind, task: &str| -> Vec<&(RunMeta, RunState)> {
            mine.iter()
                .copied()
                .filter(|(m, _)| m.kind == kind && (kind == RunKind::Integration || m.task == task))
                .collect()
        };

        let mut done: BTreeMap<&str, Progress> = BTreeMap::new();
        let mut outputs: BTreeMap<&str, String> = BTreeMap::new();
        for task in plan.tasks {
            let runs = of_task(RunKind::Task, task);
            let deps = plan.depends_on.get(task).map(Vec::as_slice).unwrap_or(&[]);
            let progress = from_runs(&runs).unwrap_or_else(|| before_running(deps, &done));
            if let Progress::Verified {
                commit: Some(c), ..
            } = &progress
            {
                outputs.insert(task, c.clone());
            }
            done.insert(task, progress);
        }

        let integration = from_runs(&of_task(RunKind::Integration, "")).unwrap_or_else(|| {
            let blocked = roots(plan.tasks.iter().map(String::as_str), &done);
            if !blocked.is_empty() {
                return Progress::Blocked { by: blocked };
            }
            Progress::Waiting {
                on: unverified(plan.tasks.iter().map(String::as_str), &done),
            }
        });

        let mark = |p: Progress| match p {
            Progress::Verified {
                run,
                commit,
                from,
                reused_from,
                ..
            } => {
                let outdated = from
                    .iter()
                    .filter(|s| outputs.get(s.task.as_str()).is_some_and(|c| *c != s.commit))
                    .map(|s| s.task.clone())
                    .collect();
                Progress::Verified {
                    run,
                    commit,
                    from,
                    outdated,
                    reused_from,
                }
            }
            other => other,
        };
        let tasks = plan
            .tasks
            .iter()
            .map(|t| TaskProgress {
                task: t.clone(),
                progress: mark(done.remove(t.as_str()).expect("derived above")),
            })
            .collect();
        Self {
            intake: intake.to_string(),
            handover: handover.to_string(),
            tasks,
            integration: mark(integration),
        }
    }

    /// The feature passed its integration check.
    pub fn is_verified(&self) -> bool {
        matches!(self.integration, Progress::Verified { .. })
    }
}

/// The state of handover `handover_id` now. Runs whose worker is gone are
/// recorded as interrupted first, as `run status` does.
pub fn load(project_root: &Path, handover_id: &str) -> Result<FeatureState> {
    let h = super::contract::load_handover(project_root, handover_id)?;
    let depends_on = h
        .manifest
        .tasks
        .iter()
        .map(|t| Ok((t.clone(), h.depends_on(t)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut runs = Vec::new();
    for run in super::record::list(project_root)? {
        let meta = run.meta()?;
        if meta.intake == h.intake && meta.handover == h.manifest.id {
            let state = super::ops::refresh(&run)?;
            runs.push((meta, state));
        }
    }
    Ok(FeatureState::derive(
        &h.intake,
        &h.manifest.id,
        &Plan {
            tasks: &h.manifest.tasks,
            depends_on: &depends_on,
        },
        &runs,
    ))
}

/// What a task's (or the integration's) own runs say, oldest first; `None`
/// when it has none.
fn from_runs(runs: &[&(RunMeta, RunState)]) -> Option<Progress> {
    if let Some((m, s)) = runs
        .iter()
        .rev()
        .find(|(_, s)| s.status == RunStatus::Verified)
    {
        return Some(Progress::Verified {
            run: m.id.clone(),
            commit: s.output.clone(),
            from: m.start.as_ref().map(|s| s.from.clone()).unwrap_or_default(),
            outdated: Vec::new(),
            reused_from: s.reused_from.clone(),
        });
    }
    let (m, s) = runs.last()?;
    Some(match s.status.is_final() {
        false => Progress::Running { run: m.id.clone() },
        true => Progress::Stopped {
            run: m.id.clone(),
            status: s.status,
            reason: s.reason.clone(),
        },
    })
}

/// A task with no run yet, given its dependencies' progress.
fn before_running(deps: &[String], done: &BTreeMap<&str, Progress>) -> Progress {
    let blocked = roots(deps.iter().map(String::as_str), done);
    if !blocked.is_empty() {
        return Progress::Blocked { by: blocked };
    }
    Progress::Waiting {
        on: unverified(deps.iter().map(String::as_str), done),
    }
}

/// The tasks where the chain through `tasks` stopped, without repeats.
fn roots<'a>(tasks: impl Iterator<Item = &'a str>, done: &BTreeMap<&str, Progress>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tasks {
        let found = match done.get(t) {
            Some(Progress::Stopped { .. }) => vec![t.to_string()],
            Some(Progress::Blocked { by }) => by.clone(),
            _ => Vec::new(),
        };
        for r in found {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out
}

fn unverified<'a>(
    tasks: impl Iterator<Item = &'a str>,
    done: &BTreeMap<&str, Progress>,
) -> Vec<String> {
    tasks
        .filter(|t| !matches!(done.get(t), Some(Progress::Verified { .. })))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::record::{RunEvent, Start};
    use chrono::Utc;
    use std::path::PathBuf;

    fn meta(id: &str, task: &str, kind: RunKind, from: &[(&str, &str)]) -> RunMeta {
        RunMeta {
            id: id.into(),
            created_at: Utc::now(),
            intake: "F".into(),
            handover: "HANDOVER-001".into(),
            task: task.into(),
            manifest_sha256: "m".into(),
            worktree: PathBuf::from("/w"),
            branch: "b".into(),
            baseline_commit: "base".into(),
            budget_usd: 1.0,
            max_iterations: 1,
            retry_of: None,
            start: (!from.is_empty()).then(|| Start {
                commit: from[0].1.into(),
                from: from
                    .iter()
                    .map(|(t, c)| Source {
                        task: (*t).into(),
                        run: format!("run-of-{t}"),
                        commit: (*c).into(),
                    })
                    .collect(),
            }),
            kind,
            checks: None,
        }
    }

    fn state(events: Vec<RunEvent>) -> RunState {
        RunState::replay(&events).unwrap()
    }

    fn running() -> RunState {
        state(vec![RunEvent::Started {
            at: Utc::now(),
            pid: 1,
        }])
    }

    fn ended(end: RunEvent) -> RunState {
        state(vec![
            RunEvent::Started {
                at: Utc::now(),
                pid: 1,
            },
            end,
        ])
    }

    fn verified(commit: &str) -> RunState {
        let at = Utc::now();
        state(vec![
            RunEvent::Started { at, pid: 1 },
            RunEvent::VerifyStarted { at },
            RunEvent::Verified {
                at,
                candidate: "c".into(),
                commit: Some(commit.into()),
                tolerated: Vec::new(),
                known_passing: Vec::new(),
            },
        ])
    }

    fn blocked() -> RunState {
        ended(RunEvent::Blocked {
            at: Utc::now(),
            reason: "budget: $1.00 of $1.00 used".into(),
        })
    }

    fn failed() -> RunState {
        ended(RunEvent::Failed {
            at: Utc::now(),
            reason: "x".into(),
        })
    }

    /// A ← B ← C, D independent.
    fn derive(runs: &[(RunMeta, RunState)]) -> FeatureState {
        let tasks: Vec<String> = ["A", "B", "C", "D"].map(String::from).to_vec();
        let depends_on = BTreeMap::from([
            ("A".to_string(), vec![]),
            ("B".to_string(), vec!["A".to_string()]),
            ("C".to_string(), vec!["B".to_string()]),
            ("D".to_string(), vec![]),
        ]);
        FeatureState::derive(
            "F",
            "HANDOVER-001",
            &Plan {
                tasks: &tasks,
                depends_on: &depends_on,
            },
            runs,
        )
    }

    fn task(f: &FeatureState, t: &str) -> Progress {
        f.tasks
            .iter()
            .find(|p| p.task == t)
            .unwrap()
            .progress
            .clone()
    }

    fn task_run(id: &str, t: &str, from: &[(&str, &str)], s: RunState) -> (RunMeta, RunState) {
        (meta(id, t, RunKind::Task, from), s)
    }

    #[test]
    fn nothing_run_yet() {
        let f = derive(&[]);
        assert_eq!(task(&f, "A"), Progress::Waiting { on: vec![] });
        assert_eq!(task(&f, "A").label(), "ready");
        assert_eq!(
            task(&f, "B"),
            Progress::Waiting {
                on: vec!["A".into()]
            }
        );
        assert_eq!(task(&f, "D").label(), "ready");
        assert_eq!(
            f.integration,
            Progress::Waiting {
                on: ["A", "B", "C", "D"].map(String::from).to_vec()
            }
        );
        assert!(!f.is_verified());
    }

    #[test]
    fn a_chain_in_progress() {
        let f = derive(&[
            task_run("RUN-001", "A", &[], verified("ca")),
            task_run("RUN-002", "B", &[("A", "ca")], verified("cb")),
            task_run("RUN-003", "C", &[("B", "cb")], running()),
        ]);
        assert!(matches!(task(&f, "A"), Progress::Verified { ref run, .. } if run == "RUN-001"));
        let Progress::Verified { from, outdated, .. } = task(&f, "B") else {
            panic!()
        };
        assert_eq!(from[0].task, "A");
        assert!(outdated.is_empty());
        assert_eq!(
            task(&f, "C"),
            Progress::Running {
                run: "RUN-003".into()
            }
        );
        assert_eq!(task(&f, "D").label(), "ready");
        assert_eq!(
            f.integration,
            Progress::Waiting {
                on: vec!["C".into(), "D".into()]
            }
        );
    }

    /// AC-01: a stopped task blocks everything downstream, and only that.
    #[test]
    fn a_blocked_task_blocks_its_dependents_and_the_integration() {
        let f = derive(&[
            task_run("RUN-001", "A", &[], blocked()),
            task_run("RUN-002", "D", &[], verified("cd")),
        ]);
        assert_eq!(task(&f, "A").label(), "blocked");
        assert!(matches!(task(&f, "A"), Progress::Stopped { .. }));
        assert_eq!(
            task(&f, "B"),
            Progress::Blocked {
                by: vec!["A".into()]
            }
        );
        assert_eq!(
            task(&f, "C"),
            Progress::Blocked {
                by: vec!["A".into()]
            }
        );
        assert_eq!(task(&f, "D").label(), "verified");
        assert_eq!(
            f.integration,
            Progress::Blocked {
                by: vec!["A".into()]
            }
        );
    }

    /// AC-02: a verified run stands over a failed one, before or after it.
    #[test]
    fn a_verified_run_wins_over_failures() {
        let f = derive(&[
            task_run("RUN-001", "A", &[], failed()),
            task_run("RUN-002", "A", &[], verified("ca")),
            task_run("RUN-003", "D", &[], verified("cd1")),
            task_run("RUN-004", "D", &[], failed()),
        ]);
        assert_eq!(task(&f, "A").label(), "verified");
        assert!(
            matches!(task(&f, "D"), Progress::Verified { commit: Some(ref c), .. } if c == "cd1")
        );
    }

    #[test]
    fn a_task_whose_latest_run_failed_is_stopped() {
        let f = derive(&[task_run("RUN-001", "D", &[], failed())]);
        assert_eq!(
            task(&f, "D"),
            Progress::Stopped {
                run: "RUN-001".into(),
                status: RunStatus::Failed,
                reason: Some("x".into())
            }
        );
        assert_eq!(
            f.integration,
            Progress::Blocked {
                by: vec!["D".into()]
            }
        );
    }

    #[test]
    fn the_integration_run_decides_the_feature() {
        let base = vec![
            task_run("RUN-001", "A", &[], verified("ca")),
            task_run("RUN-002", "B", &[("A", "ca")], verified("cb")),
            task_run("RUN-003", "C", &[("B", "cb")], verified("cc")),
            task_run("RUN-004", "D", &[], verified("cd")),
        ];
        let integration = |s: RunState| {
            (
                meta(
                    "RUN-005",
                    "integration",
                    RunKind::Integration,
                    &[("C", "cc"), ("D", "cd")],
                ),
                s,
            )
        };

        let f = derive(&[base.clone(), vec![integration(failed())]].concat());
        assert_eq!(f.integration.label(), "failed");
        assert!(!f.is_verified());

        let f = derive(&[base.clone(), vec![integration(verified("ci"))]].concat());
        assert!(f.is_verified());
        let Progress::Verified { outdated, .. } = &f.integration else {
            panic!()
        };
        assert!(outdated.is_empty());
    }

    /// A newer verified run of a dependency leaves what was built on the
    /// older output standing, marked outdated.
    #[test]
    fn a_replaced_output_marks_what_was_built_on_it() {
        let f = derive(&[
            task_run("RUN-001", "A", &[], verified("ca1")),
            task_run("RUN-002", "B", &[("A", "ca1")], verified("cb")),
            task_run("RUN-003", "A", &[], verified("ca2")),
        ]);
        let Progress::Verified { outdated, .. } = task(&f, "B") else {
            panic!()
        };
        assert_eq!(outdated, ["A"]);
    }

    #[test]
    fn other_handovers_do_not_count() {
        let mut other = task_run("RUN-001", "A", &[], verified("ca"));
        other.0.handover = "HANDOVER-002".into();
        let f = derive(&[other]);
        assert_eq!(task(&f, "A").label(), "ready");
    }
}
