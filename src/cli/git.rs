//! `zforge git` — thin git primitives for the auto-approve branch workflow.
//!
//! Shells out to the user's `git` binary so commits use their real config
//! (author + signing). Three subcommands:
//!
//! - `check-clean`  exits 0 if the working tree is clean, 1 with a list of
//!   modified paths otherwise. Pre-flight gate.
//! - `init-branch`  checks out `zforge/<TASK-ID>` (creates from HEAD if it
//!   does not exist).
//! - `commit-phase`  stages the task's `.zforge/tasks/<ID>/` artifacts plus
//!   any project source changes and commits with a phase message.
//!
//! The slash command (`templates/commands/zforge.md`) calls these via the
//! `Bash` tool when the user passes `--auto-approve`. Keeping the heavy
//! lifting (merge / cleanup / PR) in the agent's hands avoids a parallel
//! CLI surface that would duplicate plain `git`.

use crate::config;
use crate::fs::reader::MarkdownFile;
use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Subcommand, Debug)]
pub enum GitCmd {
    /// Exit 0 if the working tree is clean, 1 otherwise. Prints modified
    /// paths to stderr on failure.
    CheckClean,
    /// Checkout `zforge/<TASK-ID>` (creating from HEAD if missing).
    /// Refuses if the working tree is dirty.
    InitBranch { task_id: String },
    /// Stage task artifacts + project source and commit with a phase message.
    /// Phase is one of: spec, testspec, plan, code, review.
    CommitPhase { task_id: String, phase: String },
}

pub fn run(cmd: GitCmd) -> Result<()> {
    let project_root = resolve_project_root()?;
    match cmd {
        GitCmd::CheckClean => check_clean(&project_root),
        GitCmd::InitBranch { task_id } => init_branch(&project_root, &task_id),
        GitCmd::CommitPhase { task_id, phase } => commit_phase(&project_root, &task_id, &phase),
    }
}

fn resolve_project_root() -> Result<PathBuf> {
    let config = config::load().map_err(|_| anyhow!("config not found — run: zforge init"))?;
    Ok(config.project_root())
}

fn run_git(project_root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run `git {}`", args.join(" ")))
}

fn git_ok(project_root: &Path, args: &[&str]) -> Result<String> {
    let out = run_git(project_root, args)?;
    if !out.status.success() {
        bail!(
            "git {} failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn is_git_repo(project_root: &Path) -> bool {
    run_git(project_root, &["rev-parse", "--is-inside-work-tree"])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_git_repo(project_root: &Path) -> Result<()> {
    if !is_git_repo(project_root) {
        bail!("not a git repository: {}", project_root.display());
    }
    Ok(())
}

fn working_tree_status(project_root: &Path) -> Result<String> {
    git_ok(project_root, &["status", "--porcelain"])
}

fn check_clean(project_root: &Path) -> Result<()> {
    ensure_git_repo(project_root)?;
    let status = working_tree_status(project_root)?;
    if status.is_empty() {
        println!("clean");
        return Ok(());
    }
    eprintln!("working tree not clean:\n{status}");
    std::process::exit(1);
}

fn branch_exists(project_root: &Path, branch: &str) -> bool {
    run_git(
        project_root,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )
    .map(|o| o.status.success())
    .unwrap_or(false)
}

fn current_branch(project_root: &Path) -> Result<String> {
    git_ok(project_root, &["rev-parse", "--abbrev-ref", "HEAD"])
}

fn branch_name(task_id: &str) -> String {
    format!("zforge/{task_id}")
}

fn init_branch(project_root: &Path, task_id: &str) -> Result<()> {
    ensure_git_repo(project_root)?;
    validate_task_id(task_id)?;

    let status = working_tree_status(project_root)?;
    if !status.is_empty() {
        bail!(
            "refuse to switch branch: working tree not clean\n{status}\nResolve with `git stash` or commit, then retry."
        );
    }

    let branch = branch_name(task_id);
    let current = current_branch(project_root)?;
    if current == branch {
        println!("already on {branch}");
        return Ok(());
    }

    if branch_exists(project_root, &branch) {
        git_ok(project_root, &["checkout", &branch])?;
        println!("switched to existing {branch}");
    } else {
        git_ok(project_root, &["checkout", "-b", &branch])?;
        println!("created and switched to {branch}");
    }
    Ok(())
}

fn validate_task_id(task_id: &str) -> Result<()> {
    if task_id.is_empty() {
        bail!("task_id must not be empty");
    }
    if task_id
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '\\' || c == ':')
    {
        bail!("task_id contains invalid characters: {task_id}");
    }
    Ok(())
}

fn valid_phase(phase: &str) -> bool {
    matches!(phase, "spec" | "testspec" | "plan" | "code" | "review")
}

fn task_title(project_root: &Path, task_id: &str) -> Result<String> {
    let task_md = project_root
        .join(".zforge")
        .join("tasks")
        .join(task_id)
        .join("task.md");
    let md = MarkdownFile::read(&task_md)
        .with_context(|| format!("failed to read {}", task_md.display()))?;
    Ok(md.get_str("title").unwrap_or("untitled").trim().to_string())
}

fn commit_phase(project_root: &Path, task_id: &str, phase: &str) -> Result<()> {
    ensure_git_repo(project_root)?;
    validate_task_id(task_id)?;
    if !valid_phase(phase) {
        bail!("phase must be one of: spec, testspec, plan, code, review (got {phase})");
    }

    let title = task_title(project_root, task_id)?;

    // Stage everything inside this task's artifact dir, plus any project
    // source changes the agent just made. `git add -A` keeps it simple and
    // matches how most contributors actually commit.
    git_ok(project_root, &["add", "-A"])?;

    // Bail out if nothing changed — `git commit` would error otherwise and
    // the caller (slash command) treats success as "phase committed".
    let staged = git_ok(project_root, &["diff", "--cached", "--name-only"])?;
    if staged.is_empty() {
        println!("nothing to commit for {phase} (working tree unchanged)");
        return Ok(());
    }

    let msg = format!("zforge({phase}): {task_id} {title}");
    git_ok(project_root, &["commit", "-m", &msg])?;
    println!("committed: {msg}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_repo(dir: &Path) {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["init", "--quiet", "--initial-branch=main"])
            .status()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "user.email", "test@example.com"])
            .status()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "user.name", "test"])
            .status()
            .unwrap();
        // need at least one commit so HEAD resolves
        fs::write(dir.join("README.md"), "init\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["add", "."])
            .status()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["commit", "--quiet", "-m", "init"])
            .status()
            .unwrap();
    }

    fn write_task(root: &Path, id: &str, title: &str) {
        let dir = root.join(".zforge").join("tasks").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("task.md"),
            format!("---\ntitle: {title}\n---\n\nbody\n"),
        )
        .unwrap();
    }

    #[test]
    fn validate_task_id_rejects_bad_chars() {
        assert!(validate_task_id("TASK-001").is_ok());
        assert!(validate_task_id("PROJ-42").is_ok());
        assert!(validate_task_id("").is_err());
        assert!(validate_task_id("a b").is_err());
        assert!(validate_task_id("a/b").is_err());
        assert!(validate_task_id("a:b").is_err());
    }

    #[test]
    fn valid_phase_lists_pipeline_phases() {
        for p in &["spec", "testspec", "plan", "code", "review"] {
            assert!(valid_phase(p), "{p} should be valid");
        }
        assert!(!valid_phase("ship"));
        assert!(!valid_phase("verify"));
    }

    #[test]
    fn branch_name_uses_zforge_prefix() {
        assert_eq!(branch_name("TASK-001"), "zforge/TASK-001");
    }

    #[test]
    fn init_branch_creates_then_reuses() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        // first call creates
        init_branch(root, "TASK-001").unwrap();
        assert_eq!(current_branch(root).unwrap(), "zforge/TASK-001");

        // back to main, second call reuses
        git_ok(root, &["checkout", "main"]).unwrap();
        init_branch(root, "TASK-001").unwrap();
        assert_eq!(current_branch(root).unwrap(), "zforge/TASK-001");

        // same branch already current → no-op
        init_branch(root, "TASK-001").unwrap();
    }

    #[test]
    fn init_branch_refuses_dirty_tree() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);
        fs::write(root.join("dirty.txt"), "x").unwrap();
        let err = init_branch(root, "TASK-001").unwrap_err();
        assert!(err.to_string().contains("not clean"));
    }

    fn stage_and_commit_all(root: &Path, msg: &str) {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["add", "."])
            .status()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["commit", "--quiet", "-m", msg])
            .status()
            .unwrap();
    }

    #[test]
    fn commit_phase_writes_expected_message() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);
        write_task(root, "TASK-001", "wire login");
        stage_and_commit_all(root, "import task");
        init_branch(root, "TASK-001").unwrap();

        fs::write(
            root.join(".zforge")
                .join("tasks")
                .join("TASK-001")
                .join("spec.md"),
            "spec body\n",
        )
        .unwrap();

        commit_phase(root, "TASK-001", "spec").unwrap();
        let log = git_ok(root, &["log", "-1", "--format=%s"]).unwrap();
        assert_eq!(log, "zforge(spec): TASK-001 wire login");
    }

    #[test]
    fn commit_phase_no_changes_is_noop() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);
        write_task(root, "TASK-001", "wire login");
        stage_and_commit_all(root, "import task");
        init_branch(root, "TASK-001").unwrap();

        let before = git_ok(root, &["rev-parse", "HEAD"]).unwrap();
        commit_phase(root, "TASK-001", "testspec").unwrap();
        let after = git_ok(root, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn commit_phase_rejects_bad_phase() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);
        write_task(root, "TASK-001", "t");
        let err = commit_phase(root, "TASK-001", "ship").unwrap_err();
        assert!(err.to_string().contains("phase must be one of"));
    }
}
