#![allow(dead_code)]

use std::path::PathBuf;
use tempfile::TempDir;

/// Test harness that points `ZFORGE_HOME` at a temp directory for the lifetime
/// of the test. Combine with `#[serial_test::serial]` because the env var is
/// process-global.
pub struct TestHome {
    pub home: TempDir,
}

impl TestHome {
    pub fn new() -> Self {
        let home = tempfile::tempdir().expect("tempdir");
        std::env::set_var("ZFORGE_HOME", home.path());
        Self { home }
    }

    pub fn registry_path(&self) -> PathBuf {
        self.home.path().join("registry.yaml")
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        std::env::remove_var("ZFORGE_HOME");
    }
}

/// Create a directory at `path` with a `.zforge/` subdir so it passes
/// `validate::ensure_zforge_dir`.
pub fn make_project(path: &std::path::Path) {
    std::fs::create_dir_all(path.join(".zforge/tasks")).unwrap();
}

/// Write a registry under `ZFORGE_HOME` with the given agent names registered.
/// Each name maps to a stub `AgentSpec` whose `command` equals the name. Used
/// by task-import tests that need `validate_agent_args` to succeed.
pub fn seed_registry_with_agents(names: &[&str]) {
    use zforge::registry::{
        io,
        schema::{AgentSpec, Registry},
    };
    let mut r = Registry::default();
    for n in names {
        r.agents.insert(
            (*n).to_string(),
            AgentSpec {
                command: (*n).to_string(),
                args: vec![],
            },
        );
    }
    io::save_atomic(&r).unwrap();
}

/// Scaffold `.zforge/tasks/` + a minimal `.zforge/config.yaml` at `path` so
/// `config::load()` succeeds when cwd is inside `path`. Caller must
/// `set_current_dir` into `path` before invoking task imports.
pub fn make_project_with_config(path: &std::path::Path) {
    std::fs::create_dir_all(path.join(".zforge/tasks")).unwrap();
    let cfg = r#"project:
  name: "test"
  language: "rust"
  test_command: "cargo test"
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
"#;
    std::fs::write(path.join(".zforge/config.yaml"), cfg).unwrap();
}
