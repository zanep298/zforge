//! The git worktree of a run (MOC-B TASK-002, REQ-002).
//!
//! Each run works in `.zforge/worktrees/<RUN>` on its own branch
//! `zforge/<task>/<run>`, created from the commit it starts at — the
//! handover's baseline, or its dependencies' output (`start`). The
//! user's branch and working tree are never touched. A failed run keeps its
//! worktree for inspection; removing a worktree keeps its branch, so the
//! work stays reachable until the user deletes it.
//!
//! Git goes through [`super::git`], which strips the caller's
//! repository-binding variables.

use super::git::{ok as git_ok, run as git};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Worktrees live here, relative to the project root.
pub const WORKTREES_DIR: &str = ".zforge/worktrees";
/// The `.gitignore` line that keeps them out of the project's history.
pub const IGNORE_LINE: &str = ".zforge/worktrees/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
}

/// `zforge/<task>/<run>`.
pub fn branch_name(task: &str, run: &str) -> String {
    format!("zforge/{task}/{run}")
}

pub fn path_for(project_root: &Path, run: &str) -> PathBuf {
    project_root.join(WORKTREES_DIR).join(run)
}

fn branch_exists(root: &Path, branch: &str) -> Result<bool> {
    Ok(git(
        root,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ],
    )?
    .status
    .success())
}

/// Create the run's worktree at `commit`. Checks first, so a refusal leaves
/// nothing behind; if git itself fails half-way, whatever it created is
/// removed again.
pub fn create(project_root: &Path, run: &str, task: &str, commit: &str) -> Result<Worktree> {
    crate::intake::validate_id(task)?;
    create_on(project_root, run, &branch_name(task, run), commit)
}

/// [`create`] on a branch of the caller's choosing — an integration run's
/// is `zforge/<INTAKE>/integration/<RUN>`.
pub fn create_on(project_root: &Path, run: &str, branch: &str, commit: &str) -> Result<Worktree> {
    crate::intake::validate_id(run)?;
    let inside = git(project_root, &["rev-parse", "--is-inside-work-tree"])?;
    if !inside.status.success() || String::from_utf8_lossy(&inside.stdout).trim() != "true" {
        bail!(
            "{} is not in a git work tree; runs need one",
            project_root.display()
        );
    }
    let commit_id = git_ok(
        project_root,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("{commit}^{{commit}}"),
        ],
    )
    .with_context(|| format!("start commit {commit} not found"))?;
    let branch = branch.to_string();
    if branch_exists(project_root, &branch)? {
        bail!("branch {branch} already exists; a run never reuses a branch");
    }
    let path = path_for(project_root, run);
    if path.exists() {
        bail!(
            "{} already exists; a run never reuses a worktree",
            path.display()
        );
    }

    ensure_ignored(project_root)?;
    std::fs::create_dir_all(project_root.join(WORKTREES_DIR))?;
    let path_arg = path.to_string_lossy().into_owned();
    let out = git(
        project_root,
        &[
            "worktree", "add", "--quiet", "-b", &branch, &path_arg, &commit_id,
        ],
    )?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        undo_partial(project_root, &path, &branch);
        bail!("could not create the worktree for {run}: {why}");
    }
    Ok(Worktree { path, branch })
}

/// Best-effort cleanup after `git worktree add` failed part-way.
fn undo_partial(root: &Path, path: &Path, branch: &str) {
    let _ = std::fs::remove_dir_all(path);
    let _ = git(root, &["worktree", "prune"]);
    if branch_exists(root, branch).unwrap_or(false) {
        let _ = git(root, &["branch", "-D", branch]);
    }
}

/// Remove the worktree, keeping its branch. Refuses if the worktree has
/// uncommitted changes unless `force` — they would be lost.
pub fn remove(project_root: &Path, worktree: &Worktree, force: bool) -> Result<()> {
    let path_arg = worktree.path.to_string_lossy().into_owned();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path_arg);
    let out = git(project_root, &args)?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if why.contains("modified or untracked files") {
            bail!(
                "{} has uncommitted changes; commit them to {} or remove with force",
                worktree.path.display(),
                worktree.branch
            );
        }
        bail!("could not remove {}: {why}", worktree.path.display());
    }
    Ok(())
}

/// Run worktrees git knows about, with their branches.
pub fn list(project_root: &Path) -> Result<Vec<Worktree>> {
    let base = project_root
        .join(WORKTREES_DIR)
        .canonicalize()
        .unwrap_or_else(|_| project_root.join(WORKTREES_DIR));
    let porcelain = git_ok(project_root, &["worktree", "list", "--porcelain"])?;
    let mut out = Vec::new();
    for block in porcelain.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            }
        }
        if let (Some(path), Some(branch)) = (path, branch) {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if canonical.starts_with(&base) {
                out.push(Worktree {
                    path: canonical,
                    branch,
                });
            }
        }
    }
    Ok(out)
}

/// Add [`IGNORE_LINE`] to the project's `.gitignore` once, creating the
/// file if needed and leaving the rest of it as it was — unless git already
/// ignores the worktrees (e.g. `.zforge/` is ignored), in which case the
/// user's checkout is left exactly as it was.
pub fn ensure_ignored(project_root: &Path) -> Result<()> {
    let probe = format!("{WORKTREES_DIR}/RUN-000/x");
    if git(project_root, &["check-ignore", "-q", &probe])?
        .status
        .success()
    {
        return Ok(());
    }
    let path = project_root.join(".gitignore");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    if text.lines().any(|l| l.trim() == IGNORE_LINE) {
        return Ok(());
    }
    let mut updated = text;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(IGNORE_LINE);
    updated.push('\n');
    crate::fs::write_atomic(&path, updated.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Repo {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            for args in [
                &["init", "-q", "-b", "main", "."][..],
                &["config", "user.email", "t@t"],
                &["config", "user.name", "t"],
            ] {
                git_ok(&root, args).unwrap();
            }
            std::fs::write(root.join("lib.rs"), "fn a() {}\n").unwrap();
            git_ok(&root, &["add", "-A"]).unwrap();
            git_ok(&root, &["commit", "-qm", "one"]).unwrap();
            Self { _dir: dir, root }
        }

        fn head(&self) -> String {
            git_ok(&self.root, &["rev-parse", "HEAD"]).unwrap()
        }

        fn status(&self) -> String {
            git_ok(&self.root, &["status", "--porcelain", "--branch"]).unwrap()
        }
    }

    /// AC-01 and AC-02.
    #[test]
    fn creates_a_worktree_at_the_baseline_without_touching_the_checkout() {
        let r = Repo::new();
        let baseline = r.head();
        std::fs::write(r.root.join("lib.rs"), "fn a() { later() }\n").unwrap();
        git_ok(&r.root, &["commit", "-qam", "two"]).unwrap();
        std::fs::write(r.root.join("wip.txt"), "user's work\n").unwrap();
        let before = std::fs::read_to_string(r.root.join("lib.rs")).unwrap();
        let status_before = r.status().replace("\n?? .gitignore", "");

        let w = create(&r.root, "RUN-001", "TASK-002", &baseline).unwrap();
        assert_eq!(w.branch, "zforge/TASK-002/RUN-001");
        assert_eq!(w.path, r.root.join(".zforge/worktrees/RUN-001"));
        assert_eq!(git_ok(&w.path, &["rev-parse", "HEAD"]).unwrap(), baseline);
        assert_eq!(
            git_ok(&w.path, &["branch", "--show-current"]).unwrap(),
            w.branch
        );
        assert_eq!(
            std::fs::read_to_string(w.path.join("lib.rs")).unwrap(),
            "fn a() {}\n"
        );

        assert_eq!(
            git_ok(&r.root, &["branch", "--show-current"]).unwrap(),
            "main"
        );
        assert_eq!(
            std::fs::read_to_string(r.root.join("lib.rs")).unwrap(),
            before
        );
        let after = r.status().replace("\n?? .gitignore", "");
        assert_eq!(
            after, status_before,
            "only .gitignore may appear, and the worktree must not"
        );

        remove(&r.root, &w, false).unwrap();
        assert_eq!(r.status().replace("\n?? .gitignore", ""), status_before);
    }

    /// AC-03: refusals and failures leave nothing behind.
    #[test]
    fn refuses_clearly_and_leaves_nothing_behind() {
        let r = Repo::new();
        let head = r.head();
        let w = create(&r.root, "RUN-001", "TASK-002", &head).unwrap();

        let err = create(&r.root, "RUN-001", "TASK-002", &head)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "{err}");

        let err = create(
            &r.root,
            "RUN-002",
            "TASK-002",
            "0000000000000000000000000000000000000000",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("start commit"), "{err}");
        assert!(!path_for(&r.root, "RUN-002").exists());
        assert!(!branch_exists(&r.root, "zforge/TASK-002/RUN-002").unwrap());

        std::fs::create_dir_all(path_for(&r.root, "RUN-003")).unwrap();
        let err = create(&r.root, "RUN-003", "TASK-002", &head)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "{err}");
        assert!(!branch_exists(&r.root, "zforge/TASK-002/RUN-003").unwrap());

        assert!(create(&r.root, "../x", "TASK-002", &head).is_err());
        assert_eq!(list(&r.root).unwrap(), vec![w]);

        let plain = tempfile::tempdir().unwrap();
        let err = create(plain.path(), "RUN-001", "TASK-002", &head)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not in a git work tree"), "{err}");
    }

    /// AC-04: the branch outlives the worktree; uncommitted work is protected.
    #[test]
    fn remove_keeps_the_branch_and_protects_uncommitted_work() {
        let r = Repo::new();
        let w = create(&r.root, "RUN-001", "TASK-002", &r.head()).unwrap();
        std::fs::write(w.path.join("lib.rs"), "fn a() { agent() }\n").unwrap();

        let err = remove(&r.root, &w, false).unwrap_err().to_string();
        assert!(err.contains("uncommitted changes"), "{err}");
        assert!(w.path.exists());

        git_ok(&w.path, &["commit", "-qam", "agent work"]).unwrap();
        remove(&r.root, &w, false).unwrap();
        assert!(!w.path.exists());
        assert!(branch_exists(&r.root, &w.branch).unwrap());
        assert!(list(&r.root).unwrap().is_empty());
    }

    /// Already ignored by the project: `.gitignore` is not touched.
    #[test]
    fn an_already_ignored_location_leaves_gitignore_alone() {
        let r = Repo::new();
        std::fs::write(r.root.join(".gitignore"), ".zforge/\n").unwrap();
        create(&r.root, "RUN-001", "TASK-002", &r.head()).unwrap();
        assert_eq!(
            std::fs::read_to_string(r.root.join(".gitignore")).unwrap(),
            ".zforge/\n"
        );
    }

    /// AC-05.
    #[test]
    fn gitignore_gets_the_line_exactly_once_and_keeps_the_rest() {
        let r = Repo::new();
        let head = r.head();
        create(&r.root, "RUN-001", "TASK-002", &head).unwrap();
        assert_eq!(
            std::fs::read_to_string(r.root.join(".gitignore")).unwrap(),
            ".zforge/worktrees/\n"
        );

        std::fs::write(r.root.join(".gitignore"), "/target\n*.log").unwrap();
        create(&r.root, "RUN-002", "TASK-002", &head).unwrap();
        create(&r.root, "RUN-003", "TASK-003", &head).unwrap();
        assert_eq!(
            std::fs::read_to_string(r.root.join(".gitignore")).unwrap(),
            "/target\n*.log\n.zforge/worktrees/\n"
        );
        assert_eq!(list(&r.root).unwrap().len(), 3);
    }
}
