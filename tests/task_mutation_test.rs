//! Task-state mutation safety: FIX-004 (lock coverage), FIX-005 (flow-aware
//! retry), FIX-007 (ship must not overwrite the orchestrator's fallback
//! record).
//!
//! The concurrency cases drive the real binary in separate processes. The
//! bug was two *processes* interleaving writes to `.state.yaml`, and an
//! in-process simulation would not exercise the same `flock` path.

mod common;

use common::{make_project, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use zforge::registry::{
    io,
    schema::{AgentSpec, FallbackPolicy, Registry},
};
use zforge::state::{try_acquire_task_lock, Flow, State, TaskState};

// ─── fixtures ────────────────────────────────────────────────────────────────

fn zforge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_zforge")
}

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

fn write_config(root: &Path, test_command: &str) {
    std::fs::write(
        root.join(".zforge/config.yaml"),
        format!(
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
        ),
    )
    .unwrap();
}

const ALL_ARTIFACTS: &[&str] = &[
    "task.md",
    "spec.md",
    "testspec.md",
    "plan.md",
    "verify.md",
    "review-summary.md",
];

/// Project with task T1 in `flow`, walked forward to `state`, every artifact
/// present with non-empty content.
fn scaffold(root: &Path, test_command: &str, flow: Flow, state: State) -> PathBuf {
    make_project(root);
    let agents = root.join(".zforge/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("code.tmpl"), "Implement {{task_id}}\n").unwrap();
    std::fs::write(agents.join("verify-analysis.tmpl"), "Analyze {{task_id}}\n").unwrap();
    write_config(root, test_command);

    let tasks_dir = root.join(".zforge/tasks");
    std::fs::create_dir_all(tasks_dir.join("T1")).unwrap();
    for name in ALL_ARTIFACTS {
        std::fs::write(
            tasks_dir.join("T1").join(name),
            format!("---\npassed: true\n---\n# {name}\nstub\n"),
        )
        .unwrap();
    }

    let mut ts = TaskState::new_with_flow("T1", flow);
    while ts.state < state {
        let next = ts.flow.next_after(&ts.state).cloned().unwrap();
        ts.advance(next, "fixture").unwrap();
    }
    ts.save(&tasks_dir).unwrap();
    tasks_dir
}

fn cli(project: &Path, home: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(zforge_bin());
    c.args(args)
        .current_dir(project)
        .env("ZFORGE_HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

fn run_cli(project: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    cli(project, home, args).output().expect("run zforge")
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Poll until another process holds T1's lock (i.e. `ship` has started).
fn wait_until_locked(tasks_dir: &Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if try_acquire_task_lock(tasks_dir, "T1").is_err() {
            return;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("ship exited ({status}) before it was observed holding the lock");
        }
        assert!(Instant::now() < deadline, "ship never took the task lock");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn state_of(tasks_dir: &Path) -> State {
    TaskState::load(tasks_dir, "T1").unwrap().state
}

fn existing_artifacts(tasks_dir: &Path) -> Vec<&'static str> {
    ALL_ARTIFACTS
        .iter()
        .copied()
        .filter(|a| tasks_dir.join("T1").join(a).exists())
        .collect()
}

// ─── FIX-004: every mutation respects the task lock ──────────────────────────

/// The backlog repro: `ship` holds the lock while slow tests run; `retry`
/// used to delete artifacts and reset the task underneath it, after which
/// `ship` saved its stale snapshot back as Verified.
#[test]
fn retry_is_refused_while_ship_holds_the_task() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(
        project.path(),
        "sh -c 'sleep 2; exit 0'",
        Flow::Full,
        State::Coded,
    );

    let mut ship = cli(project.path(), home.path(), &["ship", "T1"])
        .spawn()
        .expect("spawn ship");
    wait_until_locked(&tasks_dir, &mut ship);

    let retry = run_cli(
        project.path(),
        home.path(),
        &["retry", "T1", "--from", "spec", "--yes"],
    );
    assert_ne!(
        retry.status.code(),
        Some(0),
        "retry must not succeed mid-ship"
    );
    assert!(
        stderr(&retry).contains("locked"),
        "retry should report the lock, got: {}",
        stderr(&retry)
    );
    assert!(
        tasks_dir.join("T1/spec.md").exists(),
        "a refused retry must not delete artifacts"
    );

    let ship_out = ship.wait_with_output().unwrap();
    assert_eq!(
        ship_out.status.code(),
        Some(0),
        "ship should finish normally: {}",
        String::from_utf8_lossy(&ship_out.stderr)
    );
    assert_eq!(state_of(&tasks_dir), State::Verified);
    assert!(tasks_dir.join("T1/spec.md").exists());
}

/// `verify`, `review --done` and `approve` all rewrite `.state.yaml`; each
/// must refuse to run while another process owns the task.
#[test]
fn state_writing_commands_are_refused_while_the_task_is_locked() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(
        project.path(),
        "sh -c 'exit 0'",
        Flow::Full,
        State::Verified,
    );
    std::fs::remove_file(tasks_dir.join("T1/verify.md")).unwrap();

    // Any holder in another open file description blocks — flock is not
    // re-entrant — so holding it from the test process stands in for a
    // concurrent `ship`.
    let _holder = try_acquire_task_lock(&tasks_dir, "T1").unwrap();

    for args in [
        vec!["verify", "T1"],
        vec!["review", "T1", "--done"],
        vec!["approve", "T1", "plan", "--yes"],
        vec!["retry", "T1", "--from", "review", "--yes"],
        vec!["ship", "T1"],
    ] {
        let out = run_cli(project.path(), home.path(), &args);
        assert_ne!(out.status.code(), Some(0), "{args:?} must not succeed");
        assert!(
            stderr(&out).contains("locked"),
            "{args:?} should report the lock, got: {}",
            stderr(&out)
        );
    }

    assert_eq!(
        state_of(&tasks_dir),
        State::Verified,
        "no state was changed"
    );
    assert!(
        !tasks_dir.join("T1/verify.md").exists(),
        "verify must not have run the suite or written a report"
    );
}

/// A refused command releases nothing it did not own: once the holder is
/// gone the same command succeeds.
#[test]
fn commands_proceed_once_the_lock_is_released() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(project.path(), "sh -c 'exit 0'", Flow::Full, State::Coded);

    {
        let _holder = try_acquire_task_lock(&tasks_dir, "T1").unwrap();
        let out = run_cli(project.path(), home.path(), &["verify", "T1"]);
        assert_ne!(out.status.code(), Some(0));
    }

    let out = run_cli(project.path(), home.path(), &["verify", "T1"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(state_of(&tasks_dir), State::Verified);
}

// ─── FIX-005: retry rewinds within the task's flow ───────────────────────────

#[test]
fn fixbug_retry_from_code_stays_inside_the_flow() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(project.path(), "sh -c 'exit 0'", Flow::Fixbug, State::Coded);

    let out = run_cli(
        project.path(),
        home.path(),
        &["retry", "T1", "--from", "code", "--yes"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let ts = TaskState::load(&tasks_dir, "T1").unwrap();
    assert_eq!(
        ts.state,
        State::TestspecDone,
        "fixbug's predecessor of Coded is TestspecDone, not the full-flow PlanReviewed"
    );
    assert!(Flow::Fixbug.contains(&ts.state));
}

/// Retrying a phase the task has not reached would "rewind" it forward.
/// It must be refused with state and artifacts left exactly as they were.
#[test]
fn retry_ahead_of_the_task_is_refused_and_changes_nothing() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(
        project.path(),
        "sh -c 'exit 0'",
        Flow::Full,
        State::Imported,
    );
    let before_artifacts = existing_artifacts(&tasks_dir);
    let before_state = std::fs::read_to_string(tasks_dir.join("T1/.state.yaml")).unwrap();

    let out = run_cli(
        project.path(),
        home.path(),
        &["retry", "T1", "--from", "review", "--yes"],
    );
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("forward"), "{}", stderr(&out));

    assert_eq!(state_of(&tasks_dir), State::Imported, "no forward jump");
    assert_eq!(existing_artifacts(&tasks_dir), before_artifacts);
    assert_eq!(
        std::fs::read_to_string(tasks_dir.join("T1/.state.yaml")).unwrap(),
        before_state,
        "state file untouched"
    );
    assert!(
        !tasks_dir.join("T1/.history").exists(),
        "a refused retry must not create a backup either"
    );
}

#[test]
fn retry_of_a_phase_the_flow_lacks_is_refused() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(project.path(), "sh -c 'exit 0'", Flow::Docs, State::Coded);
    let before = existing_artifacts(&tasks_dir);

    let out = run_cli(
        project.path(),
        home.path(),
        &["retry", "T1", "--from", "verify", "--yes"],
    );
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("docs"), "{}", stderr(&out));
    assert_eq!(state_of(&tasks_dir), State::Coded);
    assert_eq!(existing_artifacts(&tasks_dir), before);
}

// ─── FIX-007: ship keeps the orchestrator's fallback record ─────────────────

fn fake_agent_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/examples/fake_agent")
}

fn ensure_fake_agent_built() {
    if fake_agent_path().exists() {
        return;
    }
    let status = Command::new("cargo")
        .args(["build", "--example", "fake_agent"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("cargo build --example fake_agent");
    assert!(status.success());
}

fn fake_agent(cfg_dir: &Path, name: &str, exit_code: i32) -> AgentSpec {
    let cfg = cfg_dir.join(format!("{name}.json"));
    std::fs::write(&cfg, json!({ "exit_code": exit_code }).to_string()).unwrap();
    AgentSpec {
        command: fake_agent_path().to_string_lossy().into_owned(),
        args: vec![cfg.to_string_lossy().into_owned()],
    }
}

/// Backlog repro: primary exits 124 (timeout → retryable), fallback exits 0.
/// The orchestrator recorded the swap, then `ship` saved the snapshot it had
/// loaded before dispatch — putting `active_agent` back to the primary and
/// erasing `fallback_history`, although the cost log showed both spawns.
#[test]
#[serial]
fn ship_preserves_the_fallback_swap_recorded_during_dispatch() {
    ensure_fake_agent_built();
    let _home = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();

    let mut registry = Registry {
        fallback_policy: FallbackPolicy {
            max_retries: 1,
            cooldown_seconds: 0,
            ..FallbackPolicy::default()
        },
        ..Default::default()
    };
    registry
        .agents
        .insert("primary".into(), fake_agent(cfg_dir.path(), "primary", 124));
    registry.agents.insert(
        "secondary".into(),
        fake_agent(cfg_dir.path(), "secondary", 0),
    );
    io::save_atomic(&registry).unwrap();

    let project = tempfile::tempdir().unwrap();
    let tasks_dir = scaffold(
        project.path(),
        "sh -c 'exit 0'",
        Flow::Full,
        State::PlanReviewed,
    );
    let mut ts = TaskState::load(&tasks_dir, "T1").unwrap();
    ts.assigned_agent = Some("primary".into());
    ts.active_agent = Some("primary".into());
    ts.fallback_agent = Some("secondary".into());
    ts.save(&tasks_dir).unwrap();

    let _cwd = CwdGuard::enter(project.path());
    let outcome = zforge::cli::ship::run("T1", None, 60, 1).expect("ship runs");
    assert!(outcome.is_success(), "{outcome:?}");

    let after = TaskState::load(&tasks_dir, "T1").unwrap();
    assert_eq!(after.state, State::Verified);
    assert_eq!(
        after.active_agent.as_deref(),
        Some("secondary"),
        "the agent that actually did the work must stay recorded"
    );
    assert_eq!(
        after.fallback_history.len(),
        1,
        "the primary → secondary swap must survive ship's own save"
    );
    assert_eq!(after.fallback_history[0].from, "primary");
    assert_eq!(after.fallback_history[0].to, "secondary");
    assert_eq!(
        after.assigned_agent.as_deref(),
        Some("primary"),
        "assigned_agent is immutable"
    );
}
