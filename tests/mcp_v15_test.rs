#![cfg(unix)]
//! v1.5 intake and run tools over MCP (decision D4).
//!
//! Driven through a real `zforge mcp` subprocess: what matters is what an
//! agent can reach over the transport. The point of these tools is what they
//! do *not* offer — accepting a revision or handing over is the user's, at a
//! terminal (D1).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn call(id: u64, tool: &str, args: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    })
}

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: shell\n  test_command: \"true\"\n\
             execution:\n  budget_usd: 1.0\n",
        )
        .unwrap();
        for args in [
            &["init", "-q", "-b", "main", "."][..],
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "base",
            ],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        Self { _dir: dir, root }
    }

    /// Run one MCP session and return the tool results by request id.
    fn mcp(&self, requests: &[Value]) -> Vec<Value> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zforge"))
            .arg("mcp")
            .current_dir(&self.root)
            .env("ZFORGE_HOME", self.root.join(".home"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            for r in requests {
                writeln!(stdin, "{r}").unwrap();
            }
        }
        drop(child.stdin.take());
        let out = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON-RPC ({e}): {l}")))
            .collect()
    }
}

/// Text of a tool result, or the error message.
fn text(frame: &Value) -> String {
    if let Some(e) = frame.get("error") {
        return e["message"].as_str().unwrap_or("").to_string();
    }
    frame["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn json_result(frame: &Value) -> Value {
    serde_json::from_str(&text(frame)).unwrap_or_else(|e| panic!("not JSON ({e}): {}", text(frame)))
}

/// D1: no tool records the user's decision, and none can be called.
#[test]
fn the_transport_offers_no_way_to_decide() {
    let p = Project::new();
    let frames = p.mcp(&[json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})]);
    let tools = frames[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    for forbidden in zforge::mcp::v15::FORBIDDEN {
        assert!(
            !names.contains(&forbidden),
            "{forbidden} must not be an MCP tool"
        );
    }
    for expected in [
        "intake_new",
        "intake_status",
        "intake_review",
        "readiness",
        "knowledge_index",
        "run_start",
        "run_status",
        "run_cancel",
    ] {
        assert!(names.contains(&expected), "missing {expected} in {names:?}");
    }

    // Calling one anyway is an unknown tool, not a hidden path.
    let frames = p.mcp(&[call(
        1,
        "intake_accept",
        json!({"intake_id": "F", "file": "01-outcome.md"}),
    )]);
    assert!(text(&frames[0]).contains("unknown tool"), "{:?}", frames[0]);
}

/// Preparing an intake over MCP: create, fill, review, see the state.
#[test]
fn an_agent_prepares_an_intake_and_stops_at_the_decision() {
    let p = Project::new();
    let frames = p.mcp(&[
        call(1, "intake_new", json!({"intake_id": "F"})),
        call(
            2,
            "intake_task",
            json!({"intake_id": "F", "task_id": "TASK-001"}),
        ),
    ]);
    assert!(text(&frames[0]).contains("intakes/F"), "{:?}", frames[0]);
    assert!(text(&frames[1]).contains("TASK-001.md"));

    // An untouched template cannot be reviewed.
    let frames = p.mcp(&[call(
        1,
        "intake_review",
        json!({"intake_id": "F", "file": "01-outcome.md"}),
    )]);
    assert!(
        text(&frames[0]).contains("no content yet"),
        "{:?}",
        frames[0]
    );

    std::fs::write(
        p.root.join(".zforge/intakes/F/01-outcome.md"),
        "# F\n\n## Yêu cầu\n\n- REQ-001: lọc\n\n## Câu hỏi còn mở\n\n- [ ] còn gì nữa?\n",
    )
    .unwrap();
    let frames = p.mcp(&[
        call(
            1,
            "intake_review",
            json!({"intake_id": "F", "file": "01-outcome.md"}),
        ),
        call(2, "intake_status", json!({"intake_id": "F"})),
        call(3, "readiness", json!({"intake_id": "F"})),
    ]);

    let review = json_result(&frames[0]);
    assert_eq!(review["revision"], 1);
    assert!(review["next"]
        .as_str()
        .unwrap()
        .contains("zforge intake accept F"));

    let status = json_result(&frames[1]);
    let outcome = status["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["status"]["file"] == "01-outcome.md")
        .unwrap()
        .clone();
    assert_eq!(outcome["status"]["state"], "in_review");
    assert_eq!(outcome["open_questions"][0], "còn gì nữa?");
    assert!(status["decides"]
        .as_str()
        .unwrap()
        .contains("at a terminal"));

    // Nothing is accepted, so nothing is ready to hand over.
    let readiness = json_result(&frames[2]);
    assert_eq!(readiness["ready"], false);
    let problems = readiness["checks"].to_string();
    assert!(problems.contains("has no accepted revision"), "{problems}");
}

/// Runs over MCP: start in the background, watch, cancel.
#[test]
fn an_agent_starts_and_watches_a_run() {
    let p = Project::new();
    let stub = p.root.join("claude-stub");
    std::fs::write(
        &stub,
        "#!/bin/sh\ncat > /dev/null\necho working\nsleep 30\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(p.root.join(".home")).unwrap();
    std::fs::write(
        p.root.join(".home/registry.yaml"),
        format!(
            "agents:\n  claude:\n    command: {}\n    args: [\"-p\"]\nfallback_policy:\n  max_retries: 0\n",
            stub.display()
        ),
    )
    .unwrap();
    hand_over(&p.root);

    let frames = p.mcp(&[call(
        1,
        "run_start",
        json!({"handover": "HANDOVER-001", "task": "TASK-001"}),
    )]);
    let started = json_result(&frames[0]);
    assert_eq!(started["run"], "RUN-001");
    assert_eq!(started["branch"], "zforge/TASK-001/RUN-001");
    assert_eq!(started["budget_usd"], 1.0);

    // Wait for the agent to be working, then look and stop it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let frames = p.mcp(&[call(1, "run_status", json!({"run": "RUN-001"}))]);
        let status = json_result(&frames[0]);
        if status["state"]["attempts_in_flight"] != Value::Null
            || status["events"].to_string().contains("attempt_started")
        {
            assert_eq!(status["state"]["status"], "running");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never started working"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let frames = p.mcp(&[
        call(1, "run_log", json!({"run": "RUN-001", "tail": 20})),
        call(2, "run_cancel", json!({"run": "RUN-001"})),
        call(3, "run_list", json!({})),
    ]);
    assert!(
        text(&frames[0]).contains("attempt 1"),
        "{:?}",
        text(&frames[0])
    );
    assert!(
        text(&frames[1]).contains("RUN-001 cancelled"),
        "{:?}",
        text(&frames[1])
    );
    let list = json_result(&frames[2]);
    assert_eq!(list[0]["state"]["status"], "cancelled");
    assert_eq!(list[0]["meta"]["task"], "TASK-001");
}

/// An accepted intake handed over through the library, so the run tools
/// have something real to work with.
fn hand_over(root: &Path) {
    use zforge::intake::{handover, readiness, review};
    let i = review::create(root, "F").unwrap();
    let d = &i.dir;
    std::fs::write(
        d.join("01-outcome.md"),
        "# F\n\n## Yêu cầu\n\n- REQ-001: lọc\n\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    std::fs::write(
        d.join("02-behavior.md"),
        "# B\n\n## Tình huống\nREQ-001.\n\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    std::fs::write(
        d.join("03-solution.md"),
        "# S\n\n## Luồng\nSửa.\n\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    std::fs::write(
        d.join("04-breakdown.md"),
        "# K\n\n## Task\nTASK-001\n\n## Kiểm chứng tích hợp\ntrue\n\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    std::fs::write(
        d.join("tasks/TASK-001.md"),
        "---\nid: TASK-001\nparent: F\nrequirements: [REQ-001]\ndepends_on: []\n---\n\n# TASK-001\n\n\
         ## Mục tiêu\nLọc.\n## Input\nCode.\n## Output\nĐúng.\n## Ràng buộc\nGiữ shape.\n## Tự chủ\nTự chọn.\n\
         ## Acceptance và kiểm chứng\n- AC-01: test pass\n## Bàn giao\nLocal.\n## Cần amendment khi\nĐổi shape.\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    for f in i.files() {
        review::review(&i, &f).unwrap();
        review::accept(&i, &f, None).unwrap();
    }
    let config = zforge::config::load_from(&root.join(".zforge/config.yaml")).unwrap();
    let r = readiness::check(&i, root, &[], &config.execution).unwrap();
    assert!(r.ready, "{:?}", r.checks);
    handover::create(&i, root, &[], &config, &r.files, None).unwrap();
}

/// MOC-C TASK-005 AC-05: a whole handover over MCP — start it without a
/// task, watch it by its id, cancel it.
#[test]
fn an_agent_runs_a_whole_handover() {
    let p = Project::new();
    let stub = p.root.join("claude-stub");
    std::fs::write(
        &stub,
        "#!/bin/sh\ncat > /dev/null\necho working\nsleep 30\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(p.root.join(".home")).unwrap();
    std::fs::write(
        p.root.join(".home/registry.yaml"),
        format!(
            "agents:\n  claude:\n    command: {}\n    args: [\"-p\"]\nfallback_policy:\n  max_retries: 0\n",
            stub.display()
        ),
    )
    .unwrap();
    hand_over(&p.root);

    let frames = p.mcp(&[call(1, "run_start", json!({"handover": "HANDOVER-001"}))]);
    let started = json_result(&frames[0]);
    assert_eq!(started["handover"], "F/HANDOVER-001");
    assert!(started["worker_pid"].as_u64().is_some());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let frames = p.mcp(&[call(1, "run_status", json!({"run": "HANDOVER-001"}))]);
        let f = json_result(&frames[0]);
        if f["tasks"][0]["state"] == "running" {
            assert_eq!(f["integration"]["state"], "waiting");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "never started: {f}");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let frames = p.mcp(&[
        call(1, "run_log", json!({"run": "HANDOVER-001"})),
        call(2, "run_cancel", json!({"run": "HANDOVER-001"})),
    ]);
    assert!(
        text(&frames[0]).contains("RUN-001 — TASK-001"),
        "{:?}",
        text(&frames[0])
    );
    let f = json_result(&frames[1]);
    assert_eq!(f["tasks"][0]["state"], "stopped");
    assert_eq!(f["tasks"][0]["status"], "cancelled");
}
