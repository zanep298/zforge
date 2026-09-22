//! `zforge run <HANDOVER> --task <TASK>` — execute one handed-over leaf task
//! in its own worktree (v1.5 Mốc B, MOC-B TASK-004).
//!
//! Runs in the foreground. Ctrl-C (or SIGTERM/SIGHUP) stops the agent and
//! the tests, records the run as `cancelled` and exits 130; the run is never
//! left looking alive.

use crate::config;
use crate::run::{execute, record::RunStatus};
use anyhow::{anyhow, Result};
use colored::Colorize;

pub fn run(handover: &str, task: &str) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let root = config.project_root();
    crate::process::catch_interrupts();

    let run = execute::create(&root, handover, task, None)?;
    let meta = run.meta()?;
    eprintln!(
        "{} {} — {task} of {}, budget ${:.2}, at most {} verification(s)",
        "▶".cyan(),
        run.id,
        meta.handover,
        meta.budget_usd,
        meta.max_iterations
    );
    let state = execute::execute(&root, &run)?;

    let line = match state.status {
        RunStatus::Verified => format!(
            "{} {} verified — branch {}, candidate {}, {} verification(s), ${:.2}",
            "✓".green(),
            run.id,
            meta.branch,
            state
                .last_candidate
                .as_deref()
                .map(|c| &c[..c.len().min(12)])
                .unwrap_or("?"),
            state.verifications,
            state.cost_usd
        ),
        status => format!(
            "{} {} {} — {} (${:.2} spent; worktree kept at {})",
            "✗".red(),
            run.id,
            status.as_str(),
            state.reason.as_deref().unwrap_or("no reason recorded"),
            state.cost_usd,
            meta.worktree.display()
        ),
    };
    println!("{line}");
    match state.status {
        RunStatus::Verified => Ok(()),
        RunStatus::Cancelled
            if state
                .reason
                .as_deref()
                .is_some_and(|r| r.contains("signal")) =>
        {
            std::process::exit(130)
        }
        status => Err(anyhow!("{} ended {}", run.id, status.as_str())),
    }
}
