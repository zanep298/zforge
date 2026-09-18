//! Re-verification semantics (FIX-002).
//!
//! `verify` is a question, not a one-way pipeline step: it can be asked
//! repeatedly, and the answer it gets last is the one that counts. Three
//! properties are covered here:
//!
//!   pass → pass   re-running a green suite is allowed, not an
//!                 `InvalidTransition: Verified → Verified` error
//!   pass → fail   the earlier pass is withdrawn and `review --done` is
//!                 refused, instead of `Verified` surviving a red suite
//!   fail → pass   a fixed suite restores `Verified` normally
//!
//! Before this, a task that passed once kept `Verified` forever and
//! `review --done` promoted it to `Reviewed` on top of a failing suite.

mod common;

use common::{make_project, TestHome};
use serial_test::serial;
use std::path::{Path, PathBuf};
use zforge::state::{Flow, State, TaskState};

struct CwdGuard {
    prev: PathBuf,
}

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self { prev }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev);
    }
}

/// `test_command` reads a control file: `pass` exits 0, anything else exits 1.
/// Flipping the file between calls simulates a suite that regresses (or gets
/// fixed) without touching the zforge config.
fn write_switchable_test_command(script_dir: &Path, switch_path: &Path) -> String {
    let script_path = script_dir.join("test.sh");
    let body = format!(
        "#!/bin/bash\n[ \"$(cat '{path}')\" = pass ]\n",
        path = switch_path.display()
    );
    std::fs::write(&script_path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&script_path).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&script_path, perm).unwrap();
    }
    format!("bash {}", script_path.display())
}

fn set_switch(switch_path: &Path, value: &str) {
    std::fs::write(switch_path, value).unwrap();
}

fn write_config(project: &Path, test_command: &str) {
    let cfg = format!(
        r#"project:
  name: "test"
  language: "shell"
  test_command: "{cmd}"
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
"#,
        cmd = test_command.replace('"', "\\\""),
    );
    std::fs::write(project.join(".zforge/config.yaml"), cfg).unwrap();
}

/// Minimal templates so the verify-analysis render on failure does not bail.
fn write_min_agents_dir(project: &Path) {
    let dir = project.join(".zforge/agents");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("verify-analysis.tmpl"), "Analyze {{task_id}}\n").unwrap();
    std::fs::write(dir.join("review.tmpl"), "Review {{task_id}}\n").unwrap();
}

/// A task parked at `Coded` with the artifacts `verify` and `review` expect.
fn make_coded_task(project: &Path, task_id: &str) {
    let tasks_dir = project.join(".zforge/tasks");
    std::fs::create_dir_all(tasks_dir.join(task_id)).unwrap();
    for name in ["task.md", "spec.md", "testspec.md", "plan.md"] {
        std::fs::write(
            tasks_dir.join(task_id).join(name),
            format!("# {name}\nstub\n"),
        )
        .unwrap();
    }

    let mut ts = TaskState::new_with_flow(task_id, Flow::Full);
    for next in [
        State::SpecDone,
        State::TestspecDone,
        State::TestspecReviewed,
        State::Planned,
        State::PlanReviewed,
        State::Coded,
    ] {
        ts.advance(next, "test").unwrap();
    }
    ts.save(&tasks_dir).unwrap();
}

struct Fixture {
    _home: TestHome,
    _guard: CwdGuard,
    _script_dir: tempfile::TempDir,
    project: tempfile::TempDir,
    switch: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = TestHome::new();
        let project = tempfile::tempdir().unwrap();
        make_project(project.path());
        write_min_agents_dir(project.path());

        let script_dir = tempfile::tempdir().unwrap();
        let switch = script_dir.path().join("switch");
        let cmd = write_switchable_test_command(script_dir.path(), &switch);
        write_config(project.path(), &cmd);

        let guard = CwdGuard::enter(project.path());
        make_coded_task(project.path(), "T1");

        Self {
            _home: home,
            _guard: guard,
            _script_dir: script_dir,
            project,
            switch,
        }
    }

    fn tasks_dir(&self) -> PathBuf {
        self.project.path().join(".zforge/tasks")
    }

    fn state(&self) -> State {
        TaskState::load(&self.tasks_dir(), "T1").unwrap().state
    }

    fn verify(&self) -> zforge::cli::outcome::OperationOutcome {
        zforge::cli::verify::run("T1", None, 60).expect("verify should run, not error")
    }
}

#[test]
#[serial]
fn reverify_after_pass_is_allowed() {
    let fx = Fixture::new();
    set_switch(&fx.switch, "pass");

    assert!(fx.verify().is_success());
    assert_eq!(fx.state(), State::Verified);

    // Second green run used to fail with `InvalidTransition: Verified →
    // Verified` because `advance` only accepts the next state in the flow.
    let second = fx.verify();
    assert!(
        second.is_success(),
        "re-running a green suite must succeed, got {second:?}"
    );
    assert_eq!(fx.state(), State::Verified);

    let ts = TaskState::load(&fx.tasks_dir(), "T1").unwrap();
    assert!(
        ts.history.iter().any(|e| e.note.contains("re-verified")),
        "the repeat run must leave a trace in history"
    );
}

#[test]
#[serial]
fn failing_reverify_withdraws_the_earlier_pass() {
    let fx = Fixture::new();

    set_switch(&fx.switch, "pass");
    assert!(fx.verify().is_success());
    assert_eq!(fx.state(), State::Verified);

    set_switch(&fx.switch, "fail");
    let outcome = fx.verify();
    assert!(
        !outcome.is_success(),
        "a red re-run must not report success, got {outcome:?}"
    );
    assert_eq!(
        fx.state(),
        State::Coded,
        "Verified must be withdrawn once the suite that justified it fails"
    );

    let ts = TaskState::load(&fx.tasks_dir(), "T1").unwrap();
    assert!(
        ts.history.iter().any(|e| e.state == State::Verified),
        "history must still record that the task once reached Verified"
    );
}

#[test]
#[serial]
fn review_is_refused_after_a_failing_reverify() {
    let fx = Fixture::new();

    set_switch(&fx.switch, "pass");
    assert!(fx.verify().is_success());

    set_switch(&fx.switch, "fail");
    assert!(!fx.verify().is_success());

    // `review --done` used to see a stale `Verified` and promote the task to
    // `Reviewed` over a red suite.
    let err = zforge::cli::review::run("T1", true)
        .expect_err("review --done must be refused after a failing re-verify");
    let msg = err.to_string();
    assert!(
        msg.contains("Coded") || msg.contains("Verified") || msg.contains("passed: false"),
        "error should explain the missing verification, got: {msg}"
    );

    assert_eq!(fx.state(), State::Coded, "review must not advance the FSM");
}

#[test]
#[serial]
fn passing_again_after_a_failure_restores_verified() {
    let fx = Fixture::new();

    set_switch(&fx.switch, "pass");
    assert!(fx.verify().is_success());

    set_switch(&fx.switch, "fail");
    assert!(!fx.verify().is_success());
    assert_eq!(fx.state(), State::Coded);

    set_switch(&fx.switch, "pass");
    assert!(fx.verify().is_success());
    assert_eq!(
        fx.state(),
        State::Verified,
        "a fixed suite must be able to reach Verified again"
    );
}

/// Reviewed sits on top of Verified, so a failing re-run invalidates both.
#[test]
#[serial]
fn failing_reverify_reaches_through_reviewed() {
    let fx = Fixture::new();

    set_switch(&fx.switch, "pass");
    assert!(fx.verify().is_success());

    // Promote to Reviewed the way `review --done` does.
    let mut ts = TaskState::load(&fx.tasks_dir(), "T1").unwrap();
    ts.advance(State::Reviewed, "review complete").unwrap();
    ts.save(&fx.tasks_dir()).unwrap();
    assert_eq!(fx.state(), State::Reviewed);

    set_switch(&fx.switch, "fail");
    assert!(!fx.verify().is_success());
    assert_eq!(
        fx.state(),
        State::Coded,
        "a review resting on a withdrawn pass is withdrawn with it"
    );
}

/// A repeat pass gives no reason to withdraw a completed review.
#[test]
#[serial]
fn passing_reverify_leaves_reviewed_intact() {
    let fx = Fixture::new();

    set_switch(&fx.switch, "pass");
    assert!(fx.verify().is_success());

    let mut ts = TaskState::load(&fx.tasks_dir(), "T1").unwrap();
    ts.advance(State::Reviewed, "review complete").unwrap();
    ts.save(&fx.tasks_dir()).unwrap();

    assert!(fx.verify().is_success());
    assert_eq!(fx.state(), State::Reviewed);
}
