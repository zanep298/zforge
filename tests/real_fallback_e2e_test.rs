//! Real-binary end-to-end test of the fallback swap.
//!
//! The orchestrator's PR3 fallback logic is unit-tested with `fake_agent`.
//! PR8 real-binary tests verify CONTRACT (claude/codex stdin, exit codes,
//! output channels). This test bridges the two: a **real** agent runs as
//! the fallback target, picking up where a simulated primary failure leaves
//! off.
//!
//! Hybrid design rationale:
//!   - Inducing a true rate-limit / quota error on demand is unreliable
//!     and burns API quota.
//!   - The primary is `fake_agent` configured to emit a codex-style
//!     `"status":429` JSON in stderr — the orchestrator's PR9 patterns
//!     match this and fire fallback.
//!   - The fallback is the REAL claude binary, which must actually run,
//!     receive the prompt, and exit 0.
//!
//! Verifies the wire end-to-end: registry lookup → agent swap →
//! `Registry::resolved_agent` → real `spawn_agent` → real exit code →
//! state file updated with the new `active_agent`.
//!
//! Gated by `ZFORGE_TEST_REAL_FALLBACK=1` because it burns ~$0.005 of real
//! API quota.

mod common;

use common::{make_project, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use zforge::orchestrator;
use zforge::registry::{
    io,
    schema::{AgentSpec, FallbackPolicy, Registry},
};
use zforge::state::{Flow, State, TaskState};

// ─── gate ─────────────────────────────────────────────────────────────────────

fn fallback_e2e_enabled() -> Option<()> {
    if std::env::var("ZFORGE_TEST_REAL_FALLBACK").is_err() {
        eprintln!(
            "[real_fallback_e2e_test] SKIP: set ZFORGE_TEST_REAL_FALLBACK=1 \
             to run (burns ~$0.005 API quota)"
        );
        return None;
    }
    if which::which("claude").is_err() {
        eprintln!("[real_fallback_e2e_test] SKIP: `claude` not on $PATH");
        return None;
    }
    Some(())
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

fn make_task(project_root: &Path, task_id: &str, primary: &str, fallback: &str) {
    std::fs::create_dir_all(project_root.join(".zforge/tasks").join(task_id)).unwrap();
    let mut ts = TaskState::new_with_flow(task_id, Flow::Full);
    ts.assigned_agent = Some(primary.into());
    ts.active_agent = Some(primary.into());
    ts.fallback_agent = Some(fallback.into());
    ts.save(&project_root.join(".zforge/tasks")).unwrap();
}

fn load_state(project_root: &Path, task_id: &str) -> TaskState {
    TaskState::load(&project_root.join(".zforge/tasks"), task_id).unwrap()
}

// ─── test ─────────────────────────────────────────────────────────────────────

/// Primary (fake) emits a codex-style 429 stderr signature. The
/// orchestrator's `should_fallback` matches the `"status":429` regex (added
/// to defaults in PR9) and swaps `active_agent` to the real claude binary,
/// which then runs to success.
#[test]
#[serial]
fn fake_primary_429_falls_back_to_real_claude() {
    let Some(_) = fallback_e2e_enabled() else {
        return;
    };
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let cfg_dir = tempfile::tempdir().unwrap();
    // Primary: codex-style 429 stderr signature.
    let primary_cfg = write_fake_config(
        cfg_dir.path(),
        "primary",
        &json!({
            "exit_code": 1,
            "stderr": r#"ERROR: {"type":"error","status":429,"error":{"type":"rate_limit_error"}}"#
        }),
    );

    // Build registry: primary = fake_agent (will fail with 429), fallback =
    // real claude with `-p` flag.
    let mut r = Registry {
        fallback_policy: FallbackPolicy {
            max_retries: 2,
            cooldown_seconds: 0, // no sleep — test should be fast
            ..FallbackPolicy::default()
        },
        ..Default::default()
    };
    r.agents.insert(
        "primary".into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: vec![primary_cfg.to_str().unwrap().into()],
        },
    );
    r.agents.insert(
        "real_claude".into(),
        AgentSpec {
            command: "claude".into(),
            args: vec![
                "-p".into(),
                // Force a tiny model so the test costs almost nothing.
                "--model".into(),
                "haiku".into(),
            ],
        },
    );
    io::save_atomic(&r).unwrap();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    make_task(proj.path(), "T1", "primary", "real_claude");

    // Run orchestrator with a short, cheap prompt. Headless mode injects
    // `--dangerously-skip-permissions` so claude doesn't block.
    std::env::set_var("ZFORGE_HEADLESS", "1");
    let result = orchestrator::run_phase(
        "T1",
        "code",
        proj.path(),
        "Respond with exactly the single word: SWAPPED\n",
    );
    std::env::remove_var("ZFORGE_HEADLESS");

    assert!(result.is_ok(), "orchestrator should succeed via fallback: {result:?}");

    let ts = load_state(proj.path(), "T1");
    assert_eq!(
        ts.fallback_history.len(),
        1,
        "exactly one fallback should have fired"
    );
    let entry = &ts.fallback_history[0];
    assert_eq!(entry.from, "primary");
    assert_eq!(entry.to, "real_claude");
    // First-match pattern wins; either `rate.?limit` or `"status":429`
    // hits the codex-style stderr. Both prove the PR9 stdout/stderr scan
    // detected the failure.
    assert!(
        entry.reason.starts_with("output_match:"),
        "expected output_match reason; got: {}",
        entry.reason
    );
    assert_eq!(
        ts.active_agent.as_deref(),
        Some("real_claude"),
        "active_agent must move to the fallback"
    );
    assert_eq!(
        ts.assigned_agent.as_deref(),
        Some("primary"),
        "assigned_agent is immutable — must NOT change"
    );

    // Sanity: state file should reflect what would survive a process kill.
    let reloaded = TaskState::load(&proj.path().join(".zforge/tasks"), "T1").unwrap();
    assert_eq!(reloaded.active_agent.as_deref(), Some("real_claude"));
    assert_eq!(reloaded.state, State::Imported, "state advance is caller-driven, not orchestrator's job");
}
