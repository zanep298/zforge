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
//! monorepo do not invalidate it. Code a tree hash cannot see — inside a
//! checked-out submodule, or behind a symlink out of the project — is
//! folded in by [`super::linked`].

use std::path::Path;
use std::process::{Command, Output, Stdio};

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
    let code_tree = if prefix.is_empty() {
        tree
    } else {
        // Project in a subdirectory: its own subtree.
        let spec = format!("{tree}:{}", prefix.trim_end_matches('/'));
        stdout(git(root, None, &["rev-parse", &spec])?)
    };
    super::linked::with_linked_inputs(root, idx, code_tree, try_fingerprint)
}

/// Path for a throwaway index; git creates the file, the guard removes it
/// (and any `.lock` git left) however the fingerprint ends.
struct TempIndex(std::path::PathBuf);

impl TempIndex {
    /// Unique per process *and* per call. A timestamp alone was not: clock
    /// resolution let two threads fingerprinting at once pick the same file,
    /// and git then failed on the other's index lock.
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!("zforge-index-{}-{seq}", std::process::id())))
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

/// Variables that bind git to a particular repository, as listed by
/// `git rev-parse --local-env-vars` (git clears the same set before it
/// descends into a submodule). Inherited from a git hook or a user's shell
/// they would point every call at that repository instead of `root`, so
/// they are removed; `GIT_INDEX_FILE` is set again when a throwaway index
/// is in use.
const REPO_LOCAL_GIT_ENV: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

pub(super) fn git(root: &Path, index: Option<&Path>, args: &[&str]) -> Result<Output, String> {
    run_git(root, index, args, None)
}

/// [`git`] with `input` on stdin (`hash-object --stdin`, `--stdin-paths`).
pub(super) fn git_with_input(root: &Path, args: &[&str], input: &[u8]) -> Result<Output, String> {
    run_git(root, None, args, Some(input))
}

fn run_git(
    root: &Path,
    index: Option<&Path>,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Output, String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(root);
    for var in REPO_LOCAL_GIT_ENV {
        cmd.env_remove(var);
    }
    if let Some(idx) = index {
        cmd.env("GIT_INDEX_FILE", idx);
    }
    let out = match input {
        None => cmd.output(),
        Some(bytes) => {
            use std::io::Write;
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd.spawn().and_then(|mut child| {
                // Written from a thread: `--stdin-paths` answers each line
                // as it goes, so for a long listing git's stdout would fill
                // while we were still writing its stdin.
                let mut stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| std::io::Error::other("git stdin was not piped"))?;
                let bytes = bytes.to_vec();
                let writer = std::thread::spawn(move || stdin.write_all(&bytes));
                let out = child.wait_with_output()?;
                writer
                    .join()
                    .map_err(|_| std::io::Error::other("stdin writer panicked"))??;
                Ok(out)
            })
        }
    }
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

pub(super) fn stdout(out: Output) -> String {
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

    // Invariant guard: every temp index gets its own path. (The clock-based
    // name this replaced only collided between threads at the same instant,
    // which neither this nor the concurrent test reproduces reliably.)
    #[test]
    fn temp_indexes_are_distinct() {
        let a = TempIndex::new();
        let b = TempIndex::new();
        assert_ne!(a.path(), b.path());
    }

    // Several fingerprints at once in one process must not share an index.
    #[test]
    fn concurrent_fingerprints_do_not_collide() {
        let r = Repo::new();
        let expected = r.fp();
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let root = r.root.clone();
                std::thread::spawn(move || fingerprint(&root))
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), Candidate::Git(expected.clone()));
        }
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
