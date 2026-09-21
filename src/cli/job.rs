//! `zforge job` CLI surface — list / status / log / wait / cancel.

use crate::config;
use crate::job::{
    lifecycle::reconcile_dead_worker,
    schema::{Job, JobStatus},
    store::{job_log_path, list_jobs, load_job},
};
use crate::note;
use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

#[derive(Debug, Subcommand)]
pub enum JobCmd {
    /// List every job in the current project's `.zforge/jobs/`.
    List(ListArgs),
    /// Print a single job's metadata. Detects dead workers and updates state.
    Status(StatusArgs),
    /// Print (or follow) the combined stdout+stderr log of a job.
    Log(LogArgs),
    /// Block until the job reaches a terminal state. Exit code 0 on success,
    /// non-zero otherwise.
    Wait(WaitArgs),
    /// SIGTERM the worker; falls back to SIGKILL after 5s. Marks status
    /// `cancelled` regardless.
    Cancel(CancelArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    pub job_id: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct LogArgs {
    pub job_id: String,
    #[arg(long)]
    pub follow: bool,
    #[arg(long)]
    pub tail: Option<usize>,
}

#[derive(Debug, Args)]
pub struct WaitArgs {
    pub job_id: String,
    /// Maximum time to wait in seconds. Default 600.
    #[arg(long, default_value = "600")]
    pub timeout: u64,
    /// Poll interval in milliseconds. Default 500.
    #[arg(long, default_value = "500")]
    pub poll_ms: u64,
}

#[derive(Debug, Args)]
pub struct CancelArgs {
    pub job_id: String,
}

pub fn run(cmd: JobCmd) -> Result<()> {
    match cmd {
        JobCmd::List(a) => list(a),
        JobCmd::Status(a) => status(a),
        JobCmd::Log(a) => log(a),
        JobCmd::Wait(a) => wait(a),
        JobCmd::Cancel(a) => cancel(a),
    }
}

fn list(a: ListArgs) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let jobs = list_jobs(&config)?;
    if a.json {
        note!("{}", serde_json::to_string_pretty(&jobs)?);
        return Ok(());
    }
    if jobs.is_empty() {
        note!("No jobs.");
        return Ok(());
    }
    note!(
        "{:<28} {:<10} {:<10} {:<10} CREATED",
        "JOB",
        "TASK",
        "KIND",
        "STATUS"
    );
    for j in jobs {
        let status_col = colorize_status(&j.status);
        note!(
            "{:<28} {:<10} {:<10} {:<10} {}",
            j.job_id,
            j.task_id,
            j.kind.as_str(),
            status_col,
            j.created_at.format("%Y-%m-%d %H:%M:%S"),
        );
    }
    Ok(())
}

fn colorize_status(s: &JobStatus) -> String {
    match s {
        JobStatus::Running | JobStatus::Queued => s.as_str().yellow().to_string(),
        JobStatus::Success => s.as_str().green().to_string(),
        JobStatus::Failed | JobStatus::Cancelled | JobStatus::Timeout => {
            s.as_str().red().to_string()
        }
    }
}

fn status(a: StatusArgs) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    reconcile_dead_worker(&config, &a.job_id)?;
    let job = load_job(&config, &a.job_id)?;
    if a.json {
        note!("{}", serde_json::to_string_pretty(&job)?);
        return Ok(());
    }
    note!("{}", render_status_block(&job));
    Ok(())
}

fn render_status_block(j: &Job) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let sep = "━".repeat(40);
    let _ = writeln!(out, "{}", sep);
    let _ = writeln!(out, "  Job:    {}", j.job_id);
    let _ = writeln!(out, "{}", sep);
    let _ = writeln!(out, "  Task:    {}", j.task_id);
    let _ = writeln!(out, "  Kind:    {}", j.kind.as_str());
    let _ = writeln!(out, "  Status:  {}", colorize_status(&j.status));
    let _ = writeln!(
        out,
        "  Created: {}",
        j.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(t) = j.started_at {
        let _ = writeln!(out, "  Started: {}", t.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(t) = j.finished_at {
        let _ = writeln!(out, "  Done:    {}", t.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(pid) = j.worker_pid {
        let _ = writeln!(out, "  PID:     {}", pid);
    }
    if let Some(c) = j.exit_code {
        let _ = writeln!(out, "  Exit:    {}", c);
    }
    if let Some(e) = &j.error {
        let _ = writeln!(out, "  Error:   {}", e);
    }
    let _ = writeln!(out, "{}", sep);
    out
}

fn log(a: LogArgs) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let path = job_log_path(&config, &a.job_id);
    if !path.exists() {
        return Err(anyhow!("no log for job {}", a.job_id));
    }

    if let Some(n) = a.tail {
        let content = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(n);
        for line in &lines[start..] {
            note!("{line}");
        }
        if !a.follow {
            return Ok(());
        }
    } else if !a.follow {
        let content = std::fs::read_to_string(&path)?;
        print!("{content}");
        return Ok(());
    }

    // Follow mode — poll file size every 200ms, print delta, stop when job
    // terminal.
    let mut file = std::fs::File::open(&path)?;
    let mut pos: u64 = file.seek(SeekFrom::End(0))?;
    loop {
        // Pump any new bytes.
        file.seek(SeekFrom::Start(pos))?;
        let mut buf = Vec::new();
        let n = file.read_to_end(&mut buf)?;
        if n > 0 {
            print!("{}", String::from_utf8_lossy(&buf));
            pos += n as u64;
        }
        // Stop following once the job is done AND we've drained.
        reconcile_dead_worker(&config, &a.job_id).ok();
        let job = load_job(&config, &a.job_id)?;
        if job.status.is_terminal() && n == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

fn wait(a: WaitArgs) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let deadline = Instant::now() + Duration::from_secs(a.timeout);
    let poll = Duration::from_millis(a.poll_ms);
    loop {
        reconcile_dead_worker(&config, &a.job_id)?;
        let job = load_job(&config, &a.job_id)?;
        if job.status.is_terminal() {
            note!("{}", job.status.as_str());
            if matches!(job.status, JobStatus::Success) {
                return Ok(());
            }
            if let Some(e) = job.error {
                eprintln!("{e}");
            }
            std::process::exit(match job.status {
                JobStatus::Success => 0,
                JobStatus::Failed => 1,
                JobStatus::Cancelled => 2,
                JobStatus::Timeout => 3,
                _ => 4,
            });
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "wait timeout after {}s; job {} still {}",
                a.timeout,
                a.job_id,
                job.status.as_str()
            ));
        }
        std::thread::sleep(poll);
    }
}

fn cancel(a: CancelArgs) -> Result<()> {
    use crate::job::lifecycle::{cancel_job, CancelOutcome};
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    match cancel_job(&config, &a.job_id)? {
        CancelOutcome::AlreadyTerminal(status) => {
            note!("job {} already {}", a.job_id, status.as_str())
        }
        CancelOutcome::Terminated => note!("cancelled {}", a.job_id),
        CancelOutcome::NoWorker => note!("cancelled {} (no worker was launched)", a.job_id),
    }
    Ok(())
}
