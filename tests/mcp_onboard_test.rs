#![cfg(unix)]
//! ONBOARD TASK-009 AC-02: onboarding over MCP prepares and observes only —
//! probe, status, send for review. Deciding (accept/revise/baseline) stays
//! off the transport (D1); `mcp_v15_test.rs::the_transport_offers_no_way_to_decide`
//! already checks the tool list generically against `v15::FORBIDDEN`, so this
//! file only exercises that the three onboard tools actually work.

use serde_json::{json, Value};
use std::path::PathBuf;
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

/// AC-02: the tool list has the three onboard tools, and calling the
/// decision tool anyway is an unknown tool, not a hidden path (D1).
#[test]
fn the_tool_list_has_the_three_onboard_tools_and_no_decision_tool() {
    let p = Project::new();
    let frames = p.mcp(&[json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})]);
    let tools = frames[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    for expected in ["onboard_status", "onboard_probe", "onboard_review"] {
        assert!(names.contains(&expected), "missing {expected} in {names:?}");
    }
    for forbidden in ["onboard_accept", "onboard_revise", "onboard_baseline"] {
        assert!(
            !names.contains(&forbidden),
            "{forbidden} must not be an MCP tool"
        );
        assert!(
            zforge::mcp::v15::FORBIDDEN.contains(&forbidden),
            "{forbidden} must be in v15::FORBIDDEN"
        );
    }

    let frames = p.mcp(&[call(1, "onboard_accept", json!({"file": "rules.md"}))]);
    assert!(text(&frames[0]).contains("unknown tool"), "{:?}", frames[0]);
}

/// The onboard tools prepare and observe: probe, status, send for review.
#[test]
fn onboard_probe_status_and_review_over_mcp() {
    let p = Project::new();
    // The probe pins knowledge to a commit and refuses an uncommitted tree;
    // `Project::new()`'s own commit is empty (`.zforge/` is left untracked),
    // so commit it here before probing.
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "cfg"][..]] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(&p.root)
            .status()
            .unwrap()
            .success());
    }

    let frames = p.mcp(&[call(1, "onboard_probe", json!({}))]);
    let probe = json_result(&frames[0]);
    assert!(probe["baseline"]["result"].as_bool().unwrap(), "{probe}");
    assert!(
        p.root.join("docs/knowledge/rules.md").is_file(),
        "the probe creates the three knowledge files from a stub"
    );

    std::fs::write(
        p.root.join("docs/knowledge/rules.md"),
        "# Rules\n\nSomething must not happen.\n\n## Open questions\n",
    )
    .unwrap();

    let frames = p.mcp(&[
        call(1, "onboard_review", json!({"file": "rules.md"})),
        call(2, "onboard_status", json!({})),
    ]);
    let r = json_result(&frames[0]);
    assert_eq!(r["revision"], 1, "{r}");

    let s = json_result(&frames[1]);
    assert_eq!(s["onboarded"], false, "{s}");
    let files = s["files"].as_array().unwrap();
    let rules = files
        .iter()
        .find(|f| f["status"]["file"] == "rules.md")
        .unwrap();
    assert_eq!(rules["status"]["state"], "in_review", "{rules}");
}
