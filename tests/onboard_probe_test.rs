//! `zforge onboard` through the real binary (ONBOARD TASK-003): the probe,
//! the baseline it records, the known-failure list, and how `zforge
//! status` and `zforge init` react to onboarding — end to end, not just
//! through the library (see `src/knowledge/probe.rs`, `src/onboard.rs` for
//! the unit-level coverage).

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new(test_command: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            format!("project:\n  name: t\n  language: rust\n  test_command: \"{test_command}\"\n"),
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

    fn commit_all(&self) {
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&self.root)
                .output()
                .unwrap();
        };
        git(&["add", "-A"]);
        git(&["commit", "-q", "--allow-empty", "-m", "c"]);
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
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}
fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// AC-01: a clean project with a passing suite records the commit, the
/// language and a green result, and creates the three stub files.
#[test]
fn onboard_probes_a_clean_green_project() {
    let p = Project::new("true");
    p.commit_all();

    let out = p.ok(&["onboard"]);
    assert!(out.contains("PASS"), "{out}");
    assert!(out.contains("language   rust"), "{out}");

    let baseline = std::fs::read_to_string(p.knowledge_dir().join("baseline.md")).unwrap();
    assert!(baseline.contains("result: true"), "{baseline}");
    for f in ["domain.md", "conventions.md", "rules.md"] {
        assert!(p.knowledge_dir().join(f).is_file(), "{f} not created");
    }
}

/// AC-02: a failing suite lists its failing tests in `baseline.md`.
#[test]
fn onboard_records_failing_tests() {
    let p = Project::new("__placeholder__");
    std::fs::write(
        p.root.join("fake_test.sh"),
        "#!/bin/sh\nprintf -- '--- FAIL: TestA (0.00s)\\nFAIL\\n'\nexit 1\n",
    )
    .unwrap();
    p.commit_all();
    // Point the config at the script now that its path is known.
    std::fs::write(
        p.root.join(".zforge/config.yaml"),
        format!(
            "project:\n  name: t\n  language: go\n  test_command: \"sh {}\"\n",
            p.root.join("fake_test.sh").display()
        ),
    )
    .unwrap();
    p.commit_all();

    let out = p.ok(&["onboard"]);
    assert!(out.contains("FAIL"), "{out}");
    assert!(out.contains("TestA"), "{out}");

    let baseline = std::fs::read_to_string(p.knowledge_dir().join("baseline.md")).unwrap();
    assert!(baseline.contains("- TestA"), "{baseline}");
}

/// AC-03: uncommitted changes make `zforge onboard` refuse; nothing is
/// written.
#[test]
fn onboard_refuses_a_dirty_working_tree() {
    let p = Project::new("true");
    p.commit_all();
    std::fs::write(p.root.join("dirty.txt"), "uncommitted").unwrap();

    let out = p.zforge(&["onboard"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("uncommitted changes"),
        "{}",
        stderr(&out)
    );
    assert!(!p.knowledge_dir().exists());
}

/// AC-04: a test command that does not exist is recorded as red with the
/// reason, and `zforge onboard` still exits 0.
#[test]
fn onboard_reports_a_missing_test_command_as_red_and_still_exits_zero() {
    let p = Project::new("zforge-onboard-test-nonexistent-xyz");
    p.commit_all();

    let out = p.zforge(&["onboard"]);
    assert!(
        out.status.success(),
        "zforge onboard must exit 0 even on a red baseline: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("RED"), "{text}");

    let baseline = std::fs::read_to_string(p.knowledge_dir().join("baseline.md")).unwrap();
    assert!(baseline.contains("result: false"), "{baseline}");
    assert!(baseline.contains("reason:"), "{baseline}");
}

/// AC-05: `onboard baseline --known` without a terminal is refused; with a
/// typed confirmation (simulated here by calling the library directly is
/// not applicable — the binary path always lacks a terminal in tests) the
/// command still refuses, proving no flag bypasses it (D1).
#[test]
fn onboard_baseline_known_needs_a_terminal() {
    let p = Project::new("true");
    p.commit_all();
    p.ok(&["onboard"]);

    let out = p.zforge(&["onboard", "baseline", "--known", "TestA"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("needs an interactive terminal"),
        "{}",
        stderr(&out)
    );

    let out = p.zforge(&["onboard", "baseline", "--clear"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("needs an interactive terminal"),
        "{}",
        stderr(&out)
    );
}

/// AC-06: `zforge status` names `zforge onboard` as next while the project
/// is not onboarded.
#[test]
fn status_points_at_onboard_while_not_onboarded() {
    let p = Project::new("true");
    p.commit_all();

    let text = p.ok(&["status"]);
    assert!(text.contains("project:"), "{text}");
    assert!(text.contains("zforge onboard"), "{text}");

    let json: serde_json::Value = serde_json::from_str(&p.ok(&["status", "--json"])).unwrap();
    assert_eq!(json["onboarding"]["onboarded"], false);
    assert_eq!(json["onboarding"]["next"], "zforge onboard");
}
