//! Git as runs use it.
//!
//! Every call strips the caller's repository-binding variables (as the
//! evidence fingerprint does), so a git hook's `GIT_DIR` cannot point a run
//! at another repository. Commits zforge makes on a run's branch — the
//! output it seals, the work it saves before removing a worktree — carry a
//! fixed identity and otherwise follow the repository's own configuration,
//! hooks included. A hook that rejects such a commit fails the step; one
//! that rewrites files is caught by whoever checks the result (`output`
//! compares the sealed tree with the tested candidate).

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Output};

/// Run `git args` in `dir`.
pub fn run(dir: &Path, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir);
    for var in crate::evidence::candidate::REPO_LOCAL_GIT_ENV {
        cmd.env_remove(var);
    }
    cmd.output().context("git is not available")
}

/// Run `git args` in `dir`; its trimmed stdout, or an error with stderr.
pub fn ok(dir: &Path, args: &[&str]) -> Result<String> {
    let out = run(dir, args)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        // A rejecting hook may report on either stream.
        let why = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        bail!("`git {}` failed: {why}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether `commit` is in local branch `branch` now (`git merge-base
/// --is-ancestor`). A squash or rebase merge copies the content, not the
/// commit, so it does not count; neither does anything git cannot answer.
pub fn in_branch(dir: &Path, commit: &str, branch: &str) -> bool {
    run(
        dir,
        &[
            "merge-base",
            "--is-ancestor",
            commit,
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok_and(|o| o.status.success())
}

pub fn head(dir: &Path) -> Result<String> {
    ok(dir, &["rev-parse", "HEAD"])
}

/// No staged, unstaged or untracked (non-ignored) changes.
pub fn is_clean(dir: &Path) -> Result<bool> {
    Ok(ok(dir, &["status", "--porcelain"])?.is_empty())
}

/// Commit everything uncommitted in `dir` as zforge. The new commit, or
/// `None` when there was nothing to commit.
pub fn commit_all(dir: &Path, message: &str) -> Result<Option<String>> {
    if is_clean(dir)? {
        return Ok(None);
    }
    ok(dir, &["add", "-A"])?;
    ok(
        dir,
        &[
            "-c",
            "user.name=zforge",
            "-c",
            "user.email=zforge@localhost",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    )?;
    head(dir).map(Some)
}
