//! Detached worker spawn.
//!
//! Re-invokes the running `zforge` binary with the hidden `worker` subcommand.
//! `process_group(0)` puts the child in its own group so it survives the
//! controller's terminal closing; stdout/stderr redirect into the job's log
//! file. Returns the child PID immediately — never `wait`.

use crate::config::Config;
use crate::job::store::job_log_path;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub fn spawn_worker(config: &Config, job_id: &str) -> Result<u32> {
    let log_path = job_log_path(config, job_id);
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("open log {log_path:?}"))?;
    let log_clone = log_file.try_clone().context("dup log fd for stderr")?;

    // `current_exe()` returns the test binary under `cargo test`. Tests set
    // `ZFORGE_WORKER_BIN` to point at the real `zforge` binary built by
    // `CARGO_BIN_EXE_zforge`. Production runs ignore the override.
    let exe = match std::env::var_os("ZFORGE_WORKER_BIN") {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_exe().context("locate zforge binary")?,
    };
    let project_root = config.project_root();

    let mut cmd = Command::new(&exe);
    cmd.arg("worker")
        .arg("--job-id")
        .arg(job_id)
        .current_dir(&project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_clone));

    #[cfg(unix)]
    {
        // Detach from the controller's process group. Survives terminal close
        // and `Ctrl-C` on the controller.
        cmd.process_group(0);
    }

    let child = cmd
        .spawn()
        .with_context(|| format!("spawn worker for job {job_id}"))?;
    Ok(child.id())
}
