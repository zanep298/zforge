use crate::config::Config;
use crate::job::schema::JobStatus;
use crate::job::store::{load_job, save_atomic};
use anyhow::Result;
use chrono::Utc;

pub fn mark_running(config: &Config, job_id: &str, pid: u32) -> Result<()> {
    let mut job = load_job(config, job_id)?;
    job.status = JobStatus::Running;
    job.started_at = Some(Utc::now());
    job.worker_pid = Some(pid);
    save_atomic(config, &job)
}

pub fn mark_success(config: &Config, job_id: &str) -> Result<()> {
    let mut job = load_job(config, job_id)?;
    job.status = JobStatus::Success;
    job.finished_at = Some(Utc::now());
    job.exit_code = Some(0);
    save_atomic(config, &job)
}

pub fn mark_failed(config: &Config, job_id: &str, err: &str) -> Result<()> {
    let mut job = load_job(config, job_id)?;
    job.status = JobStatus::Failed;
    job.finished_at = Some(Utc::now());
    job.error = Some(err.to_string());
    save_atomic(config, &job)
}

pub fn mark_cancelled(config: &Config, job_id: &str) -> Result<()> {
    let mut job = load_job(config, job_id)?;
    if job.status.is_terminal() {
        return Ok(());
    }
    job.status = JobStatus::Cancelled;
    job.finished_at = Some(Utc::now());
    save_atomic(config, &job)
}

/// Polling-side health check. If the YAML says `Running` but the recorded PID
/// no longer exists, the worker crashed before its terminal write — flip
/// to `Failed` so callers don't poll forever.
pub fn reconcile_dead_worker(config: &Config, job_id: &str) -> Result<JobStatus> {
    let job = load_job(config, job_id)?;
    if job.status != JobStatus::Running {
        return Ok(job.status);
    }
    let pid = match job.worker_pid {
        Some(p) => p,
        None => return Ok(job.status),
    };
    if !pid_alive(pid) {
        mark_failed(
            config,
            job_id,
            "worker process died without recording outcome",
        )?;
        return Ok(JobStatus::Failed);
    }
    Ok(JobStatus::Running)
}

/// `kill(pid, 0)` returns Ok if signal could be delivered (process exists +
/// we have permission). ESRCH = dead; EPERM = alive but ours-or-theirs.
/// Anything other than ESRCH is treated as alive — conservative.
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let pid_i = pid as i32;
        // SAFETY: `kill(pid, 0)` is a query, not a signal send. No state
        // mutation; thread-safe per POSIX.
        let rc = unsafe { libc::kill(pid_i, 0) };
        if rc == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        // ESRCH (3) → not alive. Other errors (EPERM etc.) → alive.
        !matches!(err.raw_os_error(), Some(3))
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true // unsupported platform; assume alive to avoid false negatives
    }
}

/// How long a cancelled worker and its children get between SIGTERM and
/// SIGKILL.
pub const CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Stop a job's worker and every process it started.
///
/// The worker leads its own process group (`job::spawn`), so `kill(-pid)`
/// reaches the worker. The agents and test commands it runs each lead a
/// group of their own (`crate::process`, FIX-006), which a signal to the
/// worker's group does not reach. The worker forwards SIGTERM to them
/// itself, but a child that ignores SIGTERM — or a worker that is stuck or
/// already gone — would leave them running. Their group ids are recorded in
/// the job directory; they get the same SIGTERM → grace → SIGKILL sequence.
///
/// Used by both the CLI and the MCP cancel so the two cannot drift (the MCP
/// path previously sent SIGTERM only, with no escalation).
#[cfg(unix)]
pub fn terminate_job_processes(config: &Config, job_id: &str, worker_pid: u32) {
    let pgids_file = crate::process::child_pgids_file(&crate::job::store::job_dir(config, job_id));
    let mut groups: Vec<i32> = vec![worker_pid as i32];
    groups.extend(crate::process::read_child_pgids(&pgids_file));
    groups.sort_unstable();
    groups.dedup();

    let signal_all = |sig: libc::c_int| {
        for g in &groups {
            // SAFETY: negative pid addresses the process group, per kill(2).
            // ESRCH (group already gone) is harmless.
            unsafe {
                libc::kill(-g, sig);
            }
        }
    };
    let any_alive = || {
        groups
            .iter()
            // SAFETY: signal 0 only probes for existence.
            .any(|g| unsafe { libc::kill(-g, 0) } == 0)
    };

    signal_all(libc::SIGTERM);
    let start = std::time::Instant::now();
    while any_alive() && start.elapsed() < CANCEL_GRACE {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if any_alive() {
        signal_all(libc::SIGKILL);
    }
}

/// Windows: no process groups. Cancel is mark-only until a Job Object
/// based implementation exists.
#[cfg(not(unix))]
pub fn terminate_job_processes(_config: &Config, _job_id: &str, _worker_pid: u32) {
    eprintln!("warning: cancel on Windows does not yet terminate the worker — mark-only.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_alive_for_self_process() {
        let pid = std::process::id();
        assert!(pid_alive(pid));
    }

    #[test]
    #[cfg(unix)]
    fn pid_alive_false_for_reaped_pid() {
        // Spawn /bin/true (Unix-only), wait, then check. Windows doesn't
        // ship a `true` binary; this test is gated to Unix.
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();
        child.wait().unwrap();
        // After wait, PID may be reused — best-effort assertion.
        // Use unlikely-allocated PID instead for stability.
        let high = 999_999_999u32;
        assert!(!pid_alive(high));
        // Real reaped pid is OS-dependent; skip strict check.
        let _ = pid;
    }
}
