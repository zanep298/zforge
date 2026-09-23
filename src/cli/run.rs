//! `zforge run` — execute and operate v1.5 Mốc B runs (MOC-B TASK-004/005).
//!
//! `zforge run <HANDOVER> --task <TASK> [--async]` executes one handed-over
//! leaf task in its own worktree: in the foreground by default (Ctrl-C
//! records the run as `cancelled` and exits 130), or in the background.
//! `status`, `list`, `cancel`, `retry` and `clean` operate existing runs.

use crate::config;
use crate::run::record::{Run, RunState, RunStatus};
use crate::run::{execute, ops, view};
use anyhow::{anyhow, bail, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::path::Path;

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct RunArgs {
    #[command(subcommand)]
    pub cmd: Option<RunCmd>,
    /// `HANDOVER-001`, or `<INTAKE>/HANDOVER-001` when ambiguous.
    pub handover: Option<String>,
    /// The leaf task to run.
    #[arg(long)]
    pub task: Option<String>,
    /// Run in the background; follow with `zforge run status <RUN>`.
    #[arg(long = "async")]
    pub background: bool,
}

#[derive(Debug, Subcommand)]
pub enum RunCmd {
    /// State, events, verifications, agent calls and cost of a run.
    Status {
        run: String,
        #[arg(long)]
        json: bool,
    },
    /// Every run, oldest first.
    List {
        /// Only runs of this handover.
        #[arg(long)]
        handover: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Stop a run's agent and tests and record it cancelled.
    Cancel { run: String },
    /// Run the same task again as a new run, with what is left of the budget.
    Retry {
        run: String,
        #[arg(long = "async")]
        background: bool,
    },
    /// Remove a finished run's worktree; its branch is kept, with any
    /// uncommitted work committed to it first.
    Clean { run: String },
}

fn project_root() -> Result<std::path::PathBuf> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    Ok(config.project_root())
}

pub fn run(args: RunArgs) -> Result<()> {
    let root = project_root()?;
    match args.cmd {
        Some(RunCmd::Status { run, json }) => status(&root, &run, json),
        Some(RunCmd::List { handover, json }) => list(&root, handover.as_deref(), json),
        Some(RunCmd::Cancel { run }) => {
            let r = Run::open(&root, &run)?;
            let state = ops::cancel(&root, &r)?;
            println!("{} {} {}", "✓".green(), r.id, describe(&state));
            Ok(())
        }
        Some(RunCmd::Retry { run, background }) => {
            let old = Run::open(&root, &run)?;
            let new = ops::retry(&root, &old)?;
            eprintln!("{} {} retries {}", "▶".cyan(), new.id, old.id);
            start(&root, &new, background)
        }
        Some(RunCmd::Clean { run }) => {
            let r = Run::open(&root, &run)?;
            let saved = ops::clean(&root, &r)?;
            let meta = r.meta()?;
            match saved {
                Some(commit) => println!(
                    "{} {} worktree removed; uncommitted work saved to {} as {}",
                    "✓".green(),
                    r.id,
                    meta.branch,
                    &commit[..commit.len().min(12)]
                ),
                None => println!(
                    "{} {} worktree removed; branch {} kept",
                    "✓".green(),
                    r.id,
                    meta.branch
                ),
            }
            Ok(())
        }
        None => {
            let (Some(handover), Some(task)) = (args.handover, args.task) else {
                bail!("usage: zforge run <HANDOVER> --task <TASK> [--async], or a subcommand (see --help)");
            };
            let r = execute::create(&root, &handover, &task, None)?;
            start(&root, &r, args.background)
        }
    }
}

/// Execute `run` in the foreground, or hand it to a background worker.
fn start(root: &Path, run: &Run, background: bool) -> Result<()> {
    let meta = run.meta()?;
    eprintln!(
        "{} {} — {} of {}, budget ${:.2}, at most {} verification(s)",
        "▶".cyan(),
        run.id,
        meta.task,
        meta.handover,
        meta.budget_usd,
        meta.max_iterations
    );
    if background {
        let pid = ops::spawn_async(root, run)?;
        println!(
            "{} {} running in the background (worker {pid}); follow with `zforge run status {}`",
            "✓".green(),
            run.id,
            run.id
        );
        return Ok(());
    }
    std::env::set_var(
        crate::process::CHILD_PGIDS_FILE_ENV,
        crate::process::child_pgids_file(&run.dir),
    );
    crate::process::catch_interrupts();
    let state = execute::execute(root, run)?;
    view::write(root, run, &state)?;
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
        _ => format!(
            "{} {} {} (${:.2} spent; worktree kept at {})",
            "✗".red(),
            run.id,
            describe(&state),
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

fn describe(state: &RunState) -> String {
    match &state.reason {
        Some(r) => format!("{} — {r}", state.status.as_str()),
        None => state.status.as_str().to_string(),
    }
}

fn status(root: &Path, id: &str, json: bool) -> Result<()> {
    let run = Run::open(root, id)?;
    let state = ops::refresh(&run)?;
    let meta = run.meta()?;
    let events = run.events()?;
    let (traces, _) = crate::trace::log::read(&crate::run::runs_dir(root), &run.id);
    if json {
        let v = serde_json::json!({
            "meta": meta,
            "state": state,
            "events": events,
            "traces": traces,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    print!("{}", view::summary(&meta, &state));
    println!("\nevents");
    print!("{}", view::timeline(&events));
    if !traces.is_empty() {
        println!("\nagent calls");
        for t in &traces {
            print!("{}", crate::cli::trace::render_phase_text(t));
        }
    }
    Ok(())
}

fn list(root: &Path, handover: Option<&str>, json: bool) -> Result<()> {
    let runs = ops::list(root, handover)?;
    if json {
        let v: Vec<_> = runs
            .iter()
            .map(|(_, meta, state)| serde_json::json!({ "meta": meta, "state": state }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    if runs.is_empty() {
        println!("no runs yet");
    }
    for (run, meta, state) in &runs {
        println!(
            "{:<8} {:<10} {:<16} {:<10} ${:>5.2}  {}{}",
            run.id,
            meta.task,
            format!("{}/{}", meta.intake, meta.handover),
            state.status.as_str(),
            state.cost_usd,
            meta.created_at.format("%Y-%m-%d %H:%M"),
            state
                .reason
                .as_deref()
                .map(|r| format!("  — {r}"))
                .unwrap_or_default()
        );
    }
    Ok(())
}

/// Body of the hidden `zforge run-worker <RUN>`.
pub fn worker(run_id: &str) -> Result<()> {
    let root = project_root()?;
    let state = ops::worker(&root, run_id)?;
    match state.status {
        RunStatus::Verified => Ok(()),
        _ => Err(anyhow!("{run_id} ended {}", describe(&state))),
    }
}
