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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    /// The worker picked the run up.
    Started {
        at: DateTime<Utc>,
        pid: u32,
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
    Verified {
        at: DateTime<Utc>,
        candidate: String,
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
}

impl RunEvent {
    pub fn at(&self) -> DateTime<Utc> {
        match self {
            Self::Started { at, .. }
            | Self::Attempt { at, .. }
            | Self::VerifyStarted { at }
            | Self::Verified { at, .. }
            | Self::VerifyFailed { at, .. }
            | Self::Blocked { at, .. }
            | Self::Failed { at, .. }
            | Self::Cancelled { at, .. } => *at,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::Attempt { .. } => "attempt",
            Self::VerifyStarted { .. } => "verify_started",
            Self::Verified { .. } => "verified",
            Self::VerifyFailed { .. } => "verify_failed",
            Self::Blocked { .. } => "blocked",
            Self::Failed { .. } => "failed",
            Self::Cancelled { .. } => "cancelled",
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
    /// Spent so far: reported costs, and the allotment of any attempt that
    /// reported none.
    pub cost_usd: f64,
    /// Candidate of the latest verification, passed or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_candidate: Option<String>,
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
            last_candidate: None,
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
            RunEvent::Attempt {
                allotted_usd,
                cost_usd,
                ..
            } => {
                if self.status != Running {
                    return refuse();
                }
                next.attempts += 1;
                next.cost_usd += cost_usd.unwrap_or(*allotted_usd);
            }
            RunEvent::VerifyStarted { .. } => {
                if self.status != Running {
                    return refuse();
                }
                next.status = Verifying;
            }
            RunEvent::Verified { candidate, .. } => {
                if self.status != Verifying {
                    return refuse();
                }
                next.status = Verified;
                next.verifications += 1;
                next.last_candidate = Some(candidate.clone());
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
        assert_eq!(RunState::replay(&events).unwrap(), s);
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
                    candidate: "c".into()
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
