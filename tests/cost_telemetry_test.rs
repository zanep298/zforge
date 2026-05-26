//! End-to-end test: orchestrator records a CostEntry per spawn.

mod common;

use common::{make_project, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use zforge::cost::log::{load_all, log_path};
use zforge::orchestrator;
use zforge::registry::{
    io,
    schema::{AgentSpec, FallbackPolicy, Registry},
};
use zforge::state::TaskState;

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

fn make_task(project_root: &Path, task_id: &str, primary: &str, fallback: Option<&str>) {
    std::fs::create_dir_all(project_root.join(".zforge/tasks").join(task_id)).unwrap();
    let mut ts = TaskState::new(task_id);
    ts.assigned_agent = Some(primary.into());
    ts.active_agent = Some(primary.into());
    ts.fallback_agent = fallback.map(str::to_string);
    ts.save(&project_root.join(".zforge/tasks")).unwrap();
}

fn seed_registry(primary_args: &[&str], fallback_args: Option<&[&str]>) {
    let mut r = Registry {
        fallback_policy: FallbackPolicy {
            max_retries: 2,
            cooldown_seconds: 0,
            ..FallbackPolicy::default()
        },
        ..Default::default()
    };
    r.agents.insert(
        "primary".into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: primary_args.iter().map(|s| s.to_string()).collect(),
        },
    );
    if let Some(fb) = fallback_args {
        r.agents.insert(
            "fallback".into(),
            AgentSpec {
                command: fake_agent_path().to_string_lossy().into_owned(),
                args: fb.iter().map(|s| s.to_string()).collect(),
            },
        );
    }
    io::save_atomic(&r).unwrap();
}

#[test]
#[serial]
fn successful_spawn_writes_one_cost_entry() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry(&[primary_cfg.to_str().unwrap()], None);

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", None);

    orchestrator::run_phase("T1", "plan", proj.path(), "test prompt").unwrap();

    let entries = load_all(proj.path()).unwrap();
    assert_eq!(
        entries.len(),
        1,
        "exactly one cost entry per successful spawn"
    );
    let e = &entries[0];
    assert_eq!(e.task_id, "T1");
    assert_eq!(e.phase, "plan");
    assert_eq!(e.agent, "primary");
    assert_eq!(e.exit_code, 0);
    assert!(!e.timed_out);
    assert!(e.prompt_chars > 0);
    assert!(e.est_input_tokens > 0);
}

#[test]
#[serial]
fn fallback_records_two_entries_one_per_attempt() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    let primary_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 124}));
    let fallback_cfg = write_fake_config(cfg_dir.path(), "fallback", &json!({"exit_code": 0}));
    seed_registry(
        &[primary_cfg.to_str().unwrap()],
        Some(&[fallback_cfg.to_str().unwrap()]),
    );

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", Some("fallback"));

    orchestrator::run_phase("T1", "code", proj.path(), "p").unwrap();

    let entries = load_all(proj.path()).unwrap();
    assert_eq!(
        entries.len(),
        2,
        "one entry for failed primary + one for successful fallback"
    );
    assert_eq!(entries[0].agent, "primary");
    assert_eq!(entries[0].exit_code, 124);
    assert_eq!(entries[1].agent, "fallback");
    assert_eq!(entries[1].exit_code, 0);
}

#[test]
#[serial]
fn cost_log_path_is_under_project_dot_zforge() {
    let tmp = tempfile::tempdir().unwrap();
    let p = log_path(tmp.path());
    assert!(p.ends_with(".zforge/cost-log.jsonl"));
}

#[test]
#[serial]
fn codex_tokens_used_line_parsed_into_reported_total() {
    ensure_fake_agent_built();
    let _h = TestHome::new();
    let cfg_dir = tempfile::tempdir().unwrap();
    // Simulate codex's `tokens used N` line in stdout.
    let primary_cfg = write_fake_config(
        cfg_dir.path(),
        "primary",
        &json!({
            "exit_code": 0,
            "stdout": "doing work\ntokens used 12345\nfinal"
        }),
    );
    seed_registry(&[primary_cfg.to_str().unwrap()], None);

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", None);

    orchestrator::run_phase("T1", "plan", proj.path(), "p").unwrap();

    let entries = load_all(proj.path()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].reported_total_tokens,
        Some(12345),
        "should parse `tokens used N` from stdout"
    );
}
