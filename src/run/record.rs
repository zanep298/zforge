//! The record of one run (MOC-B TASK-001, REQ-003).
//!
//! - `run.yaml` — what the run was created with ([`RunMeta`]). Written once;
//!   creating a run whose directory already exists fails.
//! - `events.jsonl` — every change, appended and synced ([`RunEvent`]). The
//!   run's state is obtained by replaying it ([`RunState`]); an event the
//!   state machine does not allow is refused when written, so the log can
//!   only describe legal histories.
//!
//! States follow workflow §6.1:
//!
//! ```text
//! ready → running → verifying → verified
//!         running/verifying → blocked | failed | cancelled
//!         verifying → running   (a failed verification, fixed within the contract)
//! ```
//!
//! `verified`, `blocked`, `failed` and `cancelled` are final: continuing
//! means a new run.

use super::runs_dir;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const META_FILE: &str = "run.yaml";
pub const EVENTS_FILE: &str = "events.jsonl";

/// What the run was created with. Never rewritten.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMeta {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub intake: String,
    pub handover: String,
    pub task: String,
    /// SHA-256 of the manifest file the run was created from.
    pub manifest_sha256: String,
    pub worktree: PathBuf,
    pub branch: String,
    pub baseline_commit: String,
    /// Budget left for this run when it was created (the handover's budget
    /// for the task, less what earlier runs of it spent).
    pub budget_usd: f64,
    pub max_iterations: u32,
    /// The run this one retries, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<String>,
    /// Where the worktree starts when not at `baseline_commit` (MOC-C
    /// TASK-002): the outputs of the task's dependencies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Start>,
    /// What the run is (MOC-C TASK-003). Absent means a task run.
    #[serde(default, skip_serializing_if = "RunKind::is_task")]
    pub kind: RunKind,
    /// The commands an integration run checks the feature with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Checks>,
}

/// A run implements one task, or checks a whole handover's outputs
/// together without an agent.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    #[default]
    Task,
    Integration,
}

impl RunKind {
    pub fn is_task(&self) -> bool {
        *self == Self::Task
    }
}

/// Commands run in order; the first to fail fails the check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checks {
    pub from: ChecksFrom,
    pub commands: Vec<String>,
}

/// Where the commands came from: the first code block of the pinned
/// breakdown's "Kiểm chứng tích hợp", or `project.test_command` when that
/// section has none.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksFrom {
    Breakdown,
    Config,
}

impl RunMeta {
    /// The commit the worktree is created at.
    pub fn start_commit(&self) -> &str {
        self.start
            .as_ref()
            .map_or(&self.baseline_commit, |s| &s.commit)
    }
}

/// A worktree created at `commit`, then with the output of every source
/// merged in, in order; a source already contained in it is not merged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Start {
    pub commit: String,
    pub from: Vec<Source>,
}

/// The sealed output of `task`'s run `run`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Source {
    pub task: String,
    pub run: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    /// The worker picked the run up.
    Started {
        at: DateTime<Utc>,
        pid: u32,
    },
    /// An agent invocation is about to start with `allotted_usd` of budget.
    /// Until its `attempt` event arrives the whole allotment counts as spent:
    /// a run killed mid-call may have spent all of it.
    AttemptStarted {
        at: DateTime<Utc>,
        n: u32,
        allotted_usd: f64,
    },
    /// One agent invocation finished. `cost_usd` is what the client
    /// reported; when it reported nothing the whole `allotted_usd` counts as
    /// spent (MOC-B 03-solution).
    Attempt {
        at: DateTime<Utc>,
        n: u32,
        exit_code: i32,
        allotted_usd: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
    },
    /// The test suite started on the worktree.
    VerifyStarted {
        at: DateTime<Utc>,
    },
    /// The suite passed on `candidate` (fingerprint of the worktree).
    /// `commit` is the run's output: the worktree sealed as a commit whose
    /// tree is `candidate` (MOC-C TASK-001). Runs verified before outputs
    /// were sealed have none.
    Verified {
        at: DateTime<Utc>,
        candidate: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
    },
    /// The suite failed; the run goes back to work within the contract.
    VerifyFailed {
        at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        failed_tests: Vec<String>,
    },
    Blocked {
        at: DateTime<Utc>,
        reason: String,
    },
    Failed {
        at: DateTime<Utc>,
        reason: String,
    },
    Cancelled {
        at: DateTime<Utc>,
        reason: String,
    },
    /// The task's contract and starting outputs are exactly those of
    /// `from_run` in `from_handover`, verified there: its output stands for
    /// this handover too, and no agent is called (MOC-C TASK-006).
    Reused {
        at: DateTime<Utc>,
        from_run: String,
        from_handover: String,
        candidate: String,
        commit: String,
    },
}

impl RunEvent {
    pub fn at(&self) -> DateTime<Utc> {
        match self {
            Self::Started { at, .. }
            | Self::AttemptStarted { at, .. }
            | Self::Attempt { at, .. }
            | Self::VerifyStarted { at }
            | Self::Verified { at, .. }
            | Self::VerifyFailed { at, .. }
            | Self::Blocked { at, .. }
            | Self::Failed { at, .. }
            | Self::Cancelled { at, .. }
            | Self::Reused { at, .. } => *at,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::AttemptStarted { .. } => "attempt_started",
            Self::Attempt { .. } => "attempt",
            Self::VerifyStarted { .. } => "verify_started",
            Self::Verified { .. } => "verified",
            Self::VerifyFailed { .. } => "verify_failed",
            Self::Blocked { .. } => "blocked",
            Self::Failed { .. } => "failed",
            Self::Cancelled { .. } => "cancelled",
            Self::Reused { .. } => "reused",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ready,
    Running,
    Verifying,
    Verified,
    Blocked,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn is_final(self) -> bool {
        matches!(
            self,
            Self::Verified | Self::Blocked | Self::Failed | Self::Cancelled
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Verifying => "verifying",
            Self::Verified => "verified",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// The run as its events describe it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunState {
    pub status: RunStatus,
    /// Why a blocked, failed or cancelled run stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// PID of the worker that started the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub attempts: u32,
    pub verifications: u32,
    /// Spent so far: reported costs, the allotment of any attempt that
    /// reported none, and the allotment of an attempt still in flight.
    pub cost_usd: f64,
    /// Allotment of the attempt started but not finished, already counted in
    /// `cost_usd`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_flight_usd: Option<f64>,
    /// Candidate of the latest verification, passed or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_candidate: Option<String>,
    /// The sealed output of a verified run: the commit whose tree was tested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// `<HANDOVER>/<RUN>` whose verified output this run reuses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reused_from: Option<String>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            status: RunStatus::Ready,
            reason: None,
            pid: None,
            attempts: 0,
            verifications: 0,
            cost_usd: 0.0,
            in_flight_usd: None,
            last_candidate: None,
            output: None,
            reused_from: None,
        }
    }
}

impl RunState {
    /// The state after `event`, or why the state machine refuses it.
    pub fn apply(&self, event: &RunEvent) -> Result<Self, String> {
        use RunStatus::*;
        let refuse = || {
            Err(format!(
                "`{}` is not allowed while the run is {}",
                event.name(),
                self.status.as_str()
            ))
        };
        if self.status.is_final() {
            return refuse();
        }
        let mut next = self.clone();
        match event {
            RunEvent::Started { pid, .. } => {
                if self.status != Ready {
                    return refuse();
                }
                next.status = Running;
                next.pid = Some(*pid);
            }
            RunEvent::AttemptStarted { allotted_usd, .. } => {
                if self.status != Running || self.in_flight_usd.is_some() {
                    return refuse();
                }
                next.in_flight_usd = Some(*allotted_usd);
                next.cost_usd += allotted_usd;
            }
            RunEvent::Attempt {
                allotted_usd,
                cost_usd,
                ..
            } => {
                if self.status != Running {
                    return refuse();
                }
                // The in-flight allotment is replaced by what was reported.
                next.cost_usd -= next.in_flight_usd.take().unwrap_or(0.0);
                next.attempts += 1;
                next.cost_usd += cost_usd.unwrap_or(*allotted_usd);
            }
            RunEvent::VerifyStarted { .. } => {
                if self.status != Running {
                    return refuse();
                }
                next.status = Verifying;
            }
            RunEvent::Verified {
                candidate, commit, ..
            } => {
                if self.status != Verifying {
                    return refuse();
                }
                next.status = Verified;
                next.verifications += 1;
                next.last_candidate = Some(candidate.clone());
                next.output = commit.clone();
            }
            RunEvent::VerifyFailed { candidate, .. } => {
                if self.status != Verifying {
                    return refuse();
                }
                next.status = Running;
                next.verifications += 1;
                next.last_candidate = candidate.clone();
            }
            RunEvent::Blocked { reason, .. } | RunEvent::Failed { reason, .. } => {
                if !matches!(self.status, Running | Verifying) {
                    return refuse();
                }
                next.status = if matches!(event, RunEvent::Blocked { .. }) {
                    Blocked
                } else {
                    Failed
                };
                next.reason = Some(reason.clone());
            }
            RunEvent::Reused {
                from_run,
                from_handover,
                candidate,
                commit,
                ..
            } => {
                if self.status != Ready {
                    return refuse();
                }
                next.status = Verified;
                next.last_candidate = Some(candidate.clone());
                next.output = Some(commit.clone());
                next.reused_from = Some(format!("{from_handover}/{from_run}"));
            }
            RunEvent::Cancelled { reason, .. } => {
                // A run can be cancelled before a worker picked it up.
                next.status = Cancelled;
                next.reason = Some(reason.clone());
            }
        }
        Ok(next)
    }

    /// Replay `events` from `ready`.
    pub fn replay(events: &[RunEvent]) -> Result<Self> {
        events
            .iter()
            .enumerate()
            .try_fold(Self::default(), |s, (i, e)| {
                s.apply(e)
                    .map_err(|why| anyhow::anyhow!("event {} of the run log: {why}", i + 1))
            })
    }
}

/// A run's directory under `.zforge/runs/`.
#[derive(Debug, Clone)]
pub struct Run {
    pub id: String,
    pub dir: PathBuf,
}

impl Run {
    pub fn open(project_root: &Path, id: &str) -> Result<Self> {
        crate::intake::validate_id(id)?;
        let dir = runs_dir(project_root).join(id);
        if !dir.join(META_FILE).is_file() {
            bail!("run {id} not found");
        }
        Ok(Self {
            id: id.to_string(),
            dir,
        })
    }

    pub fn meta(&self) -> Result<RunMeta> {
        let path = self.dir.join(META_FILE);
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    fn events_path(&self) -> PathBuf {
        self.dir.join(EVENTS_FILE)
    }

    /// Every event, oldest first. A torn last line (crash mid-append) is
    /// ignored: the event it would have recorded did not happen. Any other
    /// unreadable line is an error.
    pub fn events(&self) -> Result<Vec<RunEvent>> {
        let path = self.events_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let mut events = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            match serde_json::from_str(line) {
                Ok(e) => events.push(e),
                Err(_) if i + 1 == lines.len() && !text.ends_with('\n') => {}
                Err(e) => bail!(
                    "{} line {} is corrupt ({e}); the run log is append-only and cannot be repaired automatically",
                    path.display(),
                    i + 1
                ),
            }
        }
        Ok(events)
    }

    pub fn state(&self) -> Result<RunState> {
        RunState::replay(&self.events()?)
    }

    /// Append `event` if the run's state allows it; returns the new state.
    /// Serialized by the run's lock so two writers cannot both pass the check.
    pub fn append(&self, event: &RunEvent) -> Result<RunState> {
        let _lock = self.lock()?;
        let next = self
            .state()?
            .apply(event)
            .map_err(|why| anyhow::anyhow!("run {}: {why}", self.id))?;
        crate::fs::writer::append_record_line(&self.events_path(), &serde_json::to_string(event)?)?;
        Ok(next)
    }

    fn lock(&self) -> Result<crate::state::TaskLockGuard> {
        let parent = self.dir.parent().context("run directory has no parent")?;
        crate::state::lock_task(parent, &self.id)
    }
}

/// Create a run with the next free `RUN-nnn` id. The id is claimed by
/// creating its directory, which fails if another process got there first,
/// so concurrent creations never share an id; `run.yaml` is written with
/// `create_new`, so an existing record is never overwritten.
pub fn create(project_root: &Path, meta_for: impl Fn(&str) -> RunMeta) -> Result<Run> {
    let dir = runs_dir(project_root);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let mut n = highest_id(&dir) + 1;
    loop {
        let id = format!("RUN-{n:03}");
        let run_dir = dir.join(&id);
        match std::fs::create_dir(&run_dir) {
            Ok(()) => {
                let meta = meta_for(&id);
                if meta.id != id {
                    bail!("run meta id {} does not match the allocated {id}", meta.id);
                }
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(run_dir.join(META_FILE))
                    .with_context(|| format!("create {}/{META_FILE}", run_dir.display()))?;
                file.write_all(serde_yaml::to_string(&meta)?.as_bytes())?;
                file.sync_all()?;
                return Ok(Run { id, dir: run_dir });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e).with_context(|| format!("create {}", run_dir.display())),
        }
    }
}

fn highest_id(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_prefix("RUN-")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

/// Every run in the project, oldest first.
pub fn list(project_root: &Path) -> Result<Vec<Run>> {
    let dir = runs_dir(project_root);
    let mut runs: Vec<(u32, Run)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let n = id.strip_prefix("RUN-")?.parse::<u32>().ok()?;
            e.path()
                .join(META_FILE)
                .is_file()
                .then(|| (n, Run { id, dir: e.path() }))
        })
        .collect();
    runs.sort_by_key(|(n, _)| *n);
    Ok(runs.into_iter().map(|(_, r)| r).collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn meta(id: &str) -> RunMeta {
        RunMeta {
            id: id.to_string(),
            created_at: Utc::now(),
            intake: "MOC-B".into(),
            handover: "HANDOVER-001".into(),
            task: "TASK-002".into(),
            manifest_sha256: "m".into(),
            worktree: PathBuf::from("/w"),
            branch: format!("zforge/TASK-002/{id}"),
            baseline_commit: "c".into(),
            budget_usd: 3.0,
            max_iterations: 3,
            retry_of: None,
            start: None,
            kind: Default::default(),
            checks: None,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn attempt(n: u32, cost: Option<f64>) -> RunEvent {
        RunEvent::Attempt {
            at: now(),
            n,
            exit_code: 0,
            allotted_usd: 1.0,
            cost_usd: cost,
        }
    }

    /// AC-01: the §6.1 transitions, replayed.
    #[test]
    fn replays_the_work_state_machine() {
        use RunStatus::*;
        let events = vec![
            RunEvent::Started { at: now(), pid: 42 },
            attempt(1, Some(0.25)),
            RunEvent::VerifyStarted { at: now() },
            RunEvent::VerifyFailed {
                at: now(),
                candidate: Some("c1".into()),
                failed_tests: vec!["t".into()],
            },
            attempt(2, None),
            RunEvent::VerifyStarted { at: now() },
            RunEvent::Verified {
                at: now(),
                candidate: "c2".into(),
                commit: Some("k2".into()),
            },
        ];
        let mut s = RunState::default();
        let mut seen = vec![s.status];
        for e in &events {
            s = s.apply(e).unwrap();
            seen.push(s.status);
        }
        assert_eq!(
            seen,
            [Ready, Running, Running, Verifying, Running, Running, Verifying, Verified]
        );
        assert_eq!(s.attempts, 2);
        assert_eq!(s.verifications, 2);
        assert_eq!(s.pid, Some(42));
        assert_eq!(s.cost_usd, 1.25, "missing cost counts the whole allotment");
        assert_eq!(s.last_candidate.as_deref(), Some("c2"));
        assert_eq!(s.output.as_deref(), Some("k2"));
        assert_eq!(RunState::replay(&events).unwrap(), s);
    }

    /// MOC-C TASK-006: a reused run goes straight from ready to verified.
    #[test]
    fn a_reused_run_is_verified_without_running() {
        let reused = RunEvent::Reused {
            at: now(),
            from_run: "RUN-001".into(),
            from_handover: "HANDOVER-001".into(),
            candidate: "c1".into(),
            commit: "k1".into(),
        };
        let s = RunState::default().apply(&reused).unwrap();
        assert_eq!(s.status, RunStatus::Verified);
        assert_eq!(s.output.as_deref(), Some("k1"));
        assert_eq!(s.last_candidate.as_deref(), Some("c1"));
        assert_eq!(s.reused_from.as_deref(), Some("HANDOVER-001/RUN-001"));
        assert_eq!(s.cost_usd, 0.0);
        let running = RunState::default()
            .apply(&RunEvent::Started { at: now(), pid: 1 })
            .unwrap();
        assert!(running.apply(&reused).is_err(), "only a run not started");
    }

    /// AC-04 (MOC-C TASK-001): a `verified` line written before outputs were
    /// sealed still replays; the run simply has no output.
    #[test]
    fn a_verified_event_without_a_commit_still_reads() {
        let line = r#"{"event":"verified","at":"2026-09-20T10:00:00Z","candidate":"c1"}"#;
        let e: RunEvent = serde_json::from_str(line).unwrap();
        assert_eq!(
            e,
            RunEvent::Verified {
                at: "2026-09-20T10:00:00Z".parse().unwrap(),
                candidate: "c1".into(),
                commit: None,
            }
        );
        let written = serde_json::to_string(&e).unwrap();
        assert!(!written.contains("commit"), "{written}");
    }

    /// An attempt killed mid-call counts its whole allotment; a finished
    /// one counts what it reported.
    #[test]
    fn an_attempt_in_flight_counts_its_whole_allotment() {
        let s = RunState::default()
            .apply(&RunEvent::Started { at: now(), pid: 1 })
            .unwrap()
            .apply(&RunEvent::AttemptStarted {
                at: now(),
                n: 1,
                allotted_usd: 2.5,
            })
            .unwrap();
        assert_eq!((s.cost_usd, s.in_flight_usd), (2.5, Some(2.5)));
        assert!(
            s.apply(&RunEvent::AttemptStarted {
                at: now(),
                n: 2,
                allotted_usd: 1.0
            })
            .is_err(),
            "one attempt at a time"
        );

        let done = s
            .apply(&RunEvent::Attempt {
                at: now(),
                n: 1,
                exit_code: 0,
                allotted_usd: 2.5,
                cost_usd: Some(0.2),
            })
            .unwrap();
        assert_eq!((done.cost_usd, done.in_flight_usd), (0.2, None));

        let killed = s
            .apply(&RunEvent::Failed {
                at: now(),
                reason: "interrupted".into(),
            })
            .unwrap();
        assert_eq!(
            killed.cost_usd, 2.5,
            "the budget it could have spent is gone"
        );
    }

    /// AC-01: illegal events are refused, and final states stay final.
    #[test]
    fn refuses_what_the_state_machine_does_not_allow() {
        let ready = RunState::default();
        assert!(
            ready.apply(&attempt(1, None)).is_err(),
            "attempt before start"
        );
        assert!(ready.apply(&RunEvent::VerifyStarted { at: now() }).is_err());
        let running = ready
            .apply(&RunEvent::Started { at: now(), pid: 1 })
            .unwrap();
        assert!(running
            .apply(&RunEvent::Started { at: now(), pid: 2 })
            .is_err());
        assert!(
            running
                .apply(&RunEvent::Verified {
                    at: now(),
                    candidate: "c".into(),
                    commit: None,
                })
                .is_err(),
            "verified without verifying"
        );

        for stop in [
            RunEvent::Cancelled {
                at: now(),
                reason: "user".into(),
            },
            RunEvent::Failed {
                at: now(),
                reason: "x".into(),
            },
            RunEvent::Blocked {
                at: now(),
                reason: "budget".into(),
            },
        ] {
            let done = running.apply(&stop).unwrap();
            assert!(done.status.is_final());
            let err = done
                .apply(&RunEvent::VerifyStarted { at: now() })
                .unwrap_err();
            assert!(err.contains("not allowed while the run is"), "{err}");
            assert!(done
                .apply(&RunEvent::Cancelled {
                    at: now(),
                    reason: "again".into()
                })
                .is_err());
        }
        assert!(
            ready
                .apply(&RunEvent::Cancelled {
                    at: now(),
                    reason: "before start".into()
                })
                .is_ok(),
            "a queued run can be cancelled"
        );
        assert!(
            ready
                .apply(&RunEvent::Failed {
                    at: now(),
                    reason: "x".into()
                })
                .is_err(),
            "a run that never started cannot fail"
        );
    }

    #[test]
    fn append_checks_the_transition_and_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let run = create(tmp.path(), meta).unwrap();
        run.append(&RunEvent::Started { at: now(), pid: 7 })
            .unwrap();
        run.append(&RunEvent::Cancelled {
            at: now(),
            reason: "user".into(),
        })
        .unwrap();
        let err = run
            .append(&RunEvent::VerifyStarted { at: now() })
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not allowed while the run is cancelled"),
            "{err}"
        );
        let reopened = Run::open(tmp.path(), &run.id).unwrap();
        assert_eq!(
            reopened.events().unwrap().len(),
            2,
            "the refused event was not written"
        );
        assert_eq!(reopened.state().unwrap().status, RunStatus::Cancelled);
        assert_eq!(reopened.meta().unwrap().task, "TASK-002");
    }

    /// AC-02.
    #[test]
    fn a_torn_last_line_is_ignored_and_a_corrupt_one_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let run = create(tmp.path(), meta).unwrap();
        run.append(&RunEvent::Started { at: now(), pid: 7 })
            .unwrap();
        let path = run.dir.join(EVENTS_FILE);
        let good = std::fs::read_to_string(&path).unwrap();

        std::fs::write(&path, format!("{good}{{\"event\":\"attem")).unwrap();
        assert_eq!(run.events().unwrap().len(), 1);
        assert_eq!(run.state().unwrap().status, RunStatus::Running);

        std::fs::write(&path, format!("not json\n{good}")).unwrap();
        let err = run.events().unwrap_err().to_string();
        assert!(err.contains("line 1 is corrupt"), "{err}");

        // A complete last line that does not parse is not a torn write.
        std::fs::write(&path, format!("{good}garbage\n")).unwrap();
        assert!(run
            .events()
            .unwrap_err()
            .to_string()
            .contains("line 2 is corrupt"));
    }

    /// AC-03.
    #[test]
    fn ids_are_never_shared_and_records_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let root = root.clone();
                std::thread::spawn(move || create(&root, meta).unwrap().id)
            })
            .collect();
        let mut ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "{ids:?}");
        assert_eq!(list(&root).unwrap().len(), 8);

        let first = runs_dir(&root).join("RUN-001").join(META_FILE);
        let original = std::fs::read_to_string(&first).unwrap();
        let err = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&first)
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), original);

        assert!(
            create(&root, |_| meta("RUN-999")).is_err(),
            "meta must carry the allocated id"
        );
    }
}

#[cfg(test)]
mod torn_append_tests {
    use super::tests::meta;
    use super::*;

    /// A crash mid-append leaves a torn tail; the next append must not glue
    /// its event onto it, or every later read fails.
    #[test]
    fn appending_after_a_torn_line_keeps_the_log_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let run = create(tmp.path(), meta).unwrap();
        run.append(&RunEvent::Started {
            at: Utc::now(),
            pid: 7,
        })
        .unwrap();
        let path = run.dir.join(EVENTS_FILE);
        let good = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{good}{{\"event\":\"attem")).unwrap();

        run.append(&RunEvent::Cancelled {
            at: Utc::now(),
            reason: "user".into(),
        })
        .unwrap();
        let events = run.events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(run.state().unwrap().status, RunStatus::Cancelled);
    }
}
