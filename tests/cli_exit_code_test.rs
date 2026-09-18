//! CLI exit-code contract (FIX-001).
//!
//! `verify` and `ship` used to return `Ok(())` no matter what the test suite
//! said, so the process exited 0 while `verify.md` recorded `passed: false`.
//! Anything driving zforge from a script or CI step — the case autonomous
//! execution depends on — read that as a green run.
//!
//! These drive the real binary, because the exit code is the thing under test.

use std::path::Path;
use std::process::Command;

fn zforge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_zforge")
}

const EXIT_FAILED: i32 = 1;
const EXIT_BLOCKED: i32 = 2;

fn write_config(root: &Path, test_command: &str) {
    std::fs::write(
        root.join(".zforge/config.yaml"),
        format!(
            r#"project:
  name: "test"
  language: "shell"
  test_command: "{test_command}"
  root_dir: "."
opencode:
  model: "claude-sonnet-4-6"
  context_files: []
paths:
  tasks: "./.zforge/tasks"
  agents: "./.zforge/agents"
  memory: "./.zforge/memory"
  skills: "./.zforge/skills"
review:
  auto_approve: false
"#
        ),
    )
    .unwrap();
}

fn state_yaml(state: &str) -> String {
    format!(
        r#"task_id: T1
flow: Full
state: {state}
updated_at: "2026-01-01T00:00:00+00:00"
history:
  - state: Imported
    at: "2026-01-01T00:00:00+00:00"
    note: ""
  - state: {state}
    at: "2026-01-01T00:00:00+00:00"
    note: "test fixture"
"#
    )
}

/// Scaffold a project with task T1 at `state` and the given `test_command`.
fn scaffold(root: &Path, test_command: &str, state: &str) {
    std::fs::create_dir_all(root.join(".zforge/tasks/T1")).unwrap();
    std::fs::create_dir_all(root.join(".zforge/agents")).unwrap();
    write_config(root, test_command);
    std::fs::write(
        root.join(".zforge/agents/verify-analysis.tmpl"),
        "Analyze {{task_id}}\n",
    )
    .unwrap();
    for name in ["task.md", "spec.md", "testspec.md", "plan.md"] {
        std::fs::write(
            root.join(".zforge/tasks/T1").join(name),
            format!("# {name}\nstub\n"),
        )
        .unwrap();
    }
    std::fs::write(root.join(".zforge/tasks/T1/.state.yaml"), state_yaml(state)).unwrap();
}

fn run(project: &Path, args: &[&str]) -> std::process::Output {
    Command::new(zforge_bin())
        .args(args)
        .current_dir(project)
        .env("ZFORGE_HOME", project.join("home"))
        .output()
        .expect("run zforge")
}

#[test]
fn verify_exits_nonzero_when_tests_fail() {
    let project = tempfile::tempdir().unwrap();
    scaffold(project.path(), "sh -c 'exit 1'", "Coded");

    let out = run(project.path(), &["verify", "T1"]);
    assert_eq!(
        out.status.code(),
        Some(EXIT_FAILED),
        "verify must exit {EXIT_FAILED} on a red suite.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // The exit code and the written report must agree.
    let report =
        std::fs::read_to_string(project.path().join(".zforge/tasks/T1/verify.md")).unwrap();
    assert!(
        report.contains("passed: false"),
        "verify.md should record the failure"
    );
}

#[test]
fn verify_exits_zero_when_tests_pass() {
    let project = tempfile::tempdir().unwrap();
    scaffold(project.path(), "sh -c 'exit 0'", "Coded");

    let out = run(project.path(), &["verify", "T1"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "verify must exit 0 on a green suite.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn ship_exits_nonzero_when_tests_fail() {
    let project = tempfile::tempdir().unwrap();
    // Already at Coded, so ship skips the code phase and goes straight to
    // verify — no agent binary needed.
    scaffold(project.path(), "sh -c 'exit 1'", "Coded");

    let out = run(project.path(), &["ship", "T1"]);
    assert_eq!(
        out.status.code(),
        Some(EXIT_FAILED),
        "ship must exit {EXIT_FAILED} on a red suite.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn ship_exits_blocked_when_the_gate_refuses() {
    let project = tempfile::tempdir().unwrap();
    // Imported is far short of the PlanReviewed gate the full flow requires.
    scaffold(project.path(), "sh -c 'exit 0'", "Imported");

    let out = run(project.path(), &["ship", "T1"]);
    assert_eq!(
        out.status.code(),
        Some(EXIT_BLOCKED),
        "a refused gate is distinct from a failed run.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    // Blocked means nothing ran.
    assert!(
        !project.path().join(".zforge/tasks/T1/verify.md").exists(),
        "a blocked ship must not produce a verify report"
    );
}

#[test]
fn unknown_task_exits_nonzero() {
    let project = tempfile::tempdir().unwrap();
    scaffold(project.path(), "sh -c 'exit 0'", "Coded");

    let out = run(project.path(), &["verify", "NOPE"]);
    assert_eq!(
        out.status.code(),
        Some(EXIT_FAILED),
        "an operation that cannot run at all must not exit 0"
    );
}
