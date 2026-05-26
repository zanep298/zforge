//! Worker entry point — called via the hidden `zforge worker --job-id <ID>`
//! subcommand. Loads the job record, dispatches to the appropriate CLI
//! handler, writes terminal status back to disk.
//!
//! Runs in the same process as a regular zforge invocation — no IPC, no
//! daemonization beyond the controller's `process_group(0)` detach. The
//! controller exits as soon as `spawn_worker` returns; the kernel reaps
//! the worker normally on exit.

use crate::config;
use crate::job::lifecycle::{mark_failed, mark_running, mark_success};
use crate::job::schema::JobKind;
use crate::job::store::load_job;
use anyhow::{Context, Result};

pub fn run(job_id: &str) -> Result<()> {
    // Signal to the orchestrator that no human is present. `run_phase`
    // appends each agent's bypass flags (e.g. `--dangerously-skip-permissions`
    // for claude) so the spawned LLM doesn't block on interactive prompts.
    // Foreground invocations (`zforge ship T1` without `--async`) leave the
    // env var unset and run with the user's normal permission behavior.
    std::env::set_var("ZFORGE_HEADLESS", "1");

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let job = load_job(&config, job_id).context("load job")?;

    // Per-task lock prevents two workers from racing on the same `.state.yaml`.
    // Held for the worker's entire lifetime; kernel releases on process death.
    let _task_lock = crate::state::try_acquire_task_lock(&config.tasks_dir(), &job.task_id)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    mark_running(&config, job_id, std::process::id())?;

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(&job)));

    let result: Result<()> = match outcome {
        Ok(r) => r,
        Err(panic) => {
            let msg = panic_message(panic);
            Err(anyhow::anyhow!("worker panicked: {msg}"))
        }
    };

    match &result {
        Ok(()) => mark_success(&config, job_id)?,
        Err(e) => mark_failed(&config, job_id, &format!("{e:#}"))?,
    }
    result
}

fn dispatch(job: &crate::job::schema::Job) -> Result<()> {
    match job.kind {
        JobKind::Ship => crate::cli::ship::run(
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
