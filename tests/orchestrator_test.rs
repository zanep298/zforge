mod common;

use common::{make_project, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use zforge::orchestrator;
use zforge::registry::{
    io,
    schema::{AgentSpec, FallbackPolicy, ProjectEntry, RegisteredBy, Registry},
};
use zforge::state::TaskState;

// ─── helpers ──────────────────────────────────────────────────────────────────

fn fake_agent_path() -> PathBuf {
    // `cargo test` builds examples into target/<profile>/examples/.
    // `env!("CARGO_MANIFEST_DIR")` gives a stable anchor regardless of cwd.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("examples")
        .join("fake_agent")
}

fn write_fake_config(dir: &Path, name: &str, cfg: &serde_json::Value) -> PathBuf {
    let p = dir.join(format!("{name}.json"));
    std::fs::write(&p, serde_json::to_string(cfg).unwrap()).unwrap();
    p
}

fn agent_spec(extra_args: &[&str]) -> AgentSpec {
    AgentSpec {
        command: fake_agent_path().to_string_lossy().into_owned(),
        args: extra_args.iter().map(|s| s.to_string()).collect(),
    }
}

fn seed_registry(
    primary_args: &[&str],
    fallback_args: Option<&[&str]>,
    policy: FallbackPolicy,
) {
    let mut r = Registry {
        fallback_policy: policy,
        ..Default::default()
    };
    r.agents.insert("primary".into(), agent_spec(primary_args));
    if let Some(fb) = fallback_args {
        r.agents.insert("fallback".into(), agent_spec(fb));
    }
    io::save_atomic(&r).unwrap();
}

fn make_task(project_root: &Path, task_id: &str, primary: &str, fallback: Option<&str>) {
    std::fs::create_dir_all(project_root.join(".zforge/tasks").join(task_id)).unwrap();
    let mut ts = TaskState::new(task_id);
    ts.assigned_agent = Some(primary.into());
    ts.active_agent = Some(primary.into());
    ts.fallback_agent = fallback.map(str::to_string);
    ts.save(&project_root.join(".zforge/tasks")).unwrap();
}

fn load_state(project_root: &Path, task_id: &str) -> TaskState {
    TaskState::load(&project_root.join(".zforge/tasks"), task_id).unwrap()
}

fn ensure_fake_agent_built() {
    if fake_agent_path().exists() {
        return;
    }
    // First test invocation in a fresh checkout — compile the example.
    let status = std::process::Command::new("cargo")
        .args(["build", "--example", "fake_agent"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("cargo build --example fake_agent");
    assert!(status.success(), "failed to build fake_agent example");
}

fn fast_policy(retries: u32) -> FallbackPolicy {
    FallbackPolicy {
        max_retries: retries,
        cooldown_seconds: 0, // keep tests fast
        ..FallbackPolicy::default()
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
#[serial]
fn success_first_attempt() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry(&[primary_cfg.to_str().unwrap()], None, fast_policy(2));

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", None);

    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();

    let ts = load_state(proj.path(), "T1");
    assert!(ts.fallback_history.is_empty());
    assert_eq!(ts.active_agent.as_deref(), Some("primary"));
}

#[test]
#[serial]
fn fallback_on_retryable_exit_124() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 124}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(2),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();

    let ts = load_state(proj.path(), "T1");
    assert_eq!(ts.fallback_history.len(), 1);
    assert_eq!(ts.fallback_history[0].from, "primary");
    assert_eq!(ts.fallback_history[0].to, "fallback");
    assert_eq!(ts.fallback_history[0].reason, "exit_code:124");
    assert_eq!(ts.fallback_history[0].phase, "code");
    assert_eq!(ts.active_agent.as_deref(), Some("fallback"));
    assert_eq!(
        ts.assigned_agent.as_deref(),
        Some("primary"),
        "assigned_agent must be immutable"
    );
}

#[test]
#[serial]
fn fallback_on_stderr_rate_limit() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(
        cfg_dir.path(),
        "primary",
        &json!({"exit_code": 1, "stderr": "rate limit exceeded"}),
    );
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(2),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();

    let ts = load_state(proj.path(), "T1");
    assert_eq!(ts.fallback_history.len(), 1);
    // PR9 renamed `stderr_match:` → `output_match:` because the orchestrator
    // now scans combined stdout+stderr.
    assert!(ts.fallback_history[0].reason.starts_with("output_match:"));
    assert_eq!(ts.active_agent.as_deref(), Some("fallback"));
}

/// PR9: codex prints `ERROR: {"status":429,...}` to STDOUT and exits 0.
/// Verify the orchestrator detects the failure via stdout scan and fires
/// the fallback even though exit code says success.
#[test]
#[serial]
fn fallback_on_codex_stdout_429_with_exit_zero() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(
        cfg_dir.path(),
        "primary",
        &json!({
            "exit_code": 0,
            "stdout": r#"ERROR: {"type":"error","status":429,"error":{"type":"rate_limit_error","message":"rate limited"}}"#
        }),
    );
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(2),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();

    let ts = load_state(proj.path(), "T1");
    assert_eq!(
        ts.fallback_history.len(),
        1,
        "codex-style exit-0-but-stdout-error must fire fallback"
    );
    assert!(
        ts.fallback_history[0]
            .reason
            .starts_with("output_match:"),
        "reason: {}",
        ts.fallback_history[0].reason
    );
    assert_eq!(ts.active_agent.as_deref(), Some("fallback"));
}

/// PR9: claude prints model-not-found errors to STDOUT and exits 1. That
/// is a user/config error — must NOT trigger fallback. Lock the boundary.
#[test]
#[serial]
fn no_fallback_on_claude_invalid_model_stdout() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(
        cfg_dir.path(),
        "primary",
        &json!({
            "exit_code": 1,
            "stdout": "There's an issue with the selected model (X). It may not exist or you may not have access to it."
        }),
    );
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(2),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    let err = orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap_err();
    assert!(
        err.to_string().contains("exit=1"),
        "expected non-retryable bail; got: {err}"
    );

    let ts = load_state(proj.path(), "T1");
    assert!(
        ts.fallback_history.is_empty(),
        "invalid-model is a user error — must NOT swap to fallback"
    );
}

#[test]
#[serial]
fn no_fallback_on_test_failure_exit_1() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 1}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(2),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    let err = orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exit=1"), "expected exit=1 in error: {msg}");

    let ts = load_state(proj.path(), "T1");
    assert!(ts.fallback_history.is_empty());
    assert_eq!(ts.active_agent.as_deref(), Some("primary"));
}

#[test]
#[serial]
fn max_retries_terminal_failure() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 124}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 124}));
    // Cap at 1 retry: primary fails → record one fallback → fallback fails →
    // budget exhausted → error.
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(1),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    let err = orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap_err();
    assert!(
        err.to_string().contains("budget exhausted"),
        "expected budget exhausted: {err}"
    );

    let ts = load_state(proj.path(), "T1");
    assert_eq!(ts.fallback_history.len(), 1);
    assert_eq!(ts.active_agent.as_deref(), Some("fallback"));
}

#[test]
#[serial]
fn cooldown_respected() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 124}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    let policy = FallbackPolicy {
        max_retries: 2,
        cooldown_seconds: 1, // 1s — measurable but keeps test snappy
        ..FallbackPolicy::default()
    };
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        policy,
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    let started = std::time::Instant::now();
    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();
    let elapsed = started.elapsed();

    assert!(
        elapsed >= std::time::Duration::from_millis(900),
        "cooldown not respected: elapsed={elapsed:?}"
    );
}

#[test]
#[serial]
fn flow_preserved_after_fallback() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 124}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(2),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));
    let flow_before = load_state(proj.path(), "T1").flow.as_str().to_string();

    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();

    let flow_after = load_state(proj.path(), "T1").flow.as_str().to_string();
    assert_eq!(flow_before, flow_after);
}

#[test]
#[serial]
fn non_existent_binary_clean_error() {
    let _h = TestHome::new();
    let mut r = Registry {
        fallback_policy: fast_policy(2),
        ..Default::default()
    };
    r.agents.insert(
        "ghost".into(),
        AgentSpec {
            command: "/definitely/does/not/exist/zforge_ghost".into(),
            args: vec![],
        },
    );
    io::save_atomic(&r).unwrap();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "ghost", None);

    let err = orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("spawn"), "expected spawn error: {msg}");
}

#[test]
#[serial]
fn per_project_agent_override() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let global_cfg = write_fake_config(cfg_dir.path(), "global", &json!({"exit_code": 1}));
    let override_cfg = write_fake_config(cfg_dir.path(), "override", &json!({"exit_code": 0}));

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    let canon = std::fs::canonicalize(proj.path()).unwrap();

    // Build registry with a global `primary` (would fail) AND a project-level
    // override (succeeds). Orchestrator must pick the override.
    let mut r = Registry {
        fallback_policy: fast_policy(2),
        ..Default::default()
    };
    r.agents.insert(
        "primary".into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: vec![global_cfg.to_str().unwrap().into()],
        },
    );
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "primary".into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: vec![override_cfg.to_str().unwrap().into()],
        },
    );
    r.projects.push(ProjectEntry {
        name: "proj".into(),
        path: canon,
        registered_at: chrono::Utc::now(),
        registered_by: RegisteredBy::Manual,
        agent_overrides: overrides,
    });
    io::save_atomic(&r).unwrap();

    make_task(proj.path(), "T1", "primary", None);

    // If the override worked, exit_code is 0 (override). If not, exit_code is 1
    // (global) and it would propagate as a non-retryable error.
    orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap();
}

#[test]
#[serial]
fn legacy_task_without_assigned_agent_errors_in_orchestrator() {
    // Verify the orchestrator itself rejects agentless tasks. The CLI helper
    // (`dispatch_helper::run_phase_for_task`) routes such tasks to the legacy
    // dispatch path BEFORE calling the orchestrator — that path is covered
    // implicitly by every pre-PR2 CLI test.
    let _h = TestHome::new();
    seed_registry(&[], None, fast_policy(2));

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    std::fs::create_dir_all(proj.path().join(".zforge/tasks/T1")).unwrap();
    let ts = TaskState::new("T1");
    ts.save(&proj.path().join(".zforge/tasks")).unwrap();

    let err = orchestrator::run_phase("T1", "code", proj.path(), "prompt").unwrap_err();
    assert!(err.to_string().contains("no agent assigned"));
}

#[test]
#[serial]
fn missing_state_file_clean_error() {
    let _h = TestHome::new();
    seed_registry(&[], None, fast_policy(2));

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    let err = orchestrator::run_phase("T-MISSING", "code", proj.path(), "prompt").unwrap_err();
    assert!(err.to_string().contains("read"));
}

#[test]
#[serial]
fn fallback_persists_state_between_attempts() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    // Primary fails retryably; fallback also fails retryably to force the
    // orchestrator to write `.state.yaml` and then terminate.
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 124}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 124}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
        fast_policy(1),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    let _ = orchestrator::run_phase("T1", "code", proj.path(), "prompt");

    // State file should reflect the swap even though run_phase failed at the
    // end. This is the persistence guarantee documented in PR3-05.
    let reloaded = load_state(proj.path(), "T1");
    assert_eq!(reloaded.active_agent.as_deref(), Some("fallback"));
    assert_eq!(reloaded.fallback_history.len(), 1);
    assert_eq!(
        reloaded.assigned_agent.as_deref(),
        Some("primary"),
        "assigned_agent must not move during persistence"
    );
}
