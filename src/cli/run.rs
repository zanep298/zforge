//! `zforge run` — execute and operate v1.5 Mốc B runs (MOC-B TASK-004/005).
//!
//! `zforge run <HANDOVER> --task <TASK> [--async]` executes one handed-over
//! leaf task in its own worktree: in the foreground by default (Ctrl-C
//! records the run as `cancelled` and exits 130), or in the background.
//! `zforge run <HANDOVER> --integration` checks the handover's verified
//! outputs together (MOC-C TASK-003), without an agent. `zforge run
//! <HANDOVER>` does all of it: every task in order, then the integration,
//! continuing where an earlier invocation stopped (TASK-005). `status`,
//! `cancel`, `log` and `wait` take a handover as well as a run.
//! `status`, `list`, `cancel`, `retry` and `clean` operate existing runs.

use crate::config;
use crate::run::record::{Run, RunState, RunStatus};
use crate::run::{execute, feature_ops, is_handover, ops, view};
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
    #[arg(long, conflicts_with = "integration")]
    pub task: Option<String>,
    /// Check every task's verified output together, with the integration
    /// check of the handed-over breakdown.
    #[arg(long)]
    pub integration: bool,
    /// Run in the background; follow with `zforge run status <RUN>`.
    #[arg(long = "async")]
    pub background: bool,
}

#[derive(Debug, Subcommand)]
pub enum RunCmd {
    /// State, events, verifications, agent calls and cost of a run; or,
    /// given a handover, where each of its tasks and its integration stand.
    Status {
        /// `RUN-001`, or `HANDOVER-001` / `<INTAKE>/HANDOVER-001`.
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
    /// Print (or follow) a background run's log.
    Log {
        run: String,
        /// Keep printing until the run ends.
        #[arg(long)]
        follow: bool,
        /// Start from the last N lines.
        #[arg(long)]
        tail: Option<usize>,
    },
    /// Block until the run ends. Exit 0 only when it is verified.
    Wait {
        run: String,
        /// Give up after this many seconds (default 3600).
        #[arg(long, default_value = "3600")]
        timeout: u64,
        /// Poll interval in milliseconds.
        #[arg(long = "poll-ms", default_value = "500")]
        poll_ms: u64,
    },
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
        Some(RunCmd::Cancel { run }) if is_handover(&run) => {
            let f = feature_ops::cancel(&root, &run)?;
            print!("{}", view::feature(&f));
            Ok(())
        }
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
        Some(RunCmd::Log { run, follow, tail }) => log(&root, &run, follow, tail),
        Some(RunCmd::Wait {
            run,
            timeout,
            poll_ms,
        }) if is_handover(&run) => wait_feature(&root, &run, timeout, poll_ms),
        Some(RunCmd::Wait {
            run,
            timeout,
            poll_ms,
        }) => wait(&root, &run, timeout, poll_ms),
        None => {
            const USAGE: &str = "usage: zforge run <HANDOVER> [--task <TASK> | --integration] [--async], or a subcommand (see --help)";
            let Some(handover) = args.handover else {
                bail!(USAGE);
            };
            let r = match (args.task, args.integration) {
                (Some(task), false) => execute::create(&root, &handover, &task, None)?,
                (None, true) => crate::run::integrate::create(&root, &handover, None)?,
                (None, false) => return run_feature(&root, &handover, args.background),
                _ => bail!(USAGE),
            };
            start(&root, &r, args.background)
        }
    }
}

/// Execute `run` in the foreground, or hand it to a background worker.
fn start(root: &Path, run: &Run, background: bool) -> Result<()> {
    let meta = run.meta()?;
    match &meta.checks {
        Some(checks) => eprintln!(
            "{} {} — integration of {}: {} check(s) from the {}, no agent",
            "▶".cyan(),
            run.id,
            meta.handover,
            checks.commands.len(),
            match checks.from {
                crate::run::record::ChecksFrom::Breakdown => "breakdown",
                crate::run::record::ChecksFrom::Config => "project's test command",
            }
        ),
        None => eprintln!(
            "{} {} — {} of {}, budget ${:.2}, at most {} verification(s)",
            "▶".cyan(),
            run.id,
            meta.task,
            meta.handover,
            meta.budget_usd,
            meta.max_iterations
        ),
    }
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

/// Every task of a handover, then its integration check.
fn run_feature(root: &Path, handover: &str, background: bool) -> Result<()> {
    if background {
        let (qualified, pid) = feature_ops::spawn_async(root, handover)?;
        println!(
            "{} {qualified} running in the background (worker {pid}); follow with `zforge run log {qualified} --follow`",
            "✓".green()
        );
        return Ok(());
    }
    crate::process::catch_interrupts();
    let f = match feature_ops::run_feature(root, handover) {
        Ok(f) => f,
        Err(e) if e.downcast_ref::<feature_ops::Interrupted>().is_some() => {
            eprintln!(
                "{} {handover} interrupted; run it again to continue",
                "✗".red()
            );
            std::process::exit(130)
        }
        Err(e) => return Err(e),
    };
    print!("{}", view::feature(&f));
    feature_verdict(&f)
}

fn feature_verdict(f: &crate::run::feature::FeatureState) -> Result<()> {
    match f.is_verified() {
        true => Ok(()),
        false => Err(anyhow!("{} is not verified", f.handover)),
    }
}

/// Body of the hidden `zforge run-worker --handover <H>`.
pub fn feature_worker(handover: &str) -> Result<()> {
    let root = project_root()?;
    let f = feature_ops::worker(&root, handover)?;
    feature_verdict(&f)
}

/// Block until nothing of the handover is running any more; exit 0 only
/// when it is verified.
fn wait_feature(root: &Path, handover: &str, timeout_secs: u64, poll_ms: u64) -> Result<()> {
    let h = crate::run::contract::load_handover(root, handover)?;
    let dir = feature_ops::dir(root, &h.intake, &h.manifest.id);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let f = crate::run::feature::load(root, handover)?;
        let running = f
            .tasks
            .iter()
            .map(|t| &t.progress)
            .chain([&f.integration])
            .any(|p| matches!(p, crate::run::feature::Progress::Running { .. }));
        if !running && feature_ops::loop_pid(&dir).is_none() {
            print!("{}", view::feature(&f));
            return feature_verdict(&f);
        }
        if std::time::Instant::now() >= deadline {
            bail!("{} is still running after {timeout_secs}s", h.manifest.id);
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}

fn describe(state: &RunState) -> String {
    match &state.reason {
        Some(r) => format!("{} — {r}", state.status.as_str()),
        None => state.status.as_str().to_string(),
    }
}

fn status(root: &Path, id: &str, json: bool) -> Result<()> {
    if is_handover(id) {
        let f = crate::run::feature::load(root, id)?;
        match json {
            true => println!("{}", serde_json::to_string_pretty(&f)?),
            false => print!("{}", view::feature(&f)),
        }
        return Ok(());
    }
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
            print!("{}", crate::trace::text::render_phase_text(t));
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

/// Print a background run's log; `--follow` until the run ends.
fn log(root: &Path, id: &str, follow: bool, tail: Option<usize>) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    // What says the writer is done: the run ended, or the handover's loop
    // is gone.
    let (path, done): (std::path::PathBuf, Box<dyn Fn() -> Result<bool>>) = if is_handover(id) {
        let h = crate::run::contract::load_handover(root, id)?;
        let dir = feature_ops::dir(root, &h.intake, &h.manifest.id);
        let path = dir.join(feature_ops::LOG);
        (
            path,
            Box::new(move || Ok(feature_ops::loop_pid(&dir).is_none())),
        )
    } else {
        let run = Run::open(root, id)?;
        let path = run.dir.join(crate::run::ops::LOG);
        (
            path,
            Box::new(move || Ok(ops::refresh(&run)?.status.is_final())),
        )
    };
    if !path.exists() {
        bail!("{id} has no log; only a run started with --async writes one");
    }
    let mut pos = 0;
    if let Some(n) = tail {
        let content = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = content.lines().collect();
        for line in &lines[lines.len().saturating_sub(n)..] {
            println!("{line}");
        }
        pos = content.len() as u64;
    } else {
        let content = std::fs::read_to_string(&path)?;
        print!("{content}");
        pos = pos.max(content.len() as u64);
    }
    if !follow {
        return Ok(());
    }
    let mut file = std::fs::File::open(&path)?;
    loop {
        file.seek(SeekFrom::Start(pos))?;
        let mut buf = Vec::new();
        let n = file.read_to_end(&mut buf)?;
        if n > 0 {
            print!("{}", String::from_utf8_lossy(&buf));
            pos += n as u64;
        }
        if n == 0 && done()? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Block until the run ends; exit non-zero unless it was verified.
fn wait(root: &Path, id: &str, timeout_secs: u64, poll_ms: u64) -> Result<()> {
    let run = Run::open(root, id)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let state = ops::refresh(&run)?;
        if state.status.is_final() {
            println!("{} {}", run.id, describe(&state));
            return match state.status {
                RunStatus::Verified => Ok(()),
                status => Err(anyhow!("{} ended {}", run.id, status.as_str())),
            };
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "{} is still {} after {timeout_secs}s",
                run.id,
                state.status.as_str()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}
