//! End-to-end test: orchestrator reads `.zforge/models.yaml` and injects the
//! correct `--model X` arg per (agent, phase). Uses `fake_agent`'s
//! `FAKE_ARGV_DUMP` hook to capture the actual argv the orchestrator spawned.

mod common;

use common::{make_project, TestHome};
use serial_test::serial;
use std::path::PathBuf;
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

/// Scaffold `.zforge/{config.yaml,tasks/<id>/.state.yaml,models.yaml}` so the
/// orchestrator can resolve everything from `project_root`.
fn scaffold_project_with_models(project: &std::path::Path, models_yaml: &str) {
    std::fs::create_dir_all(project.join(".zforge/tasks/T1")).unwrap();
    // Minimal config.yaml — orchestrator doesn't read it directly but the
    // `.zforge/` marker is what `load_models_from_root` keys off.
    std::fs::write(
        project.join(".zforge/config.yaml"),
        "project:\n  name: t\n  language: rust\n  test_command: cargo test\n  root_dir: .\nopencode:\n  model: claude-sonnet-4-6\n  context_files: []\npaths:\n  tasks: ./.zforge/tasks\n  agents: ./.zforge/agents\n  memory: ./.zforge/memory\n  skills: ./.zforge/skills\nreview:\n  auto_approve: false\n",
    )
    .unwrap();
    std::fs::write(project.join(".zforge/models.yaml"), models_yaml).unwrap();
}

fn make_task_with_agent(project: &std::path::Path, task_id: &str, agent: &str) {
    let mut ts = TaskState::new(task_id);
    ts.assigned_agent = Some(agent.into());
    ts.active_agent = Some(agent.into());
    ts.save(&project.join(".zforge/tasks")).unwrap();
}

fn seed_registry(agent_name: &str) {
    let mut r = Registry {
        fallback_policy: FallbackPolicy {
            max_retries: 0,
            cooldown_seconds: 0,
            ..FallbackPolicy::default()
        },
        ..Default::default()
    };
    r.agents.insert(
        agent_name.into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: vec![],
        },
    );
    io::save_atomic(&r).unwrap();
}

fn read_argv_dump(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
#[serial]
fn claude_phase_plan_gets_model_opus_injected() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    scaffold_project_with_models(
        proj.path(),
        "claude:\n  plan: opus\n  code: sonnet\n",
    );
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);

    let result = orchestrator::run_phase("T1", "plan", proj.path(), "prompt body");

    std::env::remove_var("FAKE_ARGV_DUMP");
    assert!(result.is_ok(), "run_phase failed: {result:?}");

    let argv = read_argv_dump(&dump_path);
    // argv[0] = binary path. Subsequent items are the registered args + the
    // model args appended by `with_model_args`.
    assert!(
        argv.iter().any(|a| a == "--model"),
        "expected --model flag, got: {argv:?}"
    );
    assert!(
        argv.iter().any(|a| a == "opus"),
        "expected opus value, got: {argv:?}"
    );
}

#[test]
#[serial]
fn claude_phase_code_gets_model_sonnet_injected() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    scaffold_project_with_models(
        proj.path(),
        "claude:\n  plan: opus\n  code: sonnet\n",
    );
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);

    let _ = orchestrator::run_phase("T1", "code", proj.path(), "prompt body");
    std::env::remove_var("FAKE_ARGV_DUMP");

    let argv = read_argv_dump(&dump_path);
    let pos = argv.iter().position(|a| a == "--model").expect("--model");
    assert_eq!(argv.get(pos + 1).map(|s| s.as_str()), Some("sonnet"));
}

#[test]
#[serial]
fn no_models_yaml_means_no_injection() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    // Scaffold config but no models.yaml.
    std::fs::create_dir_all(proj.path().join(".zforge/tasks/T1")).unwrap();
    std::fs::write(
        proj.path().join(".zforge/config.yaml"),
        "project:\n  name: t\n  language: rust\n  test_command: cargo test\n  root_dir: .\nopencode:\n  model: claude-sonnet-4-6\n  context_files: []\npaths:\n  tasks: ./.zforge/tasks\n  agents: ./.zforge/agents\n  memory: ./.zforge/memory\n  skills: ./.zforge/skills\nreview:\n  auto_approve: false\n",
    )
    .unwrap();
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);

    let _ = orchestrator::run_phase("T1", "plan", proj.path(), "prompt");
    std::env::remove_var("FAKE_ARGV_DUMP");

    let argv = read_argv_dump(&dump_path);
    assert!(
        !argv.iter().any(|a| a == "--model"),
        "no models.yaml → no --model injection, got: {argv:?}"
    );
}

#[test]
#[serial]
fn global_models_yaml_used_when_local_missing() {
    ensure_fake_agent_built();
    let h = TestHome::new();

    // Seed global $ZFORGE_HOME/models.yaml — no local override.
    std::fs::write(
        h.home.path().join("models.yaml"),
        "claude:\n  plan: global-opus\n",
    )
    .unwrap();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    // Local has config.yaml but NO models.yaml.
    std::fs::write(
        proj.path().join(".zforge/config.yaml"),
        "project:\n  name: t\n  language: rust\n  test_command: cargo test\n  root_dir: .\nopencode:\n  model: claude-sonnet-4-6\n  context_files: []\npaths:\n  tasks: ./.zforge/tasks\n  agents: ./.zforge/agents\n  memory: ./.zforge/memory\n  skills: ./.zforge/skills\nreview:\n  auto_approve: false\n",
    )
    .unwrap();
    std::fs::create_dir_all(proj.path().join(".zforge/tasks/T1")).unwrap();
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);

    let _ = orchestrator::run_phase("T1", "plan", proj.path(), "prompt");
    std::env::remove_var("FAKE_ARGV_DUMP");

    let argv = read_argv_dump(&dump_path);
    let pos = argv
        .iter()
        .position(|a| a == "--model")
        .expect("--model not found");
    assert_eq!(
        argv.get(pos + 1).map(|s| s.as_str()),
        Some("global-opus"),
        "global models.yaml should drive injection when local absent"
    );
}

#[test]
#[serial]
fn local_models_yaml_overrides_global_per_field() {
    ensure_fake_agent_built();
    let h = TestHome::new();

    // Global: claude has plan=global-opus AND code=global-sonnet.
    std::fs::write(
        h.home.path().join("models.yaml"),
        "claude:\n  plan: global-opus\n  code: global-sonnet\n",
    )
    .unwrap();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    // Local overrides ONLY plan; code should inherit from global.
    scaffold_project_with_models(proj.path(), "claude:\n  plan: local-opus\n");
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    // Verify plan uses local override.
    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);
    let _ = orchestrator::run_phase("T1", "plan", proj.path(), "prompt");
    std::env::remove_var("FAKE_ARGV_DUMP");
    let argv = read_argv_dump(&dump_path);
    let pos = argv.iter().position(|a| a == "--model").unwrap();
    assert_eq!(argv.get(pos + 1).map(|s| s.as_str()), Some("local-opus"));

    // Re-run for `code` phase — inherits global since local didn't override.
    let dump2 = dump_dir.path().join("argv2.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump2);
    let _ = orchestrator::run_phase("T1", "code", proj.path(), "prompt");
    std::env::remove_var("FAKE_ARGV_DUMP");
    let argv2 = read_argv_dump(&dump2);
    let pos2 = argv2.iter().position(|a| a == "--model").unwrap();
    assert_eq!(
        argv2.get(pos2 + 1).map(|s| s.as_str()),
        Some("global-sonnet"),
        "code phase should fall back to global when local doesn't override"
    );
}

/// PR B (permission bypass auto-inject): when `ZFORGE_HEADLESS=1` is set (as
/// the background worker does at entry), the orchestrator appends each
/// agent's bypass flags so child processes don't block on interactive
/// confirmation prompts.
#[test]
#[serial]
fn headless_mode_injects_claude_skip_permissions() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    std::fs::write(
        proj.path().join(".zforge/config.yaml"),
        "project:\n  name: t\n  language: rust\n  test_command: cargo test\n  root_dir: .\nopencode:\n  model: claude-sonnet-4-6\n  context_files: []\npaths:\n  tasks: ./.zforge/tasks\n  agents: ./.zforge/agents\n  memory: ./.zforge/memory\n  skills: ./.zforge/skills\nreview:\n  auto_approve: false\n",
    )
    .unwrap();
    std::fs::create_dir_all(proj.path().join(".zforge/tasks/T1")).unwrap();
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);
    std::env::set_var("ZFORGE_HEADLESS", "1");

    let _ = orchestrator::run_phase("T1", "plan", proj.path(), "prompt");

    std::env::remove_var("ZFORGE_HEADLESS");
    std::env::remove_var("FAKE_ARGV_DUMP");

    let argv = read_argv_dump(&dump_path);
    assert!(
        argv.iter().any(|a| a == "--dangerously-skip-permissions"),
        "headless mode must inject claude bypass flag; got argv: {argv:?}"
    );
}

#[test]
#[serial]
fn foreground_mode_does_not_inject_bypass_flags() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    std::fs::write(
        proj.path().join(".zforge/config.yaml"),
        "project:\n  name: t\n  language: rust\n  test_command: cargo test\n  root_dir: .\nopencode:\n  model: claude-sonnet-4-6\n  context_files: []\npaths:\n  tasks: ./.zforge/tasks\n  agents: ./.zforge/agents\n  memory: ./.zforge/memory\n  skills: ./.zforge/skills\nreview:\n  auto_approve: false\n",
    )
    .unwrap();
    std::fs::create_dir_all(proj.path().join(".zforge/tasks/T1")).unwrap();
    make_task_with_agent(proj.path(), "T1", "claude");
    seed_registry("claude");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);
    // Ensure env var is NOT set.
    std::env::remove_var("ZFORGE_HEADLESS");

    let _ = orchestrator::run_phase("T1", "plan", proj.path(), "prompt");

    std::env::remove_var("FAKE_ARGV_DUMP");

    let argv = read_argv_dump(&dump_path);
    assert!(
        !argv.iter().any(|a| a == "--dangerously-skip-permissions"),
        "foreground mode must NOT inject bypass flag; got argv: {argv:?}"
    );
}

#[test]
#[serial]
fn unknown_agent_skips_injection_even_with_models_yaml() {
    ensure_fake_agent_built();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    // models.yaml mentions a custom agent but ModelsConfig only knows
    // claude/codex/opencode keys — so the lookup returns None and nothing
    // injects. This locks the "config drives behavior; unknown agents
    // require user-baked args" rule.
    scaffold_project_with_models(proj.path(), "claude:\n  plan: opus\n");
    make_task_with_agent(proj.path(), "T1", "mystery");
    seed_registry("mystery");

    let dump_dir = tempfile::tempdir().unwrap();
    let dump_path = dump_dir.path().join("argv.txt");
    std::env::set_var("FAKE_ARGV_DUMP", &dump_path);

    let _ = orchestrator::run_phase("T1", "plan", proj.path(), "prompt");
    std::env::remove_var("FAKE_ARGV_DUMP");

    let argv = read_argv_dump(&dump_path);
    assert!(!argv.iter().any(|a| a == "--model"));
}
