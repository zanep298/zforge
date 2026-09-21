//! MCP stdout hygiene (FIX-003).
//!
//! The MCP server speaks JSON-RPC over stdout. The tools behind it are the
//! same `cli::*` functions the CLI uses, and those print progress as they go
//! ("🧪 Running: …", "✓ All tests passed", the verify report path). On the CLI
//! that output is the point; under MCP each line lands in the middle of the
//! transport and the client cannot parse the frame around it.
//!
//! A real `zforge mcp` subprocess is driven here — not the tool functions in
//! isolation — because the bug is in what reaches the pipe, and only a
//! subprocess shows that.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn zforge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_zforge")
}

/// Project whose `test_command` fails, so `verify` takes its noisiest path:
/// failure banner, failed-test list, report path, and the rendered
/// verify-analysis prompt.
fn scaffold_project(root: &Path) {
    std::fs::create_dir_all(root.join(".zforge/tasks/T1")).unwrap();
    std::fs::create_dir_all(root.join(".zforge/agents")).unwrap();

    std::fs::write(
        root.join(".zforge/config.yaml"),
        r#"project:
  name: "test"
  language: "shell"
  test_command: "sh -c 'exit 1'"
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
    )
    .unwrap();

    // A chatty template: if any of this reaches stdout the frame is corrupt.
    std::fs::write(
        root.join(".zforge/agents/verify-analysis.tmpl"),
        "Analyze the failure for {{task_id}}.\nMultiple\nlines\nof\nprompt.\n",
    )
    .unwrap();

    for name in ["task.md", "spec.md", "testspec.md", "plan.md"] {
        std::fs::write(
            root.join(".zforge/tasks/T1").join(name),
            format!("# {name}\nstub\n"),
        )
        .unwrap();
    }

    // Hand-written state at Coded so `verify` runs without walking the FSM.
    std::fs::write(
        root.join(".zforge/tasks/T1/.state.yaml"),
        r#"task_id: T1
flow: Full
state: Coded
updated_at: "2026-01-01T00:00:00+00:00"
history:
  - state: Imported
    at: "2026-01-01T00:00:00+00:00"
    note: ""
  - state: Coded
    at: "2026-01-01T00:00:00+00:00"
    note: "test fixture"
"#,
    )
    .unwrap();
}

/// Feed `requests` to a `zforge mcp` subprocess, return (stdout, stderr).
fn run_mcp_session(project: &Path, requests: &[Value]) -> (String, String) {
    let mut child = Command::new(zforge_bin())
        .arg("mcp")
        .current_dir(project)
        .env("ZFORGE_HOME", project.join("home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn zforge mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for req in requests {
            writeln!(stdin, "{req}").unwrap();
        }
    }
    // Dropping stdin closes the pipe, ending the server's read loop.
    drop(child.stdin.take());

    let out = child.wait_with_output().expect("wait for zforge mcp");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_every_stdout_line_is_json(stdout: &str) -> Vec<Value> {
    let mut frames = Vec::new();
    for (idx, line) in BufReader::new(stdout.as_bytes()).lines().enumerate() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        let parsed: Value = serde_json::from_str(&line).unwrap_or_else(|e| {
            panic!(
                "stdout line {} is not valid JSON-RPC ({e}).\n\
                 Offending line: {line:?}\n\
                 Full stdout:\n{stdout}",
                idx + 1
            )
        });
        assert_eq!(
            parsed.get("jsonrpc").and_then(|v| v.as_str()),
            Some("2.0"),
            "every stdout frame must be JSON-RPC 2.0, got: {parsed}"
        );
        frames.push(parsed);
    }
    frames
}

fn verify_request(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": "verify", "arguments": { "task_id": "T1" } }
    })
}

/// The regression: one `verify` call used to emit several lines of progress
/// text ahead of the JSON response.
#[test]
fn verify_tool_leaves_stdout_pure_json() {
    let project = tempfile::tempdir().unwrap();
    scaffold_project(project.path());

    let (stdout, _stderr) = run_mcp_session(project.path(), &[verify_request(1)]);

    let frames = assert_every_stdout_line_is_json(&stdout);
    assert_eq!(frames.len(), 1, "expected exactly one response frame");
    assert_eq!(frames[0].get("id").and_then(|v| v.as_u64()), Some(1));
}

/// Progress text is not dropped — it moves to stderr, where an MCP client
/// surfaces it as server logs.
#[test]
fn verify_progress_text_goes_to_stderr() {
    let project = tempfile::tempdir().unwrap();
    scaffold_project(project.path());

    let (_stdout, stderr) = run_mcp_session(project.path(), &[verify_request(1)]);

    assert!(
        stderr.contains("Running:") || stderr.contains("Tests failed"),
        "verify progress should still be visible on stderr, got: {stderr:?}"
    );
}

/// Several calls in one session: response IDs must stay aligned with their
/// requests. An unparsed line between frames desynchronizes the client.
#[test]
fn multiple_calls_keep_stdout_framed_and_ordered() {
    let project = tempfile::tempdir().unwrap();
    scaffold_project(project.path());

    let requests = vec![
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        verify_request(3),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
               "params": {"name": "status", "arguments": {"task_id": "T1"}}}),
    ];

    let (stdout, _stderr) = run_mcp_session(project.path(), &requests);

    let frames = assert_every_stdout_line_is_json(&stdout);
    let ids: Vec<u64> = frames
        .iter()
        .filter_map(|f| f.get("id").and_then(|v| v.as_u64()))
        .collect();
    assert_eq!(
        ids,
        vec![1, 2, 3, 4],
        "one response per request, in order; stdout was:\n{stdout}"
    );
}

/// A failing operation must still produce a well-formed frame — the error
/// path is where extra output is most likely to leak.
#[test]
fn failing_verify_returns_a_framed_tool_error() {
    let project = tempfile::tempdir().unwrap();
    scaffold_project(project.path());

    let (stdout, _stderr) = run_mcp_session(project.path(), &[verify_request(1)]);
    let frames = assert_every_stdout_line_is_json(&stdout);

    let frame = &frames[0];
    let is_error = frame
        .pointer("/result/isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || frame.get("error").is_some();
    assert!(
        is_error,
        "a failing test suite must surface as a tool error, got: {frame}"
    );
}

/// FIX-011 over MCP: the orchestrating LLM has written the docs change and
/// calls `ship`. The docs flow has no verify step, so ship ends at Coded
/// instead of failing on a phase the flow does not have.
#[test]
fn mcp_ship_on_docs_flow_ends_at_coded() {
    let project = tempfile::tempdir().unwrap();
    scaffold_project(project.path());
    std::fs::write(
        project.path().join(".zforge/tasks/T1/.state.yaml"),
        r#"task_id: T1
flow: Docs
state: Imported
updated_at: "2026-01-01T00:00:00+00:00"
history:
  - state: Imported
    at: "2026-01-01T00:00:00+00:00"
    note: ""
"#,
    )
    .unwrap();

    let ship = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "ship", "arguments": { "task_id": "T1" } }
    });
    let (stdout, _stderr) = run_mcp_session(project.path(), &[ship]);
    let frames = assert_every_stdout_line_is_json(&stdout);
    let frame = &frames[0];
    assert_eq!(
        frame.pointer("/result/isError").and_then(|v| v.as_bool()),
        Some(false),
        "ship on docs must succeed: {frame}"
    );
    let text = frame
        .pointer("/result/content/0/text")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(text.contains("no verify step"), "{text}");

    let state =
        std::fs::read_to_string(project.path().join(".zforge/tasks/T1/.state.yaml")).unwrap();
    assert!(state.contains("state: Coded"), "{state}");
    assert!(!project.path().join(".zforge/tasks/T1/verify.md").exists());
}
