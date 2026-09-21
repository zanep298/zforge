//! Identify the code a verification ran against (IMP-002).
//!
//! A passing `verify.md` used to count for whatever code happened to be on
//! disk later: edit the code after a green run and `review --done` still
//! signed off on the old report. The fix is to record *which* code was
//! verified and compare it with the code being reviewed.
//!
//! The fingerprint is a git tree hash of the working tree — tracked
//! changes, staged or not, and untracked files that `.gitignore` does not
//! exclude — built in a throwaway index so the user's real index and staging
//! are never touched. zforge's own metadata (`.zforge/`) and the CodeGraph
//! index (`.codegraph/`) are left out: the pipeline writes them during and
//! after verification, and counting them would invalidate every report the
//! moment it is written. When the project is a subdirectory of the
//! repository, only that subtree is fingerprinted, so commits elsewhere in a
//! monorepo do not invalidate it.

use std::path::Path;
use std::process::{Command, Output};

/// Directories excluded from the fingerprint, relative to the project root.
const EXCLUDED: [&str; 2] = [".zforge", ".codegraph"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    /// Git tree hash of the project's code.
    Git(String),
    /// No fingerprint could be taken; the reason says why (not a git
    /// repository, git missing, …).
    Unavailable(String),
}

impl Candidate {
    pub fn hash(&self) -> Option<&str> {
        match self {
            Self::Git(h) => Some(h),
            Self::Unavailable(_) => None,
        }
    }
}

/// Fingerprint the code under `project_root`.
pub fn fingerprint(project_root: &Path) -> Candidate {
    match try_fingerprint(project_root) {
        Ok(hash) => Candidate::Git(hash),
        Err(reason) => Candidate::Unavailable(reason),
    }
}

fn try_fingerprint(root: &Path) -> Result<String, String> {
    let inside = git(root, None, &["rev-parse", "--is-inside-work-tree"])?;
    if String::from_utf8_lossy(&inside.stdout).trim() != "true" {
        return Err("not a git work tree".into());
    }
    let prefix = stdout(git(root, None, &["rev-parse", "--show-prefix"])?);

    let index = TempIndex::new();
    let idx = Some(index.path());

    let has_head = git(root, None, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok();
    if has_head {
        git(root, idx, &["read-tree", "HEAD"])?;
    } else {
        git(root, idx, &["read-tree", "--empty"])?;
    }
    let mut add = vec!["add", "-A", "--", "."];
    let excludes: Vec<String> = EXCLUDED.iter().map(|d| format!(":(exclude){d}")).collect();
    add.extend(excludes.iter().map(String::as_str));
    git(root, idx, &add)?;
    // Metadata already tracked in HEAD would otherwise stay in the tree.
    let mut rm = vec!["rm", "-r", "-q", "--cached", "--ignore-unmatch", "--"];
    rm.extend(EXCLUDED);
    git(root, idx, &rm)?;

    let tree = stdout(git(root, idx, &["write-tree"])?);
    if prefix.is_empty() {
        return Ok(tree);
    }
    // Project in a subdirectory: its own subtree.
    let spec = format!("{tree}:{}", prefix.trim_end_matches('/'));
    Ok(stdout(git(root, None, &["rev-parse", &spec])?))
}

/// Path for a throwaway index; git creates the file, the guard removes it
/// (and any `.lock` git left) however the fingerprint ends.
struct TempIndex(std::path::PathBuf);

impl TempIndex {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Self(std::env::temp_dir().join(format!("zforge-index-{}-{nanos}", std::process::id())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("lock"));
    }
}

fn git(root: &Path, index: Option<&Path>, args: &[&str]) -> Result<Output, String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(root);
    if let Some(idx) = index {
        cmd.env("GIT_INDEX_FILE", idx);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("git not available: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out)
}

fn stdout(out: Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Repo {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            run(&root, &["init", "-q", "."]);
            run(&root, &["config", "user.email", "t@t"]);
            run(&root, &["config", "user.name", "t"]);
            write(&root, "a.rs", "fn a() {}\n");
            write(&root, ".gitignore", "target/\n");
            write(&root, ".zforge/tasks/T1/.state.yaml", "state: Coded\n");
            run(&root, &["add", "-A"]);
            run(&root, &["commit", "-qm", "init"]);
            Self { _dir: dir, root }
        }

        fn fp(&self) -> String {
            match fingerprint(&self.root) {
                Candidate::Git(h) => h,
                other => panic!("expected a fingerprint, got {other:?}"),
            }
        }
    }

    fn run(root: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn code_changes_change_the_fingerprint() {
        let r = Repo::new();
        let clean = r.fp();

        write(&r.root, "a.rs", "fn a() { changed() }\n");
        let edited = r.fp();
        assert_ne!(
            clean, edited,
            "an uncommitted edit is a different candidate"
        );

        write(&r.root, "a.rs", "fn a() {}\n");
        assert_eq!(r.fp(), clean, "reverting restores the candidate");

        write(&r.root, "new.rs", "fn n() {}\n");
        assert_ne!(r.fp(), clean, "a new untracked source file counts");
    }

    // The pipeline's own writes must not invalidate the evidence they record.
    #[test]
    fn metadata_and_ignored_files_do_not() {
        let r = Repo::new();
        let clean = r.fp();
        write(&r.root, ".zforge/tasks/T1/verify.md", "passed: true\n");
        write(&r.root, ".zforge/tasks/T1/.state.yaml", "state: Verified\n");
        write(&r.root, ".codegraph/index.db", "x");
        write(&r.root, "target/debug/out", "x");
        assert_eq!(r.fp(), clean);
    }

    #[test]
    fn the_real_index_is_untouched() {
        let r = Repo::new();
        write(&r.root, "staged.rs", "fn s() {}\n");
        run(&r.root, &["add", "staged.rs"]);
        write(&r.root, "a.rs", "fn a() { unstaged() }\n");
        let before = std::fs::read(r.root.join(".git/index")).unwrap();

        r.fp();

        assert_eq!(std::fs::read(r.root.join(".git/index")).unwrap(), before);
    }

    #[test]
    fn a_project_in_a_subdirectory_ignores_changes_elsewhere() {
        let r = Repo::new();
        write(&r.root, "svc/lib.rs", "fn s() {}\n");
        run(&r.root, &["add", "-A"]);
        run(&r.root, &["commit", "-qm", "svc"]);
        let svc = r.root.join("svc");
        let before = fingerprint(&svc);

        write(&r.root, "a.rs", "fn a() { other_project() }\n");
        run(&r.root, &["commit", "-qam", "elsewhere"]);
        assert_eq!(fingerprint(&svc), before);

        write(&svc, "lib.rs", "fn s() { changed() }\n");
        assert_ne!(fingerprint(&svc), before);
    }

    #[test]
    fn a_repository_without_commits_is_fingerprinted() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q", "."]);
        write(dir.path(), "f.rs", "x\n");
        assert!(matches!(fingerprint(dir.path()), Candidate::Git(_)));
    }

    #[test]
    fn outside_git_there_is_no_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(fingerprint(dir.path()), Candidate::Unavailable(_)));
    }
}
