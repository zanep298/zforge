#![cfg(unix)]
//! IMP-006 benchmark: one bounded task, init → handover, on the real Claude
//! Code CLI and a real model. Ignored by default — it spends money and needs
//! a logged-in `claude`, `codegraph` and network. Run it explicitly:
//!
//! ```text
//! cargo test --test bench_claude_test -- --ignored --nocapture
//! ```
//!
//! `BENCH_MODEL` (default `haiku`) picks the model for every phase and
//! `BENCH_BUDGET_USD` (default `0.40`) caps each agent spawn through
//! `claude --max-budget-usd`. The scenario is fixed: a shell project whose
//! `add` subtracts, a Fixbug task to fix it, spec → testspec → ship with at
//! most two verifier iterations.
//!
//! Pass criteria are about the setup, not the model's taste: every phase
//! run was traced and finished, no infrastructure finding (agent/skill/
//! server missing, server not connected, refused calls), and the final
//! verification passed on the candidate that is on disk. Agent-choice
//! findings (e.g. CodeGraph connected but not called) are reported, not
//! failed — one green run does not prove every task will use its tools.
//!
//! The trace (text and JSON) is written to `target/bench/claude-<time>/`.
//! Runs with `ZFORGE_HEADLESS=1` (like `ship --async`) because a fresh temp
//! workspace is untrusted and Claude would drop the project allowlist.
//! `init` registers CodeGraph for the temp project in Claude's local scope;
//! the benchmark removes it again at the end.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Bench {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Bench {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("calc");
        let home = dir.path().join("zf");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(root.join("lib.sh"), "add() { echo $(( $1 - $2 )); }\n").unwrap();
        std::fs::write(
            root.join("test.sh"),
            "#!/bin/sh\n. ./lib.sh\nfail=0\n\
             [ \"$(add 2 3)\" = 5 ] || { echo 'FAIL add_small'; fail=1; }\n\
             [ \"$(add 10 -4)\" = 6 ] || { echo 'FAIL add_negative'; fail=1; }\n\
             [ $fail = 0 ] && echo 'ok 2 tests'\nexit $fail\n",
        )
        .unwrap();
        let b = Self {
            _dir: dir,
            root,
            home,
        };
        b.git(&["init", "-q", "."]);
        b.git(&["add", "-A"]);
        b.git(&[
            "-c",
            "user.email=b@b",
            "-c",
            "user.name=b",
            "commit",
            "-qm",
            "calc",
        ]);
        b
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    fn zforge(&self, args: &[&str]) -> Output {
        let out = Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
            .env("ZFORGE_HEADLESS", "1")
            .output()
            .unwrap();
        eprintln!(
            "$ zforge {}\n{}{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn step(&self, args: &[&str]) {
        let out = self.zforge(args);
        assert!(out.status.success(), "`zforge {}` failed", args.join(" "));
    }

    /// Pin the runner, model and budget the scenario is defined with.
    fn configure(&self, model: &str, budget: &str) {
        std::fs::write(
            self.home.join("registry.yaml"),
            format!(
                "agents:\n  claude:\n    command: claude\n    args: [\"-p\", \"--output-format\", \"stream-json\", \
                 \"--verbose\", \"--max-budget-usd\", \"{budget}\"]\n\
                 fallback_policy:\n  max_retries: 0\n  cooldown_seconds: 0\n  spawn_timeout_secs: 900\n"
            ),
        )
        .unwrap();
        std::fs::write(
            self.root.join(".zforge/models.yaml"),
            format!("claude:\n  spec: {model}\n  testspec: {model}\n  code: {model}\n"),
        )
        .unwrap();
        let config = self.root.join(".zforge/config.yaml");
        let text = std::fs::read_to_string(&config).unwrap();
        let text: String = text
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("test_command:") {
                    "  test_command: \"sh test.sh\"".to_string()
                } else {
                    l.to_string()
                }
            })
            .map(|l| l + "\n")
            .collect();
        std::fs::write(config, text).unwrap();
    }
}

/// Undo `init`'s CodeGraph registration in Claude's local scope, however
/// the benchmark ends.
impl Drop for Bench {
    fn drop(&mut self) {
        let _ = Command::new("claude")
            .args(["mcp", "remove", "--scope", "local", "codegraph"])
            .current_dir(&self.root)
            .output();
    }
}

fn save_report(text: &str, json: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/bench")
        .join(format!(
            "claude-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("trace.txt"), text).unwrap();
    std::fs::write(dir.join("trace.json"), json).unwrap();
    dir
}

#[test]
#[ignore = "calls the real Claude Code CLI and a model; costs money"]
fn bench_claude_fixbug_task_end_to_end() {
    let model = std::env::var("BENCH_MODEL").unwrap_or_else(|_| "haiku".into());
    let budget = std::env::var("BENCH_BUDGET_USD").unwrap_or_else(|_| "0.40".into());
    let b = Bench::new();

    b.step(&["init", "--no-install", "--no-register", "--agent", "claude"]);
    b.configure(&model, &budget);
    b.step(&[
        "task",
        "import",
        "BENCH-1",
        "--title",
        "add returns the difference instead of the sum",
        "--domain",
        "calc",
        "--description",
        "lib.sh defines add(a, b), which prints a - b. It must print a + b. \
         sh test.sh is the test suite; it must pass. Change only lib.sh.",
        "--flow",
        "fixbug",
        "--agent",
        "claude",
    ]);
    b.step(&["spec", "BENCH-1"]);
    b.step(&["spec", "BENCH-1", "--done"]);
    b.step(&["testspec", "BENCH-1"]);
    b.step(&["testspec", "BENCH-1", "--done"]);
    let shipped = b.zforge(&["ship", "BENCH-1", "--max-iterations", "2"]);

    let text = String::from_utf8_lossy(&b.zforge(&["trace", "BENCH-1"]).stdout).into_owned();
    let json_out = b.zforge(&["trace", "BENCH-1", "--json"]);
    let json = String::from_utf8_lossy(&json_out.stdout).into_owned();
    let report_dir = save_report(&text, &json);
    eprintln!("\n{text}\nreport: {}", report_dir.display());

    let report: serde_json::Value = serde_json::from_str(&json).expect("trace --json");
    let phases = report["phases"].as_array().unwrap();
    let ran: Vec<&str> = phases
        .iter()
        .map(|p| p["phase"].as_str().unwrap())
        .collect();
    for phase in ["spec", "testspec", "code"] {
        assert!(
            ran.contains(&phase),
            "phase {phase} was not traced: {ran:?}"
        );
    }
    let mut problems = Vec::new();
    for p in phases {
        let name = p["phase"].as_str().unwrap();
        if p["observed"].is_null() {
            problems.push(format!("{name}: no trace ({})", p["unavailable"]));
        } else if p["observed"]["result"]["subtype"] != "success" {
            problems.push(format!("{name}: run did not succeed"));
        }
        for f in p["findings"].as_array().into_iter().flatten() {
            if f["kind"] != "agent_choice" {
                problems.push(format!("{name}: {}: {}", f["kind"], f["message"]));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "setup problems:\n{}",
        problems.join("\n")
    );
    assert!(
        shipped.status.success(),
        "ship did not reach a passing verification"
    );
    let last = report["verifications"]
        .as_array()
        .and_then(|v| v.last())
        .cloned();
    assert_eq!(
        last.map(|v| v["passed"].clone()),
        Some(serde_json::json!(true))
    );
    assert_eq!(report["evidence"]["status"], "current");
}
