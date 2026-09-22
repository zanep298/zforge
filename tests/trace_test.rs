#![cfg(unix)]
//! IMP-006: every agent spawn leaves a trace of what the client reported.
//!
//! Driven through the real binary. `claude` is a stub that replays a real
//! Claude Code 2.1.278 stream (`tests/fixtures/claude/stream_code_agent.jsonl`),
//! so the parser sees exactly what the client emits without calling a model.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/claude/stream_code_agent.jsonl"
);

const UNTRUSTED: &str = "Ignoring 17 permissions.allow entries from .claude/settings.json: \
                         this workspace has not been trusted.";

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Project {
    /// Docs-flow task T1 at Imported, assigned to `claude`, with a
    /// `code-agent` definition and a CodeGraph index directory.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let home = dir.path().join("zf");
        for d in [
            ".zforge/tasks/T1",
            ".zforge/agents",
            ".claude/agents",
            ".codegraph",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: shell\n  test_command: \"true\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".zforge/agents/code.tmpl"),
            "Implement {{task_id}}\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".claude/agents/code-agent.md"),
            "---\nname: code-agent\ndescription: code\n---\nImplement.\n",
        )
        .unwrap();
        std::fs::write(root.join(".zforge/tasks/T1/task.md"), "# Add docs\n").unwrap();
        std::fs::write(
            root.join(".zforge/tasks/T1/.state.yaml"),
            "task_id: T1\nflow: Docs\nstate: Imported\nupdated_at: \"2026-01-01T00:00:00+00:00\"\n\
             history: []\nassigned_agent: claude\nactive_agent: claude\n",
        )
        .unwrap();
        Self {
            _dir: dir,
            root,
            home,
        }
    }

    /// Register `claude` as a stub script with the given body.
    fn stub_claude(&self, body: &str) {
        let script = self.home.join("claude-stub");
        std::fs::write(&script, format!("#!/bin/sh\ncat > /dev/null\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            self.home.join("registry.yaml"),
            format!(
                "agents:\n  claude:\n    command: {}\n    args: [\"-p\", \"--output-format\", \"stream-json\", \"--verbose\"]\n\
                 fallback_policy:\n  max_retries: 0\n  cooldown_seconds: 0\n",
                script.display()
            ),
        )
        .unwrap();
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
            .output()
            .unwrap()
    }

    fn traces(&self) -> Vec<serde_json::Value> {
        read_lines(&self.root.join(".zforge/tasks/T1/trace.jsonl"))
    }
}

fn read_lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn messages(trace: &serde_json::Value) -> Vec<String> {
    trace["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["message"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_phase_run_records_what_the_client_loaded_and_did() {
    let p = Project::new();
    p.stub_claude(&format!("cat {FIXTURE}\necho '{UNTRUSTED}' >&2"));

    let out = p.zforge(&["code", "T1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let traces = p.traces();
    assert_eq!(traces.len(), 1);
    let t = &traces[0];
    assert_eq!(t["phase"], "code");
    assert_eq!(t["runner"], "claude");
    assert_eq!(t["attempt"], 1);
    let command: Vec<&str> = t["command"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        command.windows(2).any(|w| w == ["--agent", "code-agent"]),
        "{command:?}"
    );

    assert_eq!(t["expected"]["named_agent"], "code-agent");
    assert_eq!(t["expected"]["mcp_servers"][0], "codegraph");

    let o = &t["observed"];
    assert_eq!(o["client_version"], "2.1.278");
    assert_eq!(o["mcp_servers"][0]["status"], "connected");
    assert_eq!(o["tool_calls"]["mcp__codegraph__codegraph_search"], 1);
    assert_eq!(o["result"]["subtype"], "success");

    let m = messages(t);
    assert!(
        m.iter()
            .any(|m| m.starts_with("Ignoring 17 permissions.allow")),
        "{m:?}"
    );
    assert!(
        m.iter()
            .any(|m| m.contains("refused by the permission system")),
        "{m:?}"
    );
    assert!(
        !m.iter().any(|m| m.contains("not in the client's")),
        "the replayed catalog has the agent and its skills: {m:?}"
    );
    assert!(t["not_observable"][0].as_str().unwrap().contains("skills"));
}

/// A client that dies mid-run leaves a trace saying so, not a clean one.
#[test]
fn a_run_cut_short_is_recorded_as_unfinished() {
    let p = Project::new();
    p.stub_claude(&format!("head -n 8 {FIXTURE}\nexit 1"));

    let out = p.zforge(&["code", "T1"]);
    assert!(!out.status.success());

    let traces = p.traces();
    assert_eq!(traces.len(), 1, "a failed spawn is traced too");
    assert_eq!(traces[0]["exit_code"], 1);
    assert!(
        messages(&traces[0])
            .iter()
            .any(|m| m.contains("did not finish")),
        "{:?}",
        messages(&traces[0])
    );
}

/// A runner zforge cannot read still gets a record — marked unavailable.
#[test]
fn plain_text_output_is_recorded_as_untraceable() {
    let p = Project::new();
    p.stub_claude("echo done");

    let out = p.zforge(&["code", "T1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let t = &p.traces()[0];
    assert!(t.get("observed").is_none());
    assert!(t["unavailable"].as_str().unwrap().contains("stream-json"));
}

/// `zforge trace` joins the phase runs with verification and evidence.
#[test]
fn trace_command_follows_the_task_from_run_to_evidence() {
    let p = Project::new();
    p.stub_claude(&format!("cat {FIXTURE}"));
    assert!(p.zforge(&["code", "T1"]).status.success());

    let out = p.zforge(&["trace", "T1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    for want in [
        "T1 — Add docs",
        "code · attempt 1 · claude",
        "agent code-agent",
        "mcp: codegraph connected",
        "verification",
        "not run",
        "evidence: invalid",
    ] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }

    let out = p.zforge(&["trace", "T1", "--json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["phases"][0]["observed"]["client_version"], "2.1.278");
    assert_eq!(v["evidence"]["status"], "invalid");
}
