//! Worker entry point — called via the hidden `zforge worker --job-id <ID>`
//! subcommand. Loads the job record, dispatches to the appropriate CLI
//! handler, writes terminal status back to disk.
//!
//! Runs in the same process as a regular zforge invocation — no IPC, no
//! daemonization beyond the controller's `process_group(0)` detach. The
//! controller exits as soon as `spawn_worker` returns; the kernel reaps
//! the worker normally on exit.

use crate::cli::outcome::OperationOutcome;
use crate::config;
use crate::job::lifecycle::{mark_failed, mark_running, mark_success};
use crate::job::schema::JobKind;
use crate::job::store::load_job;
use anyhow::{Context, Result};

pub fn run(job_id: &str) -> Result<OperationOutcome> {
    // Signal to the orchestrator that no human is present. `run_phase`
    // appends each agent's bypass flags (e.g. `--dangerously-skip-permissions`
    // for claude) so the spawned LLM doesn't block on interactive prompts.
    // Foreground invocations (`zforge ship T1` without `--async`) leave the
    // env var unset and run with the user's normal permission behavior.
    std::env::set_var("ZFORGE_HEADLESS", "1");

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let job = load_job(&config, job_id).context("load job")?;

    // Agents and test commands run in process groups of their own, which a
    // cancel signalling this worker's group does not reach. Record them in
    // the job directory so `job cancel` can stop them too.
    std::env::set_var(
        crate::process::CHILD_PGIDS_FILE_ENV,
        crate::process::child_pgids_file(&crate::job::store::job_dir(&config, job_id)),
    );

    // Per-task lock prevents two workers from racing on the same `.state.yaml`.
    // Held for the worker's entire lifetime; kernel releases on process death.
    let task_lock = crate::state::lock_task(&config.tasks_dir(), &job.task_id)?;

    mark_running(&config, job_id, std::process::id())?;

    let caught =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(&job, &task_lock)));

    let result: Result<OperationOutcome> = match caught {
        Ok(r) => r,
        Err(panic) => {
            let msg = panic_message(panic);
            Err(anyhow::anyhow!("worker panicked: {msg}"))
        }
    };

    // A job is successful only when the operation it ran was successful.
    // `Failed` (red tests, exhausted verifier budget) and `Blocked` (a gate
    // refused the request) are terminal non-success results, and recording
    // them as success is what let `ship --async` report a green job over a
    // red suite. An `Err` here means the operation could not run at all.
    match &result {
        Ok(outcome) if outcome.is_success() => mark_success(&config, job_id)?,
        Ok(outcome) => mark_failed(
            &config,
            job_id,
            &format!("{}: {}", outcome.label(), outcome.reason().unwrap_or("")),
        )?,
        Err(e) => mark_failed(&config, job_id, &format!("{e:#}"))?,
    }

    result
}

/// The worker owns the task lock for its lifetime and hands the guard to the
/// operation, which threads it through everything nested inside.
fn dispatch(
    job: &crate::job::schema::Job,
    task_lock: &crate::state::TaskLockGuard,
) -> Result<OperationOutcome> {
    match job.kind {
        JobKind::Ship => crate::cli::ship::run_locked(
            task_lock,
            &job.task_id,
            job.command_override.clone(),
            job.timeout_secs,
            job.max_iterations,
        ),
    }
}

fn panic_message(p: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = p.downcast_ref::<String>() {
        return s.clone();
    }
    "non-string panic payload".to_string()
}
