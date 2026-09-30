//! `zforge onboard draft` / `refresh` through the real binary (ONBOARD
//! TASK-008): a headless, edit-disabled agent call writes a module's
//! section or a stale item's replacement, and zforge — never the agent —
//! sends the result for review.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn refresh_token() {}\n").unwrap();
        let p = Self {
            _dir: dir,
            root,
            home,
        };
        p.git(&["init", "-q"]);
        p.git(&["config", "user.email", "t@t"]);
        p.git(&["config", "user.name", "t"]);
        p.git(&["add", "-A"]);
        p.git(&["commit", "-qm", "base"]);
        p
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    fn commit_all(&self, msg: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", msg]);
    }

    fn knowledge_dir(&self) -> PathBuf {
        self.root.join("docs/knowledge")
    }

    fn domain_md(&self) -> String {
        std::fs::read_to_string(self.knowledge_dir().join("domain.md")).unwrap_or_default()
    }

    fn decisions_log(&self) -> String {
        std::fs::read_to_string(self.knowledge_dir().join(".records/decisions.jsonl"))
            .unwrap_or_default()
    }

    /// Register `claude` as a stub that always answers with `response`
    /// (one `result` stream-json line) and records its argv.
    fn stub(&self, response: &str) {
        std::fs::write(self.home.join("response.json"), response).unwrap();
        let script = self.home.join("claude-stub");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat > /dev/null\necho \"$@\" >> {home}/args\ncat {home}/response.json\n",
                home = self.home.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            self.home.join("registry.yaml"),
            format!(
                "agents:\n  claude:\n    command: {}\n    args: [\"-p\", \"--output-format\", \"stream-json\", \"--verbose\"]\n",
                script.display()
            ),
        )
        .unwrap();
    }

    /// Register `claude` as a stub that exits non-zero without answering.
    fn stub_failing(&self) {
        let script = self.home.join("claude-stub");
        std::fs::write(&script, "#!/bin/sh\ncat > /dev/null\nexit 1\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            self.home.join("registry.yaml"),
            format!(
                "agents:\n  claude:\n    command: {}\n    args: []\n",
                script.display()
            ),
        )
        .unwrap();
    }

    fn args_recorded(&self) -> String {
        std::fs::read_to_string(self.home.join("args")).unwrap_or_default()
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
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
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn result_line(text: &str, cost: f64) -> String {
    serde_json::json!({
        "type": "result",
        "is_error": false,
        "result": text,
        "total_cost_usd": cost,
    })
    .to_string()
}

/// AC-02: with a stub agent, `onboard draft --module internal/token`
/// writes that section, sets `pinned`, and records a review revision.
#[test]
fn draft_writes_the_module_section_pins_and_reviews() {
    let p = Project::new();
    let head = p.head();
    p.stub(&result_line(
        "- DOM-1: refresh tokens are single-use. (src/lib.rs:1)\n",
        0.01,
    ));

    let out = p.ok(&["onboard", "draft", "--module", "internal/token"]);
    assert!(out.contains("sent for review as revision 1"), "{out}");

    let text = p.domain_md();
    assert!(text.contains(&format!("pinned: {head}")), "{text}");
    assert!(text.contains("covers: [internal/token]"), "{text}");
    assert!(text.contains("## internal/token"), "{text}");
    assert!(
        text.contains("- DOM-1: refresh tokens are single-use. (src/lib.rs:1)"),
        "{text}"
    );

    let log = p.decisions_log();
    assert_eq!(log.lines().filter(|l| !l.trim().is_empty()).count(), 1);
    assert!(log.contains("\"review\""), "{log}");
}

/// AC-03: an answer with an uncited item leaves the file unreviewed and
/// reports the lint error.
#[test]
fn draft_with_an_uncited_item_is_refused_by_lint() {
    let p = Project::new();
    p.stub(&result_line(
        "- DOM-1: something happens with no citation\n",
        0.01,
    ));

    let out = p.zforge(&["onboard", "draft", "--module", "internal/token"]);
    assert!(!out.status.success());
    let combined = format!("{}{}", stdout(&out), stderr(&out));
    assert!(combined.contains("no evidence"), "{combined}");

    // Written (so the user can see and fix it), but never sent for review.
    assert!(p.domain_md().contains("DOM-1"));
    assert_eq!(p.decisions_log().trim(), "");
}

/// AC-05: a stub agent that exits non-zero changes no file.
#[test]
fn draft_with_a_failing_agent_changes_no_file() {
    let p = Project::new();
    p.stub_failing();

    let before = p.domain_md();
    let out = p.zforge(&["onboard", "draft", "--module", "internal/token"]);
    assert!(!out.status.success());
    assert_eq!(p.domain_md(), before);
    assert_eq!(p.decisions_log().trim(), "");
}

/// AC-05: a stub agent that reports spending more than the budget changes
/// no file either.
#[test]
fn draft_over_budget_changes_no_file() {
    let p = Project::new();
    p.stub(&result_line(
        "- DOM-1: refresh tokens are single-use. (src/lib.rs:1)\n",
        999.0,
    ));

    let before = p.domain_md();
    let out = p.zforge(&[
        "onboard",
        "draft",
        "--module",
        "internal/token",
        "--budget",
        "0.10",
    ]);
    assert!(!out.status.success());
    let combined = format!("{}{}", stdout(&out), stderr(&out));
    assert!(combined.contains("budget"), "{combined}");
    assert_eq!(p.domain_md(), before);
}

/// AC-06: the agent is started without editing tools.
#[test]
fn draft_starts_the_agent_without_editing_tools() {
    let p = Project::new();
    p.stub(&result_line(
        "- DOM-1: refresh tokens are single-use. (src/lib.rs:1)\n",
        0.01,
    ));

    p.ok(&["onboard", "draft", "--module", "internal/token"]);

    let args = p.args_recorded();
    assert!(args.contains("--disallowedTools"), "{args}");
    assert!(args.contains("Edit"), "{args}");
    assert!(args.contains("Write"), "{args}");
    assert!(args.contains("NotebookEdit"), "{args}");
}

/// AC-04: `onboard refresh` replaces only the stale IDs; every other line
/// is byte-identical.
#[test]
fn refresh_replaces_only_the_stale_item() {
    let p = Project::new();
    std::fs::write(
        p.root.join("src/other.rs"),
        "pub fn audit() {}\npub fn write() {}\n",
    )
    .unwrap();
    p.commit_all("add other.rs");
    let old_head = p.head();

    std::fs::create_dir_all(p.knowledge_dir()).unwrap();
    let domain = format!(
        "---\npinned: {old_head}\n---\n## m\n- DOM-1: old fact. (src/lib.rs:1)\n- DOM-2: another fact. (src/other.rs:1)\n\n## Open questions\n"
    );
    std::fs::write(p.knowledge_dir().join("domain.md"), &domain).unwrap();
    p.ok(&["onboard", "review", "domain.md"]);
    p.commit_all("send domain.md for review");

    // `accept` needs a terminal (D1); this test is about `refresh`, not
    // about accept/revise, so the decision is recorded directly through the
    // library the CLI itself calls, bypassing only the terminal gate.
    let config = zforge::config::load_from(&p.root.join(".zforge/config.yaml")).unwrap();
    let k = zforge::knowledge::Knowledge::open(&config);
    zforge::knowledge::accept(&k, "domain.md", None).unwrap();
    p.commit_all("accept domain.md");

    // Change the line DOM-1 cites; leave DOM-2's file untouched.
    std::fs::write(p.root.join("src/lib.rs"), "pub fn totally_different() {}\n").unwrap();
    p.commit_all("change lib.rs");

    p.stub(&result_line(
        "- DOM-1: refreshed fact. (src/lib.rs:1)\n",
        0.01,
    ));
    let out = p.ok(&["onboard", "refresh"]);
    assert!(out.contains("sent for review as revision 2"), "{out}");

    let text = p.domain_md();
    let expected_head = p.head();
    assert_eq!(
        text,
        format!(
            "---\npinned: {expected_head}\n---\n## m\n- DOM-1: refreshed fact. (src/lib.rs:1)\n- DOM-2: another fact. (src/other.rs:1)\n\n## Open questions\n"
        )
    );
}
