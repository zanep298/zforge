//! Operating runs (MOC-B TASK-005; REQ-007, REQ-008): background runs,
//! status with recovery, cancel, retry and clean.
//!
//! A background run is a detached `zforge run-worker <RUN>` in its own
//! process group, logging to the run's `log`. Its launch pid is recorded in
//! `launch.pid` by the submitter (the worker records its own pid in the
//! `started` event), and the process groups of the agents and tests it
//! starts in `child-pgids`, so `cancel` can stop all of them even when the
//! worker itself is gone — the same mechanism as `job cancel`.

use super::execute;
use super::reconcile;
use super::record::{self, Run, RunEvent, RunMeta, RunState, RunStatus};
use super::{view, worktree};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::path::Path;

pub const LAUNCH_PID: &str = "launch.pid";
pub const LOG: &str = "log";
/// How long a run may stay `ready` without a live worker before it is
/// treated as never started. Covers a submitter or foreground command that
/// died between creating the run and starting it.
pub const START_GRACE: chrono::Duration = chrono::Duration::seconds(30);

fn launch_pid(run: &Run) -> Option<u32> {
    std::fs::read_to_string(run.dir.join(LAUNCH_PID))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The run's state after recording what can be inferred: a dead worker
/// (interrupted), or a run that never started.
pub fn refresh(run: &Run) -> Result<RunState> {
    let state = reconcile::reconcile(run)?;
    if state.status != RunStatus::Ready {
        return Ok(state);
    }
    let meta = run.meta()?;
    let never_started = match launch_pid(run) {
        Some(pid) => !crate::process::pid_alive(pid),
        None => Utc::now() - meta.created_at > START_GRACE,
    };
    if !never_started {
        return Ok(state);
    }
    match run.append(&RunEvent::Cancelled {
        at: Utc::now(),
        reason: "the worker exited before starting the run".into(),
    }) {
        Ok(s) => Ok(s),
        // The worker started it in the meantime.
        Err(_) => run.state(),
    }
}

/// Every run with its meta and refreshed state, oldest first; optionally
/// only those of `handover` (`HANDOVER-001` or `<INTAKE>/HANDOVER-001`).
pub fn list(project_root: &Path, handover: Option<&str>) -> Result<Vec<(Run, RunMeta, RunState)>> {
    let mut out = Vec::new();
    for run in record::list(project_root)? {
        let meta = run.meta()?;
        if let Some(h) = handover {
            let full = format!("{}/{}", meta.intake, meta.handover);
            if h != meta.handover && h != full {
                continue;
            }
        }
        let state = refresh(&run)?;
        out.push((run, meta, state));
    }
    Ok(out)
}

/// Start `run` in the background. Returns the worker's pid.
pub fn spawn_async(project_root: &Path, run: &Run) -> Result<u32> {
    use std::process::{Command, Stdio};
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.dir.join(LOG))
        .with_context(|| format!("open {}/{LOG}", run.dir.display()))?;
    let err = log.try_clone()?;
    let exe = match std::env::var_os("ZFORGE_WORKER_BIN") {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_exe().context("locate zforge binary")?,
    };
    let mut cmd = Command::new(exe);
    cmd.arg("run-worker")
        .arg(&run.id)
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
        .with_context(|| format!("start the worker for {}", run.id))?;
    let pid = child.id();
    crate::fs::write_atomic(&run.dir.join(LAUNCH_PID), format!("{pid}\n").as_bytes())?;
    Ok(pid)
}

/// Body of the hidden `zforge run-worker <RUN>`: execute a created run,
/// recording the process groups of everything it starts.
pub fn worker(project_root: &Path, run_id: &str) -> Result<RunState> {
    let run = Run::open(project_root, run_id)?;
    std::env::set_var(
        crate::process::CHILD_PGIDS_FILE_ENV,
        crate::process::child_pgids_file(&run.dir),
    );
    crate::process::catch_interrupts();
    let state = execute::execute(project_root, &run)?;
    view::write(project_root, &run, &state)?;
    Ok(state)
}

/// Stop the run's worker and everything it started, then record it
/// `cancelled` (unless the worker recorded its own end first).
pub fn cancel(project_root: &Path, run: &Run) -> Result<RunState> {
    let state = refresh(run)?;
    if state.status.is_final() {
        bail!("{} is already {}", run.id, state.status.as_str());
    }
    if let Some(pid) = state.pid.or_else(|| launch_pid(run)) {
        stop_worker(run, pid);
        // Whatever the worker left: its group and its children's groups.
        crate::process::terminate_process_groups(pid, &crate::process::child_pgids_file(&run.dir));
    }
    let state = match run.state()? {
        s if s.status.is_final() => s,
        _ => run.append(&RunEvent::Cancelled {
            at: Utc::now(),
            reason: "cancelled by the user".into(),
        })?,
    };
    view::write(project_root, run, &state)?;
    Ok(state)
}

/// A new run of the same task from the same handover, with what is left of
/// its budget. Only a run that ended without being verified is retried.
pub fn retry(project_root: &Path, run: &Run) -> Result<Run> {
    let state = refresh(run)?;
    match state.status {
        RunStatus::Verified => bail!(
            "{} is verified; to run the task again use `zforge run <HANDOVER> --task <TASK>`",
            run.id
        ),
        s if !s.is_final() => bail!("{} is still {}; cancel it first", run.id, s.as_str()),
        _ => {}
    }
    let meta = run.meta()?;
    let handover = format!("{}/{}", meta.intake, meta.handover);
    match meta.kind {
        record::RunKind::Task => {
            execute::create(project_root, &handover, &meta.task, Some(&run.id))
        }
        record::RunKind::Integration => {
            super::integrate::create(project_root, &handover, Some(&run.id))
        }
    }
}

/// Remove a finished run's worktree, keeping its branch. Uncommitted work
/// in the worktree is committed to the run's branch first, so nothing the
/// agent did is lost.
pub fn clean(project_root: &Path, run: &Run) -> Result<Option<String>> {
    let state = refresh(run)?;
    if !state.status.is_final() {
        bail!(
            "{} is still {}; cancel it first",
            run.id,
            state.status.as_str()
        );
    }
    let meta = run.meta()?;
    if !meta.worktree.exists() {
        bail!("{} has no worktree (already cleaned?)", run.id);
    }
    let saved = commit_leftovers(&meta)?;
    worktree::remove(
        project_root,
        &worktree::Worktree {
            path: meta.worktree.clone(),
            branch: meta.branch.clone(),
        },
        false,
    )?;
    Ok(saved)
}

/// Commit anything uncommitted in the worktree to the run's branch. Returns
/// the commit id when there was something to save.
fn commit_leftovers(meta: &RunMeta) -> Result<Option<String>> {
    let message = format!("zforge: uncommitted work of {} ({})", meta.id, meta.task);
    super::git::commit_all(&meta.worktree, &message)
        .with_context(|| format!("could not save the worktree's changes to {}", meta.branch))
}

/// Ask the worker itself to stop — a foreground `zforge run` is not a
/// process-group leader, so signalling its group would miss it. Workers
/// catch the signal, stop their children and record `cancelled`; wait up to
/// the job cancel grace for that.
fn stop_worker(run: &Run, pid: u32) {
    #[cfg(unix)]
    {
        if !crate::process::pid_alive(pid) {
            return;
        }
        // SAFETY: plain kill(2) on a pid we recorded; ESRCH is harmless.
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let start = std::time::Instant::now();
        while start.elapsed() < crate::process::CANCEL_GRACE {
            let done = run.state().map(|s| s.status.is_final()).unwrap_or(false);
            if done || !crate::process::pid_alive(pid) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    #[cfg(not(unix))]
    let _ = (run, pid);
}
