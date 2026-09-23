//! Running a whole handover (MOC-C TASK-005; REQ-001, REQ-005, REQ-008).
//!
//! `zforge run <HANDOVER>` walks the handover's tasks in manifest order and
//! then its integration check, one run at a time. What to do next is always
//! decided from [`FeatureState`] — the run records — never from memory of
//! an earlier invocation, so running the command again after an interruption
//! continues where it stopped:
//!
//! - a verified task is skipped; a task whose dependencies stopped is left
//!   blocked, and tasks that do not depend on it go on;
//! - a task whose latest run was interrupted (its worker died) or cancelled
//!   gets a new run (`retry_of`), with what is left of its budget; a run
//!   that failed or was blocked is a verdict and stays until the user acts;
//! - within one invocation a task gets at most one new run, so a run the
//!   user cancels by id is not restarted behind their back.
//!
//! Two locks keep runs from overlapping: the handover's loop lock, held for
//! the whole command, and [`claim`], taken by every run creation — here and
//! in `--task` / `--integration` — which refuses while that task already has
//! a run that has not ended.
//!
//! The loop's own bookkeeping — its pid, a background loop's launch pid and
//! log — lives in `.zforge/runs/features/<INTAKE>/<HANDOVER>/`. It says only
//! whether a loop is alive; the state is the runs'.

use super::feature::{self, FeatureState, Progress};
use super::record::{self, RunKind, RunStatus};
use super::{contract, execute, integrate, ops, reconcile, view};
use crate::state::task_lock::{self, TaskLockError, TaskLockGuard};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const PID: &str = "pid";
pub const LAUNCH_PID: &str = "launch.pid";
pub const LOG: &str = "log";

/// The loop's bookkeeping directory.
pub fn dir(project_root: &Path, intake: &str, handover: &str) -> PathBuf {
    super::runs_dir(project_root)
        .join("features")
        .join(intake)
        .join(handover)
}

fn locks_dir(project_root: &Path, intake: &str) -> PathBuf {
    super::runs_dir(project_root).join(".locks").join(intake)
}

/// A run of this handover was stopped by a signal; the command stops too.
#[derive(Debug)]
pub struct Interrupted;

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("interrupted")
    }
}

impl std::error::Error for Interrupted {}

/// Hold the right to create a run of `task` (or the integration) in
/// `handover`, refusing while one has not ended. Held only around the
/// creation: the new run, `ready`, is what refuses the next claim.
pub fn claim(
    project_root: &Path,
    intake: &str,
    handover: &str,
    task: &str,
    kind: RunKind,
) -> Result<TaskLockGuard> {
    let guard = task_lock::try_acquire(&locks_dir(project_root, intake).join(handover), task)
        .map_err(|e| match e {
            TaskLockError::Busy { owner_pid, .. } => anyhow!(
                "another zforge process{} is starting a run of {task} in {handover}",
                owner_pid.map(|p| format!(" (pid {p})")).unwrap_or_default()
            ),
            TaskLockError::Io(e) => e,
        })?;
    for run in record::list(project_root)? {
        let meta = run.meta()?;
        if meta.intake == intake
            && meta.handover == handover
            && meta.kind == kind
            && meta.task == task
        {
            let state = ops::refresh(&run)?;
            if !state.status.is_final() {
                bail!(
                    "{task} already has {} {} in {handover}; wait for it or `zforge run cancel {}`",
                    run.id,
                    state.status.as_str(),
                    run.id
                );
            }
        }
    }
    Ok(guard)
}

/// What the loop does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Task {
        task: String,
        retry_of: Option<String>,
    },
    Integration {
        retry_of: Option<String>,
    },
    /// Something of this handover is running outside this command.
    Busy {
        what: String,
        run: String,
    },
    Done,
}

/// The next step for `f`. `own` are runs this invocation created (never
/// retried again); `refused` are tasks whose run could not be created.
pub fn next_step(f: &FeatureState, own: &BTreeSet<String>, refused: &BTreeSet<String>) -> Step {
    let act = |what: &str, p: &Progress| -> Option<Result<Option<String>, String>> {
        if refused.contains(what) {
            return None;
        }
        match p {
            Progress::Waiting { on } if on.is_empty() => Some(Ok(None)),
            Progress::Running { run } => Some(Err(run.clone())),
            Progress::Stopped {
                run,
                status,
                reason,
            } if !own.contains(run) && resumable(*status, reason.as_deref()) => {
                Some(Ok(Some(run.clone())))
            }
            _ => None,
        }
    };
    for t in &f.tasks {
        match act(&t.task, &t.progress) {
            Some(Ok(retry_of)) => {
                return Step::Task {
                    task: t.task.clone(),
                    retry_of,
                }
            }
            Some(Err(run)) => {
                return Step::Busy {
                    what: t.task.clone(),
                    run,
                }
            }
            None => {}
        }
    }
    match act(integrate::TASK, &f.integration) {
        Some(Ok(retry_of)) => Step::Integration { retry_of },
        Some(Err(run)) => Step::Busy {
            what: integrate::TASK.into(),
            run,
        },
        None => Step::Done,
    }
}

/// A run that stopped without a verdict on the work.
fn resumable(status: RunStatus, reason: Option<&str>) -> bool {
    match status {
        RunStatus::Cancelled => true,
        RunStatus::Failed => reason.is_some_and(|r| r.starts_with(reconcile::INTERRUPTED)),
        _ => false,
    }
}

/// Run handover `handover_id` as far as it goes. Returns its state at the
/// end; `Err(Interrupted)` when a signal stopped it.
pub fn run_feature(project_root: &Path, handover_id: &str) -> Result<FeatureState> {
    let h = contract::load_handover(project_root, handover_id)?;
    let (intake, id) = (h.intake.clone(), h.manifest.id.clone());
    let _lock = task_lock::try_acquire(&locks_dir(project_root, &intake), &format!("{id}.loop"))
        .map_err(|e| match e {
            TaskLockError::Busy { owner_pid, .. } => anyhow!(
                "{id} is already being run{}",
                owner_pid
                    .map(|p| format!(" by pid {p}"))
                    .unwrap_or_default()
            ),
            TaskLockError::Io(e) => e,
        })?;
    let dir = dir(project_root, &intake, &id);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    crate::state::write_atomic(
        &dir.join(PID),
        format!("{}\n", std::process::id()).as_bytes(),
    )?;
    let result = drive(project_root, &format!("{intake}/{id}"));
    let _ = std::fs::remove_file(dir.join(PID));
    result
}

fn drive(project_root: &Path, handover: &str) -> Result<FeatureState> {
    let mut own = BTreeSet::new();
    let mut refused = BTreeSet::new();
    loop {
        if crate::process::take_interrupt().is_some() {
            return Err(Interrupted.into());
        }
        let f = feature::load(project_root, handover)?;
        let (what, retry_of, created) = match next_step(&f, &own, &refused) {
            Step::Done => return Ok(f),
            Step::Busy { what, run } => bail!(
                "{what} is being run by {run} outside this command; wait for it or `zforge run cancel {run}`"
            ),
            Step::Task { task, retry_of } => {
                let r = execute::create(project_root, handover, &task, retry_of.as_deref());
                (task, retry_of, r)
            }
            Step::Integration { retry_of } => {
                let r = integrate::create(project_root, handover, retry_of.as_deref());
                (integrate::TASK.to_string(), retry_of, r)
            }
        };
        let run = match created {
            Ok(run) => run,
            Err(e) => {
                eprintln!("✗ {what}: {e:#}");
                refused.insert(what);
                continue;
            }
        };
        own.insert(run.id.clone());
        eprintln!(
            "▶ {} — {what}{}",
            run.id,
            retry_of
                .map(|r| format!(" (retries {r})"))
                .unwrap_or_default()
        );
        std::env::set_var(
            crate::process::CHILD_PGIDS_FILE_ENV,
            crate::process::child_pgids_file(&run.dir),
        );
        let state = execute::execute(project_root, &run)?;
        view::write(project_root, &run, &state)?;
        eprintln!(
            "{} {} {}{}",
            if state.status == RunStatus::Verified {
                "✓"
            } else {
                "✗"
            },
            run.id,
            state.status.as_str(),
            state
                .reason
                .as_deref()
                .and_then(|r| r.lines().next())
                .map(|r| format!(" — {r}"))
                .unwrap_or_default()
        );
        if state.status == RunStatus::Cancelled
            && state
                .reason
                .as_deref()
                .is_some_and(|r| r.contains("signal"))
        {
            return Err(Interrupted.into());
        }
    }
}

/// The pid of a live loop for this handover: the one it recorded, or a
/// background loop still starting.
pub fn loop_pid(dir: &Path) -> Option<u32> {
    [PID, LAUNCH_PID].iter().find_map(|f| {
        let pid: u32 = std::fs::read_to_string(dir.join(f))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        crate::job::lifecycle::pid_alive(pid).then_some(pid)
    })
}

/// Start the loop for `handover_id` in the background. Returns the
/// qualified handover id and the worker's pid.
pub fn spawn_async(project_root: &Path, handover_id: &str) -> Result<(String, u32)> {
    use std::process::{Command, Stdio};
    let h = contract::load_handover(project_root, handover_id)?;
    let (intake, id) = (h.intake.as_str(), h.manifest.id.as_str());
    let dir = dir(project_root, intake, id);
    if let Some(pid) = loop_pid(&dir) {
        bail!("{id} is already being run by pid {pid}");
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG))
        .with_context(|| format!("open {}/{LOG}", dir.display()))?;
    let err = log.try_clone()?;
    let exe = match std::env::var_os("ZFORGE_WORKER_BIN") {
        Some(p) => PathBuf::from(p),
        None => std::env::current_exe().context("locate zforge binary")?,
    };
    let qualified = format!("{intake}/{id}");
    let mut cmd = Command::new(exe);
    cmd.args(["run-worker", "--handover", &qualified])
        .current_dir(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let child = cmd
        .spawn()
        .with_context(|| format!("start the worker for {id}"))?;
    let pid = child.id();
    crate::state::write_atomic(&dir.join(LAUNCH_PID), format!("{pid}\n").as_bytes())?;
    Ok((qualified, pid))
}

/// Body of the hidden `zforge run-worker --handover <H>`.
pub fn worker(project_root: &Path, handover_id: &str) -> Result<FeatureState> {
    crate::process::catch_interrupts();
    let f = run_feature(project_root, handover_id)?;
    eprint!("{}", view::feature(&f));
    Ok(f)
}

/// Stop the handover's loop and every run of it that has not ended.
pub fn cancel(project_root: &Path, handover_id: &str) -> Result<FeatureState> {
    let h = contract::load_handover(project_root, handover_id)?;
    let (intake, id) = (h.intake.as_str(), h.manifest.id.as_str());
    let mut stopped = false;
    if let Some(pid) = loop_pid(&dir(project_root, intake, id)) {
        stop_loop(pid);
        stopped = true;
    }
    for run in record::list(project_root)? {
        let meta = run.meta()?;
        if meta.intake == intake && meta.handover == id && !ops::refresh(&run)?.status.is_final() {
            ops::cancel(project_root, &run)?;
            stopped = true;
        }
    }
    if !stopped {
        bail!("nothing of {id} is running");
    }
    feature::load(project_root, &format!("{intake}/{id}"))
}

/// Ask the loop to stop — its handler stops the current run's agent and
/// tests and records the run cancelled — and wait for it to exit.
fn stop_loop(pid: u32) {
    #[cfg(unix)]
    {
        // SAFETY: plain kill(2) on a pid the loop recorded; ESRCH is harmless.
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let start = std::time::Instant::now();
        while start.elapsed() < crate::job::lifecycle::CANCEL_GRACE {
            if !crate::job::lifecycle::pid_alive(pid) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // SAFETY: as above.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::feature::TaskProgress;

    fn f(tasks: Vec<(&str, Progress)>, integration: Progress) -> FeatureState {
        FeatureState {
            intake: "F".into(),
            handover: "HANDOVER-001".into(),
            tasks: tasks
                .into_iter()
                .map(|(t, progress)| TaskProgress {
                    task: t.into(),
                    progress,
                })
                .collect(),
            integration,
        }
    }

    fn ready() -> Progress {
        Progress::Waiting { on: vec![] }
    }

    fn verified(run: &str) -> Progress {
        Progress::Verified {
            run: run.into(),
            commit: Some("c".into()),
            from: vec![],
            outdated: vec![],
        }
    }

    fn stopped(run: &str, status: RunStatus, reason: &str) -> Progress {
        Progress::Stopped {
            run: run.into(),
            status,
            reason: Some(reason.into()),
        }
    }

    fn none() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn task(t: &str, retry_of: Option<&str>) -> Step {
        Step::Task {
            task: t.into(),
            retry_of: retry_of.map(String::from),
        }
    }

    #[test]
    fn tasks_go_in_order_then_the_integration() {
        let s = f(
            vec![("A", verified("RUN-001")), ("B", ready())],
            Progress::Waiting {
                on: vec!["B".into()],
            },
        );
        assert_eq!(next_step(&s, &none(), &none()), task("B", None));

        let s = f(vec![("A", verified("RUN-001"))], ready());
        assert_eq!(
            next_step(&s, &none(), &none()),
            Step::Integration { retry_of: None }
        );

        let s = f(vec![("A", verified("RUN-001"))], verified("RUN-002"));
        assert_eq!(next_step(&s, &none(), &none()), Step::Done);
    }

    /// AC-02: a blocked task and what depends on it are passed over; an
    /// independent task still runs; nothing else is left.
    #[test]
    fn verdicts_stay_and_independent_work_goes_on() {
        let s = f(
            vec![
                ("A", stopped("RUN-001", RunStatus::Blocked, "budget")),
                (
                    "B",
                    Progress::Blocked {
                        by: vec!["A".into()],
                    },
                ),
                ("D", ready()),
            ],
            Progress::Blocked {
                by: vec!["A".into()],
            },
        );
        assert_eq!(next_step(&s, &none(), &none()), task("D", None));
        let s = f(
            vec![
                (
                    "A",
                    stopped("RUN-001", RunStatus::Failed, "agent exited with 1"),
                ),
                ("D", verified("RUN-002")),
            ],
            Progress::Blocked {
                by: vec!["A".into()],
            },
        );
        assert_eq!(next_step(&s, &none(), &none()), Step::Done);
    }

    /// AC-03: interrupted and cancelled runs are resumed, once.
    #[test]
    fn interrupted_and_cancelled_runs_are_resumed_once() {
        let s = f(
            vec![(
                "B",
                stopped(
                    "RUN-002",
                    RunStatus::Failed,
                    "interrupted (worker 42 is gone)",
                ),
            )],
            Progress::Blocked {
                by: vec!["B".into()],
            },
        );
        assert_eq!(next_step(&s, &none(), &none()), task("B", Some("RUN-002")));
        let own = BTreeSet::from(["RUN-002".to_string()]);
        assert_eq!(next_step(&s, &own, &none()), Step::Done);

        let s = f(
            vec![(
                "B",
                stopped("RUN-002", RunStatus::Cancelled, "interrupted by signal 2"),
            )],
            ready(),
        );
        assert_eq!(next_step(&s, &none(), &none()), task("B", Some("RUN-002")));
    }

    #[test]
    fn a_refused_task_is_passed_over() {
        let s = f(vec![("A", ready()), ("D", ready())], ready());
        let refused = BTreeSet::from(["A".to_string()]);
        assert_eq!(next_step(&s, &none(), &refused), task("D", None));
        let refused = BTreeSet::from(["A".to_string(), "D".into(), "integration".into()]);
        assert_eq!(next_step(&s, &none(), &refused), Step::Done);
    }

    #[test]
    fn work_running_elsewhere_stops_the_loop() {
        let s = f(
            vec![(
                "A",
                Progress::Running {
                    run: "RUN-009".into(),
                },
            )],
            ready(),
        );
        assert_eq!(
            next_step(&s, &none(), &none()),
            Step::Busy {
                what: "A".into(),
                run: "RUN-009".into()
            }
        );
    }
}
