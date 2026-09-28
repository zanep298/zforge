//! Runs whose worker died (MOC-B TASK-001, REQ-007).
//!
//! A run that is `running` or `verifying` while the worker that started it
//! is gone was interrupted — killed, crashed, the machine went down. It is
//! recorded as `failed` with reason `interrupted`, so nobody waits on it;
//! continuing is a new run. The process groups the worker recorded (agent,
//! tests) are stopped at the same time, so nothing it started outlives it. Checked whenever a run's state is read for
//! display, like the old background jobs did.

use super::record::{Run, RunEvent, RunState, RunStatus};
use anyhow::Result;
use chrono::Utc;

pub const INTERRUPTED: &str = "interrupted";

/// The run's state, after recording it as interrupted if its worker died.
pub fn reconcile(run: &Run) -> Result<RunState> {
    reconcile_with(
        run,
        crate::process::pid_alive,
        crate::process::terminate_process_groups,
    )
}

/// [`reconcile`] with the liveness probe and the group stopper injected, so
/// tests never signal real processes.
pub fn reconcile_with(
    run: &Run,
    alive: impl Fn(u32) -> bool,
    stop_groups: impl Fn(u32, &std::path::Path),
) -> Result<RunState> {
    let state = run.state()?;
    let active = matches!(state.status, RunStatus::Running | RunStatus::Verifying);
    match state.pid {
        Some(pid) if active && !alive(pid) => {
            let state = run.append(&RunEvent::Failed {
                at: Utc::now(),
                reason: format!("{INTERRUPTED} (worker {pid} is gone)"),
            })?;
            // What the dead worker started may still be running — an agent
            // still spending, a test suite. Stop every group it recorded.
            stop_groups(pid, &crate::process::child_pgids_file(&run.dir));
            Ok(state)
        }
        _ => Ok(state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::record::{create, tests::meta};

    fn started(pid: u32) -> (tempfile::TempDir, Run) {
        let tmp = tempfile::tempdir().unwrap();
        let run = create(tmp.path(), meta).unwrap();
        run.append(&RunEvent::Started {
            at: Utc::now(),
            pid,
        })
        .unwrap();
        (tmp, run)
    }

    /// AC-04: a dead worker means an interrupted run.
    #[test]
    fn a_running_run_whose_worker_died_fails_as_interrupted() {
        let (_t, run) = started(4242);
        let s = reconcile_with(&run, |_| false, |_, _| {}).unwrap();
        assert_eq!(s.status, RunStatus::Failed);
        assert!(s.reason.unwrap().starts_with(INTERRUPTED));
        // Recorded, not just reported: a second read sees it without probing.
        assert_eq!(
            reconcile_with(&run, |_| panic!("not probed again"), |_, _| {})
                .unwrap()
                .status,
            RunStatus::Failed
        );
    }

    /// The groups of a dead worker are stopped once, when it is recorded.
    #[test]
    fn interruption_stops_the_recorded_process_groups() {
        let (_t, run) = started(4242);
        let stopped = std::cell::RefCell::new(Vec::new());
        reconcile_with(
            &run,
            |_| false,
            |pid, file| stopped.borrow_mut().push((pid, file.to_path_buf())),
        )
        .unwrap();
        assert_eq!(
            *stopped.borrow(),
            vec![(4242, crate::process::child_pgids_file(&run.dir))]
        );
        reconcile_with(&run, |_| false, |_, _| panic!("already recorded")).unwrap();
    }

    /// AC-04: a live worker is left alone, as is a finished run.
    #[test]
    fn live_workers_and_final_runs_are_left_alone() {
        let (_t, run) = started(4242);
        assert_eq!(
            reconcile_with(&run, |_| true, |_, _| {}).unwrap().status,
            RunStatus::Running
        );

        run.append(&RunEvent::Cancelled {
            at: Utc::now(),
            reason: "user".into(),
        })
        .unwrap();
        let s = reconcile_with(&run, |_| false, |_, _| {}).unwrap();
        assert_eq!(
            (s.status, s.reason.as_deref()),
            (RunStatus::Cancelled, Some("user"))
        );
    }

    /// The real probe: this process is alive, an exited child is not. (The
    /// group stopper stays a no-op: the reaped pid may already be reused.)
    #[test]
    fn real_liveness_probe() {
        let (_t, run) = started(std::process::id());
        assert_eq!(
            reconcile_with(&run, crate::process::pid_alive, |_, _| {})
                .unwrap()
                .status,
            RunStatus::Running
        );

        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        let (_t2, gone) = started(pid);
        assert_eq!(
            reconcile_with(&gone, crate::process::pid_alive, |_, _| {})
                .unwrap()
                .status,
            RunStatus::Failed
        );
    }
}
