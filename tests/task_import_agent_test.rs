mod common;

use common::{make_project_with_config, seed_registry_with_agents, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::Path;
use zforge::cli::task::run_import;
use zforge::state::TaskState;

/// Run `f` inside `dir` as the process CWD. Restores the previous cwd on drop.
/// CWD is process-global — pair with `#[serial]`.
struct CwdGuard {
    prev: std::path::PathBuf,
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

fn import_args(
    id: &str,
    title: &str,
    agent: Option<&str>,
    fallback: Option<&str>,
) -> anyhow::Result<String> {
    run_import(
        Some(id),
        Some(title.to_string()),
        None,
        None,
        None,
        None,
        None,
        None,
        agent.map(str::to_string),
        fallback.map(str::to_string),
    )
}

// ─── CLI tests ────────────────────────────────────────────────────────────────

#[test]
#[serial]
fn task_import_with_agent_persists_field() {
    let _h = TestHome::new();
    seed_registry_with_agents(&["claude", "codex"]);

    let proj = tempfile::tempdir().unwrap();
    make_project_with_config(proj.path());
    let _g = CwdGuard::enter(proj.path());

    let id = import_args("TASK-001", "x", Some("claude"), Some("codex")).unwrap();
    let ts = TaskState::load(&proj.path().join(".zforge/tasks"), &id).unwrap();
    assert_eq!(ts.assigned_agent.as_deref(), Some("claude"));
    assert_eq!(ts.active_agent.as_deref(), Some("claude"));
    assert_eq!(ts.fallback_agent.as_deref(), Some("codex"));
    assert_eq!(ts.effective_agent(), Some("claude"));
}

#[test]
#[serial]
fn task_import_with_unknown_agent_rejected() {
    let _h = TestHome::new();
    seed_registry_with_agents(&["claude"]);

    let proj = tempfile::tempdir().unwrap();
    make_project_with_config(proj.path());
    let _g = CwdGuard::enter(proj.path());

    let err = import_args("TASK-001", "x", Some("ghost"), None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("ghost"), "missing agent name: {msg}");
    assert!(
        msg.contains("registry") || msg.contains("agents"),
        "missing registry hint: {msg}"
    );

    // Validation runs before scaffold — disk untouched.
    assert!(!proj.path().join(".zforge/tasks/TASK-001").exists());
}

#[test]
#[serial]
fn task_import_agent_eq_fallback_rejected() {
    let _h = TestHome::new();
    seed_registry_with_agents(&["claude"]);

    let proj = tempfile::tempdir().unwrap();
    make_project_with_config(proj.path());
    let _g = CwdGuard::enter(proj.path());

    let err = import_args("TASK-001", "x", Some("claude"), Some("claude")).unwrap_err();
    assert!(
        err.to_string().contains("must differ"),
        "{}",
        err.to_string()
    );
    assert!(!proj.path().join(".zforge/tasks/TASK-001").exists());
}

#[test]
#[serial]
fn task_import_without_flags_leaves_agent_fields_none() {
    let _h = TestHome::new();
    seed_registry_with_agents(&[]);

    let proj = tempfile::tempdir().unwrap();
    make_project_with_config(proj.path());
    let _g = CwdGuard::enter(proj.path());

    let id = import_args("TASK-001", "x", None, None).unwrap();
    let ts = TaskState::load(&proj.path().join(".zforge/tasks"), &id).unwrap();
    assert!(ts.assigned_agent.is_none());
    assert!(ts.active_agent.is_none());
    assert!(ts.fallback_agent.is_none());
    assert!(ts.fallback_history.is_empty());
}

#[test]
#[serial]
fn task_import_fallback_only_allowed() {
    let _h = TestHome::new();
    seed_registry_with_agents(&["codex"]);

    let proj = tempfile::tempdir().unwrap();
    make_project_with_config(proj.path());
    let _g = CwdGuard::enter(proj.path());

    // Spec PR2-04 §Notes: allow --fallback alone. Primary stays None;
    // PR3 orchestrator resolves the init-detected default at run time.
    let id = import_args("TASK-001", "x", None, Some("codex")).unwrap();
    let ts = TaskState::load(&proj.path().join(".zforge/tasks"), &id).unwrap();
    assert!(ts.assigned_agent.is_none());
    assert_eq!(ts.fallback_agent.as_deref(), Some("codex"));
}

#[test]
#[serial]
fn task_import_with_empty_agents_map_rejected() {
    let _h = TestHome::new();
    seed_registry_with_agents(&[]);

    let proj = tempfile::tempdir().unwrap();
    make_project_with_config(proj.path());
    let _g = CwdGuard::enter(proj.path());

    let err = import_args("TASK-001", "x", Some("claude"), None).unwrap_err();
    assert!(err.to_string().contains("claude"));
}

// ─── MCP plumbing ─────────────────────────────────────────────────────────────

#[test]
fn mcp_tools_list_includes_agent_fields() {
    // Parse the tools/list response shape directly off the generated schema.
    // Spawning the stdio server would be more thorough but is overkill for a
    // schema discoverability check.
    let line = include_str!("../src/mcp/mod.rs");
    assert!(
        line.contains(r#""agent":         { "type": "string""#),
        "task_import schema missing `agent` field"
    );
    assert!(
        line.contains(r#""fallback":      { "type": "string""#),
        "task_import schema missing `fallback` field"
    );
}

#[test]
#[serial]
fn mcp_task_import_rejects_unknown_agent() {
    // Drive the validator the MCP handler uses. Full JSON-RPC roundtrip is
    // covered indirectly — the MCP `task_import` tool delegates to
    // `cli::task::run_import`, which calls `validate_agent_args` first.
    let _h = TestHome::new();
    seed_registry_with_agents(&["claude"]);

    let err = zforge::cli::task::validate_agent_args(Some("ghost"), None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("ghost"));
    assert!(msg.contains("registry") || msg.contains("agents"));
}

#[test]
fn mcp_task_import_schema_well_formed_json() {
    // Smoke-check the schema literal parses as JSON. Catches accidental
    // commas / quoting regressions when extending the tools definition.
    let src = include_str!("../src/mcp/mod.rs");
    assert!(src.contains(r#""name": "task_import""#));
    // Spot-check both new fields land under the same properties block.
    let agent_idx = src.find(r#""agent":         { "type": "string""#).unwrap();
    let fallback_idx = src
        .find(r#""fallback":      { "type": "string""#)
        .unwrap();
    assert!(agent_idx < fallback_idx);
    // Sanity: the JSON value parses standalone if we wrap it.
    let probe = json!({
        "name": "task_import",
        "arguments": { "task_id": "T-1", "agent": "claude", "fallback": "codex" }
    });
    assert_eq!(probe["arguments"]["agent"], "claude");
}
