//! Operating runs (MOC-B TASK-005; REQ-007, REQ-008): background runs,
//! status with recovery, cancel, retry and clean — one run, every final
//! run (`clean_all`), and the merged branches of finished runs
//! (`prune_branches`). A verified run's worktree is released as soon as it
//! is verified (`release_verified`).
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
    worktree::prune(project_root);
    Ok(saved)
}

/// After a run is verified: remove its worktree, keeping its branch — the
/// sealed output (`verified.commit`) is what dependent tasks, reuse and the
/// integration start from, so nothing needs the worktree any more. Kept
/// when `execution.keep_worktrees` is set, or when the config cannot be
/// read. Never fails the run: a removal that goes wrong is a warning, and
/// the worktree stays for `zforge run clean`.
pub fn release_verified(project_root: &Path, run: &Run) {
    let keep = match super::execute::load_config(project_root) {
        Ok(c) => c.execution.keep_worktrees,
        Err(e) => {
            eprintln!(
                "warning: {}: worktree kept, the config could not be read: {e:#}",
                run.id
            );
            true
        }
    };
    if keep {
        return;
    }
    let meta = match run.meta() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("warning: {}: worktree kept: {e:#}", run.id);
            return;
        }
    };
    if !meta.worktree.exists() {
        return;
    }
    let wt = worktree::Worktree {
        path: meta.worktree.clone(),
        branch: meta.branch.clone(),
    };
    // The seal left it clean; anything else is not ours to throw away.
    if let Err(e) = worktree::remove(project_root, &wt, false) {
        eprintln!(
            "warning: {}: the verified run's worktree was kept: {e:#}; \
             remove it with `zforge run clean {}`",
            run.id, run.id
        );
    }
    worktree::prune(project_root);
}

/// One worktree removed by [`clean_all`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cleaned {
    pub run: String,
    pub branch: String,
    /// The commit holding work that was uncommitted in the worktree.
    pub saved: Option<String>,
    /// Disk the worktree took.
    pub bytes: u64,
}

/// What [`clean_all`] did.
#[derive(Debug, Default)]
pub struct CleanAll {
    pub cleaned: Vec<Cleaned>,
    /// Runs whose worktree could not be removed, with why.
    pub failed: Vec<(String, String)>,
}

/// [`clean`] every final run that still holds a worktree — only verified
/// ones with `verified_only`. A run still in progress is never touched; one
/// that cannot be cleaned is reported and the rest go on.
pub fn clean_all(project_root: &Path, verified_only: bool) -> Result<CleanAll> {
    let mut out = CleanAll::default();
    for (run, meta, state) in list(project_root, None)? {
        if !state.status.is_final() || !meta.worktree.exists() {
            continue;
        }
        if verified_only && state.status != RunStatus::Verified {
            continue;
        }
        let bytes = disk_usage(&meta.worktree);
        match clean(project_root, &run) {
            Ok(saved) => out.cleaned.push(Cleaned {
                run: run.id.clone(),
                branch: meta.branch.clone(),
                saved,
                bytes,
            }),
            Err(e) => out.failed.push((run.id.clone(), format!("{e:#}"))),
        }
    }
    worktree::prune(project_root);
    Ok(out)
}

/// Bytes under `path`, symlinks not followed. Unreadable entries count 0.
fn disk_usage(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| disk_usage(&e.path()))
                .sum::<u64>()
        })
        .unwrap_or(0)
}

/// A run branch [`prune_branches`] deleted, or would delete.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PrunedBranch {
    pub branch: String,
    pub tip: String,
    /// The runs recorded on it (a reused run shares the branch it reused).
    pub runs: Vec<String>,
}

/// What [`prune_branches`] found.
#[derive(Debug, Default, serde::Serialize)]
pub struct Prune {
    pub baseline: String,
    pub deleted: Vec<PrunedBranch>,
    /// `zforge/*` branches left alone, with why.
    pub kept: Vec<(String, String)>,
}

/// Delete local `zforge/*` branches that are done with: every run recorded
/// on the branch is final and holds no worktree, git has no worktree on it,
/// and its tip is already in `knowledge.baseline`. A branch no run records
/// is left alone — zforge did not make it, as far as it knows. With
/// `dry_run`, only report.
pub fn prune_branches(project_root: &Path, dry_run: bool) -> Result<Prune> {
    use std::collections::{BTreeMap, BTreeSet};
    let config = super::execute::load_config(project_root)?;
    let baseline = config.knowledge.baseline.clone();
    let git_dir = project_root;
    super::git::ok(
        git_dir,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{baseline}"),
        ],
    )
    .with_context(|| format!("baseline branch {baseline} (`knowledge.baseline`) not found"))?;
    worktree::prune(project_root);

    let mut by_branch: BTreeMap<String, Vec<(String, RunState, bool)>> = BTreeMap::new();
    for (run, meta, state) in list(project_root, None)? {
        by_branch.entry(meta.branch.clone()).or_default().push((
            run.id.clone(),
            state,
            meta.worktree.exists(),
        ));
    }
    let checked_out: BTreeSet<String> = worktree::list(project_root)?
        .into_iter()
        .map(|w| w.branch)
        .collect();
    let refs = super::git::ok(
        git_dir,
        &["for-each-ref", "--format=%(refname)", "refs/heads/zforge/"],
    )?;

    let mut out = Prune {
        baseline: baseline.clone(),
        ..Default::default()
    };
    for branch in refs.lines().filter_map(|r| r.strip_prefix("refs/heads/")) {
        let keep = |out: &mut Prune, why: String| out.kept.push((branch.to_string(), why));
        let Some(runs) = by_branch.get(branch) else {
            keep(&mut out, "no run records it".into());
            continue;
        };
        if let Some((id, s, _)) = runs.iter().find(|(_, s, _)| !s.status.is_final()) {
            keep(&mut out, format!("{id} is still {}", s.status.as_str()));
            continue;
        }
        if let Some((id, _, _)) = runs.iter().find(|(_, _, wt)| *wt) {
            keep(&mut out, format!("{id} still has its worktree"));
            continue;
        }
        if checked_out.contains(branch) {
            keep(&mut out, "checked out in a worktree".into());
            continue;
        }
        let tip = super::git::ok(git_dir, &["rev-parse", &format!("refs/heads/{branch}")])?;
        if !super::git::in_branch(git_dir, &tip, &baseline) {
            keep(&mut out, format!("not merged into {baseline}"));
            continue;
        }
        if !dry_run {
            if let Err(e) = super::git::ok(git_dir, &["branch", "-D", "--", branch]) {
                keep(&mut out, format!("{e:#}"));
                continue;
            }
        }
        out.deleted.push(PrunedBranch {
            branch: branch.to_string(),
            tip,
            runs: runs.iter().map(|(id, _, _)| id.clone()).collect(),
        });
    }
    Ok(out)
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
