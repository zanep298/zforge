//! `zforge onboard …` through the real binary (ONBOARD TASK-010).
//!
//! Like intake, the binary runs without a terminal exactly as an agent
//! does: it can review knowledge files but must refuse to decide on them
//! (D1, AC-01). The confirmation code is shared with `zforge intake
//! accept|revise` (`src/cli/confirm.rs`); this file proves the onboard
//! side of that sharing plus the knowledge-specific findings (AC-03..06).

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const DOMAIN_TEXT: &str = "# Domain\n\nA thing happens. (src/x.rs:1)\n\n## Open questions\n";

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
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
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

    fn knowledge_dir(&self) -> PathBuf {
        self.root.join("docs/knowledge")
    }

    fn decisions_log(&self) -> String {
        std::fs::read_to_string(self.knowledge_dir().join(".records/decisions.jsonl"))
            .unwrap_or_default()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// AC-01: no interactive terminal refuses `accept` and `revise`, and
/// nothing beyond the `review` line reaches the decision log.
#[test]
fn onboard_accept_and_revise_need_a_terminal_and_record_nothing_without_one() {
    let p = Project::new();
    std::fs::create_dir_all(p.knowledge_dir()).unwrap();
    std::fs::write(p.knowledge_dir().join("domain.md"), DOMAIN_TEXT).unwrap();

    let out = p.ok(&["onboard", "review", "domain.md"]);
    assert!(out.contains("revision 1"), "{out}");

    let out = p.zforge(&["onboard", "accept", "domain.md"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("needs an interactive terminal"),
        "{}",
        stderr(&out)
    );

    let out = p.zforge(&["onboard", "revise", "domain.md", "--note", "x"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("needs an interactive terminal"),
        "{}",
        stderr(&out)
    );

    let log = p.decisions_log();
    let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "only the review line, got: {log}");
    assert!(lines[0].contains("\"review\""), "{log}");
}

/// AC-02: intake and onboard decisions refuse in the same words — proof
/// that they share the one confirmation helper, not two copies of it.
#[test]
fn intake_and_onboard_refusals_use_the_same_wording() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    std::fs::write(
        p.root.join(".zforge/intakes/F-1/01-outcome.md"),
        "# F-1 — Outcome\n\n## Yêu cầu\n\n- REQ-001: x\n\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    p.ok(&["intake", "review", "F-1", "01-outcome.md"]);
    let intake_err = stderr(&p.zforge(&["intake", "accept", "F-1", "01-outcome.md"]));

    std::fs::create_dir_all(p.knowledge_dir()).unwrap();
    std::fs::write(p.knowledge_dir().join("domain.md"), DOMAIN_TEXT).unwrap();
    p.ok(&["onboard", "review", "domain.md"]);
    let onboard_err = stderr(&p.zforge(&["onboard", "accept", "domain.md"]));

    let common_tail =
        "records the user's decision and needs an interactive terminal; it cannot be run by an agent or from a script";
    assert!(intake_err.contains(common_tail), "{intake_err}");
    assert!(onboard_err.contains(common_tail), "{onboard_err}");
}

/// AC-03: with the default `knowledge.dir`, review writes nothing under
/// `docs/knowledge/` except `.records/` and the reviewed file itself.
#[test]
fn review_writes_only_the_file_and_records_under_the_default_dir() {
    let p = Project::new();
    std::fs::create_dir_all(p.knowledge_dir()).unwrap();
    std::fs::write(p.knowledge_dir().join("domain.md"), DOMAIN_TEXT).unwrap();

    p.ok(&["onboard", "review", "domain.md"]);

    let entries: Vec<String> = std::fs::read_dir(p.knowledge_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    for name in &entries {
        assert!(
            name == "domain.md" || name == ".records",
            "unexpected entry under docs/knowledge/: {name}"
        );
    }
    assert!(p.knowledge_dir().join(".records/decisions.jsonl").is_file());
    assert!(p.root.join(".gitattributes").is_file());
}

/// AC-04: a review the linter refuses (an unknown file) must not create
/// or change `.gitattributes`.
#[test]
fn a_refused_review_leaves_gitattributes_untouched() {
    let p = Project::new();
    std::fs::create_dir_all(p.knowledge_dir()).unwrap();
    std::fs::write(p.knowledge_dir().join("domain.md"), DOMAIN_TEXT).unwrap();

    let out = p.zforge(&["onboard", "review", "glossary.md"]);
    assert!(!out.status.success());
    assert!(!p.root.join(".gitattributes").exists());

    // A lint-refused review on a real knowledge file leaves it untouched too.
    std::fs::write(p.knowledge_dir().join("conventions.md"), "# Conventions\n").unwrap();
    let out = p.zforge(&["onboard", "review", "conventions.md"]);
    assert!(!out.status.success());
    assert!(!p.root.join(".gitattributes").exists());
}
