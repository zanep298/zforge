#![cfg(unix)]
//! Verification evidence is bound to the code it verified (IMP-002).
//!
//! Driven through the real binary in a real git repository: the property is
//! about what `review --done` accepts after the code on disk changes, which
//! only an end-to-end run shows.

use std::path::PathBuf;
use std::process::{Command, Output};

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    /// Git repo with committed code and task T1 at Coded.
    fn git(test_command: &str) -> Self {
        let p = Self::plain(test_command);
        p.git_cmd(&["init", "-q", "."]);
        p.git_cmd(&["config", "user.email", "t@t"]);
        p.git_cmd(&["config", "user.name", "t"]);
        p.git_cmd(&["add", "-A"]);
        p.git_cmd(&["commit", "-qm", "init"]);
        p
    }

    /// Same project without git.
    fn plain(test_command: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::write(root.join("lib.rs"), "fn a() {}\n").unwrap();
        std::fs::create_dir_all(root.join(".zforge/tasks/T1")).unwrap();
        std::fs::create_dir_all(root.join(".zforge/agents")).unwrap();
        std::fs::write(
            root.join(".zforge/agents/verify-analysis.tmpl"),
            "Analyze {{task_id}}\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            format!(
                "project:\n  name: t\n  language: shell\n  test_command: \"{}\"\n",
                test_command.replace('\\', "\\\\").replace('"', "\\\"")
            ),
        )
        .unwrap();
        for f in ["task.md", "spec.md", "testspec.md", "plan.md"] {
            std::fs::write(root.join(".zforge/tasks/T1").join(f), "# stub\n").unwrap();
        }
        std::fs::write(
            root.join(".zforge/tasks/T1/.state.yaml"),
            "task_id: T1\nflow: Full\nstate: Coded\nupdated_at: \"2026-01-01T00:00:00+00:00\"\nhistory: []\n",
        )
        .unwrap();
        Self { _dir: dir, root }
    }

    fn git_cmd(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    fn zforge(&self, args: &[&str]) -> Output {
        self.zforge_with_env(args, &[])
    }

    fn zforge_with_env(&self, args: &[&str], env: &[(&str, &std::path::Path)]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", self.root.join(".home"))
            .envs(env.iter().copied())
            .output()
            .unwrap()
    }

    fn verify(&self) -> Output {
        let out = self.zforge(&["verify", "T1"]);
        assert!(out.status.success(), "verify: {}", err(&out));
        out
    }

    /// `review --done` with a summary in place.
    fn review_done(&self) -> Output {
        std::fs::write(
            self.task("review-summary.md"),
            "## Summary\nok\n\n## New approved patterns\n",
        )
        .unwrap();
        self.zforge(&["review", "T1", "--done"])
    }

    fn task(&self, name: &str) -> PathBuf {
        self.root.join(".zforge/tasks/T1").join(name)
    }

    fn state(&self) -> String {
        std::fs::read_to_string(self.task(".state.yaml")).unwrap()
    }

    fn edit_code(&self) {
        std::fs::write(self.root.join("lib.rs"), "fn a() { changed() }\n").unwrap();
    }

    fn frontmatter(&self) -> serde_yaml::Value {
        let report = std::fs::read_to_string(self.task("verify.md")).unwrap();
        let fm = report
            .strip_prefix("---\n")
            .and_then(|r| r.split("\n---\n").next())
            .unwrap();
        serde_yaml::from_str(fm).unwrap()
    }
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn review_accepts_evidence_for_the_code_on_disk() {
    let p = Project::git("sh -c 'exit 0'");
    p.verify();
    let fm = p.frontmatter();
    assert_eq!(
        fm["candidate"].as_str().map(str::len),
        Some(40),
        "verify.md records the verified tree: {fm:?}"
    );

    let out = p.review_done();
    assert!(out.status.success(), "{}", err(&out));
    assert!(p.state().contains("state: Reviewed"));
}

/// The gap IMP-002 closes: code edited after a green run was still reviewed
/// on the old report.
#[test]
fn review_refuses_evidence_for_code_that_has_since_changed() {
    let p = Project::git("sh -c 'exit 0'");
    p.verify();
    p.edit_code();

    let out = p.review_done();
    assert!(!out.status.success(), "stale evidence must not be accepted");
    assert!(
        err(&out).contains("code changed after it was verified"),
        "{}",
        err(&out)
    );
    assert!(
        p.state().contains("state: Verified"),
        "review must not advance"
    );
}

#[test]
fn verifying_again_after_a_change_makes_the_evidence_current() {
    let p = Project::git("sh -c 'exit 0'");
    p.verify();
    p.edit_code();
    p.verify();

    let out = p.review_done();
    assert!(out.status.success(), "{}", err(&out));
}

/// A narrowed run is not the configured gate.
#[test]
fn review_refuses_evidence_from_a_different_command() {
    let p = Project::git("sh -c 'exit 0'");
    let out = p.zforge(&["verify", "T1", "--command", "sh -c 'exit 0; # narrowed'"]);
    assert!(out.status.success(), "{}", err(&out));

    let out = p.review_done();
    assert!(!out.status.success());
    assert!(
        err(&out).contains("configured test command"),
        "{}",
        err(&out)
    );
}

/// Files the suite itself writes must not make its own evidence stale.
#[test]
fn files_generated_by_the_suite_do_not_invalidate_it() {
    let p = Project::git("sh -c 'echo out > generated.txt'");
    p.verify();
    assert!(
        p.frontmatter()["candidate_note"]
            .as_str()
            .unwrap_or("")
            .contains("changed while the suite ran"),
        "the change during the run is noted"
    );

    let out = p.review_done();
    assert!(out.status.success(), "{}", err(&out));
}

/// Writes under .zforge/ (reports, state) are not part of the candidate.
#[test]
fn pipeline_metadata_does_not_invalidate_evidence() {
    let p = Project::git("sh -c 'exit 0'");
    p.verify();
    std::fs::write(p.root.join(".zforge/memory-note.md"), "x").unwrap();
    std::fs::write(p.task("notes.md"), "x").unwrap();
    let out = p.review_done();
    assert!(out.status.success(), "{}", err(&out));
}

#[test]
fn every_verification_is_kept_in_the_history() {
    let p = Project::git("sh -c 'exit 0'");
    p.verify();
    p.edit_code();
    p.verify();

    let lines: Vec<serde_json::Value> = std::fs::read_to_string(p.task("verify-history.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l["passed"] == true));
    assert_ne!(
        lines[0]["candidate"], lines[1]["candidate"],
        "each run records the code it verified"
    );
}

/// A report from before candidates were recorded is not trusted.
#[test]
fn report_without_a_candidate_is_refused() {
    let p = Project::git("sh -c 'exit 0'");
    p.verify();
    let report = std::fs::read_to_string(p.task("verify.md")).unwrap();
    let stripped: String = report
        .lines()
        .filter(|l| !l.starts_with("candidate"))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(p.task("verify.md"), stripped).unwrap();

    let out = p.review_done();
    assert!(!out.status.success());
    assert!(
        err(&out).contains("does not record which code"),
        "{}",
        err(&out)
    );
}

/// Outside git there is no fingerprint: said so, review proceeds.
#[test]
fn outside_git_the_review_proceeds_with_a_warning() {
    let p = Project::plain("sh -c 'exit 0'");
    p.verify();
    assert!(p.frontmatter()["candidate"].is_null());

    let out = p.review_done();
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        err(&out).contains("not tied to a code revision"),
        "{}",
        err(&out)
    );
}

/// Evidence is written before state: if the report cannot be written the
/// task must not reach Verified.
#[test]
fn a_failed_report_write_leaves_the_task_unverified() {
    let p = Project::git("sh -c 'exit 0'");
    std::fs::create_dir_all(p.task("verify.md")).unwrap(); // a directory: write fails

    let out = p.zforge(&["verify", "T1"]);
    assert!(!out.status.success(), "verify must report the failed write");
    assert!(p.state().contains("state: Coded"), "{}", p.state());
}

/// zforge run from a git hook (or any shell with `GIT_DIR` / `GIT_WORK_TREE`
/// exported for another repository) must still fingerprint the project it
/// works on. Inheriting those variables pointed every git call at the other
/// repository, whose tree never changes — so edits to the project went
/// unnoticed and stale evidence was accepted.
#[test]
fn inherited_git_environment_does_not_redirect_the_fingerprint() {
    let p = Project::git("sh -c 'exit 0'");
    let other = Project::git("sh -c 'exit 0'");
    let git_dir = other.root.join(".git");
    let env: [(&str, &std::path::Path); 2] =
        [("GIT_DIR", &git_dir), ("GIT_WORK_TREE", &other.root)];

    let out = p.zforge_with_env(&["verify", "T1"], &env);
    assert!(out.status.success(), "verify: {}", err(&out));
    p.edit_code();
    std::fs::write(
        p.task("review-summary.md"),
        "## Summary\nok\n\n## New approved patterns\n",
    )
    .unwrap();
    let out = p.zforge_with_env(&["review", "T1", "--done"], &env);

    assert!(
        !out.status.success(),
        "edits to the project must invalidate its evidence"
    );
    assert!(
        p.state().contains("state: Verified"),
        "review must not advance"
    );
}
