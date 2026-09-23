//! Sealing a run's output (MOC-C TASK-001, REQ-002).
//!
//! When the tests pass, what they ran on is a tree — the worktree, with or
//! without the agent's own commits. A task that depends on this one needs a
//! fixed point to start from, so the tested tree is sealed as a commit on the
//! run's branch: everything left uncommitted is committed, and the result is
//! accepted only if the worktree is then clean and its fingerprint is still
//! the candidate the tests ran on. That commit is the run's output; anything
//! committed to the branch afterwards is not.

use super::git;
use anyhow::{bail, Result};
use std::path::Path;

/// Seal `work_dir` — tested as `candidate` — as a commit, and return it.
pub fn seal(work_dir: &Path, message: &str, candidate: &str) -> Result<String> {
    git::commit_all(work_dir, message)?;
    if !git::is_clean(work_dir)? {
        bail!("the worktree still has changes after sealing (did a commit hook rewrite files?)");
    }
    let commit = git::head(work_dir)?;
    match crate::evidence::fingerprint(work_dir).hash() {
        Some(now) if now == candidate => Ok(commit),
        Some(now) => bail!(
            "sealed output {} has candidate {now}, but the tests ran on {candidate}",
            short(&commit)
        ),
        None => bail!(
            "sealed output {} could not be fingerprinted",
            short(&commit)
        ),
    }
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
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
            let root = dir.path().canonicalize().unwrap();
            let r = Self { _dir: dir, root };
            r.git(&["init", "-q", "-b", "main", "."]);
            r.write("a.txt", "one\n");
            r.git(&["add", "-A"]);
            r.git(&[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "base",
            ]);
            r
        }

        fn git(&self, args: &[&str]) -> String {
            git::ok(&self.root, args).unwrap()
        }

        fn write(&self, file: &str, text: &str) {
            std::fs::write(self.root.join(file), text).unwrap();
        }

        fn candidate(&self) -> String {
            crate::evidence::fingerprint(&self.root)
                .hash()
                .unwrap()
                .to_string()
        }
    }

    /// AC-01: uncommitted work becomes the output, and the output is the
    /// tree that was tested.
    #[test]
    fn uncommitted_work_is_sealed_as_the_tested_tree() {
        let r = Repo::new();
        r.write("a.txt", "two\n");
        r.write("new.txt", "fresh\n");
        let tested = r.candidate();

        let commit = seal(&r.root, "zforge: output of RUN-001 (A)", &tested).unwrap();

        assert_eq!(commit, r.git(&["rev-parse", "HEAD"]));
        assert_eq!(r.git(&["show", &format!("{commit}:new.txt")]), "fresh");
        assert_eq!(r.git(&["show", &format!("{commit}:a.txt")]), "two");
        assert!(git::is_clean(&r.root).unwrap());
        assert_eq!(r.candidate(), tested);
        assert_eq!(
            r.git(&["log", "-1", "--format=%an %s"]),
            "zforge zforge: output of RUN-001 (A)"
        );
    }

    /// AC-02: an agent that committed everything leaves nothing to seal;
    /// its own commit is the output and no empty commit is made.
    #[test]
    fn a_clean_worktree_is_sealed_at_its_head() {
        let r = Repo::new();
        r.write("a.txt", "two\n");
        r.git(&["add", "-A"]);
        r.git(&[
            "-c",
            "user.email=a@a",
            "-c",
            "user.name=agent",
            "commit",
            "-qm",
            "agent work",
        ]);
        let head = r.git(&["rev-parse", "HEAD"]);
        let commits = r.git(&["rev-list", "--count", "HEAD"]);

        let commit = seal(&r.root, "zforge: output", &r.candidate()).unwrap();

        assert_eq!(commit, head);
        assert_eq!(r.git(&["rev-list", "--count", "HEAD"]), commits);
    }

    /// The seal never vouches for a tree other than the one tested.
    #[test]
    fn a_tree_that_changed_since_the_tests_is_refused() {
        let r = Repo::new();
        let tested = r.candidate();
        r.write("a.txt", "changed after the tests\n");

        let err = seal(&r.root, "zforge: output", &tested).unwrap_err();

        assert!(
            format!("{err:#}").contains("but the tests ran on"),
            "{err:#}"
        );
    }

    /// A commit hook that rejects the seal fails it with the hook's word;
    /// hooks are never skipped.
    #[cfg(unix)]
    #[test]
    fn a_rejecting_commit_hook_fails_the_seal() {
        use std::os::unix::fs::PermissionsExt;
        let r = Repo::new();
        let hook = r.root.join(".git/hooks/pre-commit");
        std::fs::write(&hook, "#!/bin/sh\necho 'no commits today' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        r.write("a.txt", "two\n");

        let err = seal(&r.root, "zforge: output", &r.candidate()).unwrap_err();

        assert!(format!("{err:#}").contains("no commits today"), "{err:#}");
    }
}
