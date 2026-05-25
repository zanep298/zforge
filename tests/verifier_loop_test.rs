//! End-to-end test of the verifier-driven `ship --max-iterations` loop.
//!
//! Uses the `fake_agent` example as the code agent (exits 0, does nothing) and
//! a tiny shell `test_command` whose pass/fail flips after the first call via
//! a counter file. Validates that:
//!   1. The loop runs more than once on initial failure.
//!   2. It stops as soon as verify passes.
//!   3. `assigned_agent` stays untouched after retries.

mod common;

use common::{make_project, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use zforge::registry::{
    io,
    schema::{AgentSpec, FallbackPolicy, Registry},
};
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

fn fake_agent_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("examples")
        .join("fake_agent")
}

fn ensure_fake_agent_built() {
    if fake_agent_path().exists() {
        return;
    }
    let status = std::process::Command::new("cargo")
        .args(["build", "--example", "fake_agent"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("cargo build --example fake_agent");
    assert!(status.success());
}

fn write_fake_config(dir: &Path, name: &str, cfg: &serde_json::Value) -> PathBuf {
    let p = dir.join(format!("{name}.json"));
    std::fs::write(&p, serde_json::to_string(cfg).unwrap()).unwrap();
    p
}

/// Write a shell script that increments `counter_path` on each call and
/// exits 0 once the counter reaches `pass_at`. Returns a `test_command`
/// string (`bash <script>`) safe to embed in YAML — no nested quotes.
fn write_counter_script(script_dir: &Path, counter_path: &Path, pass_at: u32) -> String {
    let script_path = script_dir.join("test.sh");
    let body = format!(
        "#!/bin/bash\n\
         set -e\n\
         c=$(cat '{path}' 2>/dev/null || echo 0)\n\
         c=$((c+1))\n\
         echo $c > '{path}'\n\
         [ $c -ge {pass_at} ]\n",
        path = counter_path.display(),
        pass_at = pass_at,
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

/// Scaffold `.zforge/config.yaml` with the given test command + the `shell`
/// language so the runner falls into the generic parser (exit code drives
/// pass/fail — no rust/go test format parsing).
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

fn write_min_agents_dir(project: &Path) {
    let dir = project.join(".zforge/agents");
    std::fs::create_dir_all(&dir).unwrap();
    // Inline a minimal code template so render() succeeds without a global
    // store. Include the `{{if failed_tests}}` block so feedback rendering
    // is exercised end-to-end.
    let tmpl = r#"You are implementing a {{language}} feature for {{task_id}}.

{{if failed_tests}}
## RETRY — Previous verifier failed

Failed tests:
{{failed_tests}}

{{end}}
Run: {{test_command}}
"#;
    std::fs::write(dir.join("code.tmpl"), tmpl).unwrap();
}

fn seed_registry_with_fake_agent(extra_args: &[&str]) {
    let mut r = Registry {
        fallback_policy: FallbackPolicy {
            max_retries: 0, // disable fallback — we only test verifier loop
            cooldown_seconds: 0,
            ..FallbackPolicy::default()
        },
        ..Default::default()
    };
    r.agents.insert(
        "primary".into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: extra_args.iter().map(|s| s.to_string()).collect(),
        },
    );
    io::save_atomic(&r).unwrap();
}

/// Advance a fresh task all the way to PlanReviewed so `ship` can run.
fn make_task_ready_to_ship(project: &Path, task_id: &str) {
    let tasks_dir = project.join(".zforge/tasks");
    std::fs::create_dir_all(tasks_dir.join(task_id)).unwrap();

    // Minimal artifact files — content doesn't matter, just presence for
    // `has_content_ref` checks in build_context_for_phase.
    for name in ["task.md", "spec.md", "testspec.md", "plan.md"] {
        std::fs::write(
            tasks_dir.join(task_id).join(name),
            format!("# {name}\nstub\n"),
        )
        .unwrap();
    }

    let mut ts = TaskState::new_with_flow(task_id, Flow::Full);
    ts.assigned_agent = Some("primary".into());
    ts.active_agent = Some("primary".into());
    // Walk the FSM through every approval gate up to PlanReviewed.
    for next in [
        State::SpecDone,
        State::TestspecDone,
        State::TestspecReviewed,
        State::Planned,
        State::PlanReviewed,
    ] {
        ts.advance(next, "test").unwrap();
    }
    ts.save(&tasks_dir).unwrap();
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
#[serial]
fn ship_succeeds_immediately_when_tests_already_pass() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());

    // test_command always passes.
    write_config(proj.path(), "sh -c 'true'");

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    zforge::cli::ship::run("T1", None, 60, 3).unwrap();

    let ts = TaskState::load(&proj.path().join(".zforge/tasks"), "T1").unwrap();
    assert_eq!(ts.state, State::Verified);
    assert_eq!(
        ts.assigned_agent.as_deref(),
        Some("primary"),
        "assigned_agent must not move"
    );
}

#[test]
#[serial]
fn ship_loops_until_tests_pass() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());

    let counter_dir = tempfile::tempdir().unwrap();
    let counter_path = counter_dir.path().join("counter");
    // Test command flips to PASS on its 2nd invocation.
    let cmd = write_counter_script(counter_dir.path(), &counter_path, 2);
    write_config(proj.path(), &cmd);

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    zforge::cli::ship::run("T1", None, 60, 3).unwrap();

    let counter: u32 = std::fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(counter, 2, "verifier should have run exactly twice");

    let ts = TaskState::load(&proj.path().join(".zforge/tasks"), "T1").unwrap();
    assert_eq!(ts.state, State::Verified);
}

#[test]
#[serial]
fn ship_exhausts_budget_when_tests_never_pass() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());

    let counter_dir = tempfile::tempdir().unwrap();
    let counter_path = counter_dir.path().join("counter");
    // Set pass_at unreachably high so test never passes within the budget.
    let cmd = write_counter_script(counter_dir.path(), &counter_path, 999);
    write_config(proj.path(), &cmd);

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    let err = zforge::cli::ship::run("T1", None, 60, 2).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("budget exhausted"), "got: {msg}");

    let counter: u32 = std::fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(counter, 2, "verifier should have run exactly max_iterations times");
}

#[test]
#[serial]
fn single_shot_ship_propagates_verify_failure() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());

    write_config(proj.path(), "sh -c 'false'");

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    // max_iterations = 1 → single shot, no loop. verify::run returns Ok
    // (it doesn't error on test failure; it writes verify.md and reports).
    // Behavior here matches pre-PR4 ship.
    let result = zforge::cli::ship::run("T1", None, 60, 1);
    assert!(result.is_ok(), "single-shot ship returns Ok even on verify fail (legacy contract)");

    let ts = TaskState::load(&proj.path().join(".zforge/tasks"), "T1").unwrap();
    assert_eq!(ts.state, State::Coded, "state stays at Coded when verify fails");
}
