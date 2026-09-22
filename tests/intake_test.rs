//! v1.5 intake through the real binary (Mốc A, decisions D1/D2/D4).
//!
//! The binary runs without a terminal, exactly as an agent does, so it can
//! create, fill and review files but must refuse to accept them. The user's
//! acceptance is exercised through the library: there is deliberately no
//! environment switch that would let a test — or an agent — accept through
//! the CLI without a TTY.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const OUTCOME: &str = "# F-1 — Outcome\n\n## Vấn đề\nCần lọc.\n\n## Yêu cầu\n\n- REQ-001: lọc theo trạng thái\n\n## Câu hỏi còn mở\n\n- [ ] trạng thái archived có tính không?\n";

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();
        Self { _dir: dir, root }
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", self.root.join(".home"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.zforge(args);
        assert!(
            out.status.success(),
            "zforge {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn intake_dir(&self) -> PathBuf {
        self.root.join(".zforge/intakes/F-1")
    }

    fn status(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["intake", "status", "F-1", "--json"])).unwrap()
    }

    fn state_of(&self, file: &str) -> String {
        self.status()
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == file)
            .map(|f| f["state"].as_str().unwrap().to_string())
            .unwrap()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn an_agent_can_prepare_and_review_but_not_accept() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    for stage in [
        "01-outcome.md",
        "02-behavior.md",
        "03-solution.md",
        "04-breakdown.md",
    ] {
        assert!(p.intake_dir().join(stage).is_file(), "{stage}");
    }

    // An untouched template is not reviewable.
    let out = p.zforge(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("no content yet"), "{}", stderr(&out));

    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    let out = p.ok(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(out.contains("revision 1"), "{out}");
    assert_eq!(p.state_of("01-outcome.md"), "in_review");

    // The decision needs a human at a terminal.
    let out = p.zforge(&["intake", "accept", "F-1", "01-outcome.md"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("needs an interactive terminal"),
        "{}",
        stderr(&out)
    );
    let out = p.zforge(&["intake", "revise", "F-1", "01-outcome.md", "--note", "x"]);
    assert!(!out.status.success());
    assert_eq!(
        p.state_of("01-outcome.md"),
        "in_review",
        "nothing was recorded"
    );

    let status = p.status();
    let outcome = status
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == "01-outcome.md")
        .unwrap();
    assert_eq!(
        outcome["open_questions"][0],
        "trạng thái archived có tính không?"
    );
}

/// D2 through the files the CLI shows: acceptance binds to the reviewed
/// hash, and an edit afterwards is a draft over the accepted revision.
#[test]
fn acceptance_binds_to_the_reviewed_revision() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    p.ok(&["intake", "review", "F-1", "01-outcome.md"]);

    let intake = zforge::intake::Intake::open(&p.root, "F-1").unwrap();
    zforge::intake::review::accept(&intake, "01-outcome.md", Some("user".into())).unwrap();
    assert_eq!(p.state_of("01-outcome.md"), "accepted");

    std::fs::write(
        p.intake_dir().join("01-outcome.md"),
        OUTCOME.replace("lọc theo trạng thái", "lọc theo trạng thái và nhãn"),
    )
    .unwrap();
    let st = p.status();
    let outcome = st
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == "01-outcome.md")
        .unwrap();
    assert_eq!(outcome["state"], "accepted");
    assert_eq!(outcome["has_draft"], true);
    assert_eq!(outcome["accepted"]["revision"], 1);

    let out = p.ok(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(
        out.contains("revision 2") && out.contains("+- REQ-001: lọc theo trạng thái và nhãn"),
        "{out}"
    );

    // Edited again before the decision: the review no longer describes it.
    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    assert_eq!(p.state_of("01-outcome.md"), "changed_since_review");
    let err = zforge::intake::review::accept(&intake, "01-outcome.md", None).unwrap_err();
    assert!(
        err.to_string().contains("changed after revision 2"),
        "{err}"
    );
}

#[test]
fn tasks_are_linted_against_the_outcome() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    p.ok(&["intake", "task", "F-1", "TASK-001"]);

    let out = p.zforge(&["intake", "review", "F-1", "tasks/TASK-001.md"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("links to no requirement"), "{err}");
    assert!(err.contains("section \"Mục tiêu\" is empty"), "{err}");

    let out = p.zforge(&["intake", "review", "F-1", "../../config.yaml"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("is not an intake file"));
}
