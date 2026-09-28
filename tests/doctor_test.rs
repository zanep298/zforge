#![cfg(unix)]
//! `zforge doctor` for Claude Code (IMP-005).
//!
//! The real binary runs with every config location in a temp dir and stubs
//! on `PATH`. The `claude` stub answers `claude mcp get <name>` from files
//! the test writes, so each MCP state (connected, pending, pinned elsewhere,
//! absent) can be set up exactly; the `rtk` stub behaves like the real hook
//! or not, to exercise the smoke test.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Env {
    root: tempfile::TempDir,
    project: PathBuf,
    bin: PathBuf,
}

impl Env {
    /// A project initialized for Claude, with healthy stubs.
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        let project = r.join("proj");
        for d in ["proj", "home", "zf", "claude", "bin", "mcp"] {
            std::fs::create_dir_all(r.join(d)).unwrap();
        }
        let env = Self {
            bin: r.join("bin"),
            project,
            root,
        };
        env.stub(
            "claude",
            r#"[ "$1" = "--version" ] && { echo "2.1.0 (Claude Code)"; exit 0; }
if [ "$1" = "mcp" ] && [ "$2" = "get" ]; then
  f="$MCP_DIR/$3.txt"
  [ -f "$f" ] && { cat "$f"; exit 0; }
  echo "No MCP server named \"$3\"."; exit 1
fi
if [ "$1" = "plugin" ] && [ "$2" = "validate" ]; then
  f="$MCP_DIR/validate-$(basename "$3").txt"
  [ -f "$f" ] && { cat "$f"; exit 0; }
  echo "Validating components in: $3"; echo; echo "✔ Validation passed"; exit 0
fi
exit 0"#,
        );
        env.stub(
            "codegraph",
            r#"[ "$1" = "init" ] && mkdir -p .codegraph; exit 0"#,
        );
        env.healthy_rtk();
        let out = env.zforge(&["init", "--agent", "claude", "--no-install", "--force"]);
        assert!(
            out.status.success(),
            "init: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        env
    }

    fn stub(&self, name: &str, body: &str) {
        let path = self.bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// `rtk hook claude` that performs the documented rewrite.
    fn healthy_rtk(&self) {
        self.stub(
            "rtk",
            r#"if [ "$1" = "hook" ]; then
  cat >/dev/null
  echo '{"hookSpecificOutput":{"updatedInput":{"command":"rtk git status"}}}'
fi
exit 0"#,
        );
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn mcp(&self, server: &str, status: &str, args: &str) {
        std::fs::write(
            self.dir("mcp").join(format!("{server}.txt")),
            format!("{server}:\n  Scope: Local config\n  Status: {status}\n  Type: stdio\n  Args: {args}\n"),
        )
        .unwrap();
    }

    fn root_arg(&self) -> String {
        self.project.canonicalize().unwrap().display().to_string()
    }

    /// Write Claude's global state file with these per-project trust flags.
    fn trust(&self, entries: &[(&Path, bool)]) {
        let projects: serde_json::Map<String, Value> = entries
            .iter()
            .map(|(p, ok)| {
                (
                    p.canonicalize().unwrap().display().to_string(),
                    serde_json::json!({ "hasTrustDialogAccepted": ok }),
                )
            })
            .collect();
        std::fs::write(
            self.dir("claude").join(".claude.json"),
            serde_json::json!({ "projects": projects }).to_string(),
        )
        .unwrap();
    }

    fn user_settings(&self, json: &str) {
        std::fs::write(self.dir("claude").join("settings.json"), json).unwrap();
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.project)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("HOME", self.dir("home"))
            .env("ZFORGE_HOME", self.dir("zf"))
            .env("CLAUDE_CONFIG_DIR", self.dir("claude"))
            .env("MCP_DIR", self.dir("mcp"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    /// `doctor --json` → (exit code, check name → check).
    fn doctor(&self) -> (i32, serde_json::Map<String, Value>) {
        let out = self.zforge(&["doctor", "--json"]);
        let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "doctor --json not JSON ({e}):\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        let checks = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["name"].as_str().unwrap().to_string(), c.clone()))
            .collect();
        (out.status.code().unwrap(), checks)
    }
}

fn level<'a>(checks: &'a serde_json::Map<String, Value>, name: &str) -> &'a str {
    checks[name]["level"].as_str().unwrap()
}

/// Everything wired: every check reaches the highest level it can confirm.
#[test]
fn healthy_claude_setup() {
    let env = Env::new();
    let root = env.root_arg();
    env.mcp("zforge", "✔ Connected", "mcp");
    env.mcp(
        "codegraph",
        "✔ Connected",
        &format!("serve --mcp --path {root}"),
    );
    let script = env.dir("home").join("caveman-activate.js");
    std::fs::write(&script, "").unwrap();
    env.user_settings(&format!(
        r#"{{"hooks":{{"PreToolUse":[{{"matcher":"Bash","hooks":[{{"type":"command","command":"rtk hook claude"}}]}}],
           "SessionStart":[{{"hooks":[{{"type":"command","command":"node {}"}}]}}]}}}}"#,
        script.display()
    ));

    let (code, c) = env.doctor();
    assert_eq!(code, 0);
    assert_eq!(level(&c, "claude"), "working");
    assert_eq!(level(&c, "runner"), "configured");
    assert_eq!(level(&c, "agents"), "recognized");
    assert_eq!(level(&c, "skills"), "recognized");
    assert_eq!(level(&c, "zforge mcp"), "working");
    assert_eq!(level(&c, "codegraph mcp"), "working");
    assert_eq!(level(&c, "rtk hook"), "working");
    assert_eq!(level(&c, "caveman hook"), "configured");
    // What cannot be confirmed is said, not assumed.
    assert!(c["agents"]["not_checked"]
        .as_str()
        .unwrap()
        .contains("inside a session"));
    assert!(c["skills"]["not_checked"]
        .as_str()
        .unwrap()
        .contains("model run"));
}

#[test]
fn missing_claude_fails_the_run() {
    let env = Env::new();
    std::fs::remove_file(env.bin.join("claude")).unwrap();
    let (code, c) = env.doctor();
    assert_eq!(code, 1, "a required check failed");
    assert_eq!(level(&c, "claude"), "missing");
}

#[test]
fn a_broken_agent_definition_fails_the_run() {
    let env = Env::new();
    let f = env.project.join(".claude/agents/review-agent.md");
    let body = std::fs::read_to_string(&f)
        .unwrap()
        .replace("name: review-agent", "name: x");
    std::fs::write(&f, body).unwrap();

    let (code, c) = env.doctor();
    assert_eq!(code, 1);
    assert_eq!(level(&c, "agents"), "broken");
    assert!(c["agents"]["detail"]
        .as_str()
        .unwrap()
        .contains("review-agent"));
}

/// A registered file is not a working server: absent, pending approval and
/// pinned to another project are each reported for what they are.
#[test]
fn mcp_states_are_distinguished() {
    let env = Env::new();
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "zforge mcp"), "missing");

    env.mcp(
        "zforge",
        "⏸ Pending approval (run `claude` to approve)",
        "mcp",
    );
    env.mcp(
        "codegraph",
        "✔ Connected",
        "serve --mcp --path /some/other/project",
    );
    let (code, c) = env.doctor();
    assert_eq!(code, 0, "optional checks do not fail the run");
    assert_eq!(level(&c, "zforge mcp"), "recognized");
    assert_eq!(level(&c, "codegraph mcp"), "broken");
    assert!(c["codegraph mcp"]["detail"]
        .as_str()
        .unwrap()
        .contains("not pinned"));

    env.mcp(
        "codegraph",
        "✗ Failed to connect",
        &format!("serve --mcp --path {}", env.root_arg()),
    );
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "codegraph mcp"), "broken");
}

/// rtk installed is not rtk working: the hook must be registered, and must
/// actually rewrite a command.
#[test]
fn rtk_hook_levels() {
    let env = Env::new();
    let (_, c) = env.doctor();
    assert_eq!(
        level(&c, "rtk hook"),
        "present",
        "installed but not registered"
    );

    env.user_settings(
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}"#,
    );
    env.stub("rtk", r#"[ "$1" = "hook" ] && cat >/dev/null; exit 0"#); // registered, does nothing
    let (_, c) = env.doctor();
    assert_eq!(
        level(&c, "rtk hook"),
        "broken",
        "the smoke test must catch a no-op hook"
    );

    env.healthy_rtk();
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "rtk hook"), "working");
}

#[test]
fn caveman_hook_with_a_missing_script_is_broken() {
    let env = Env::new();
    env.user_settings(
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node /nowhere/caveman-activate.js"}]}]}}"#,
    );
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "caveman hook"), "broken");
}

#[test]
fn text_report_names_fixes() {
    let env = Env::new();
    let out = env.zforge(&["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("zforge doctor — Claude Code"));
    assert!(
        text.contains("fix: zforge mcp register --agent claude"),
        "{text}"
    );
}

/// IMP-004: native skills. Claude's validator is authoritative for zforge's
/// skills; the user's own skills in the same directory are not zforge's
/// business.
#[test]
fn native_skill_checks() {
    let env = Env::new();
    let skills = env.project.join(".claude/skills");
    assert!(skills.join("zforge-review-patch/SKILL.md").is_file());

    std::fs::write(
        env.dir("mcp").join("validate-skills.txt"),
        format!(
            "Validating components in: x\n\nValidating skill: {}/mine/SKILL.md\n\n  ❯ frontmatter: No frontmatter block found.\n\n✔ Validation passed with warnings\n",
            skills.display()
        ),
    )
    .unwrap();
    let (_, c) = env.doctor();
    assert_eq!(
        level(&c, "skills"),
        "recognized",
        "user skills do not count against zforge"
    );

    std::fs::write(
        env.dir("mcp").join("validate-skills.txt"),
        format!(
            "Validating skill: {}/zforge-debug/SKILL.md\n\n  ❯ description: No description in frontmatter.\n\n✔ Validation passed with warnings\n",
            skills.display()
        ),
    )
    .unwrap();
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "skills"), "broken");
    assert!(c["skills"]["detail"]
        .as_str()
        .unwrap()
        .contains("zforge-debug"));

    std::fs::remove_dir_all(skills.join("zforge-review-patch")).unwrap();
    let (code, c) = env.doctor();
    assert_eq!(level(&c, "skills"), "broken");
    assert_eq!(
        level(&c, "agents"),
        "broken",
        "review-agent preloads the missing skill"
    );
    assert_eq!(code, 1);
}

/// A `claude` that cannot validate is not a clean validation.
#[test]
fn a_validator_that_does_not_run_does_not_count_as_recognized() {
    let env = Env::new();
    std::fs::write(env.dir("mcp").join("validate-skills.txt"), "").unwrap();
    std::fs::write(env.dir("mcp").join("validate-agents.txt"), "").unwrap();
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "skills"), "configured");
    assert_eq!(level(&c, "agents"), "configured");
}

/// Found by the benchmark: in an untrusted workspace `claude -p` ignores the
/// project's `permissions.allow`, so zforge's allowlist does nothing for
/// runs without a TTY. Trust set on a parent directory covers the project
/// (checked against Claude Code 2.1.278).
#[test]
fn workspace_trust_levels() {
    let env = Env::new();

    let (code, c) = env.doctor();
    assert_eq!(
        level(&c, "workspace trust"),
        "missing",
        "no Claude state yet"
    );
    assert_eq!(
        code, 0,
        "trust is optional: headless runs bypass permissions"
    );

    env.trust(&[(&env.project, true)]);
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "workspace trust"), "configured");

    env.trust(&[(env.root.path(), true)]);
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "workspace trust"), "configured");
    assert!(c["workspace trust"]["detail"]
        .as_str()
        .unwrap()
        .contains("parent"));

    env.trust(&[(&env.project, false), (&env.dir("home"), true)]);
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "workspace trust"), "broken");
    let fix = c["workspace trust"]["fix"].as_str().unwrap();
    assert!(fix.contains("trust dialog"), "{fix}");
    assert!(c["workspace trust"]["detail"]
        .as_str()
        .unwrap()
        .contains("permissions.allow"));
}

/// v1.5 Mốc B: runs whose worker is gone, and worktrees left behind.
#[test]
fn run_health_is_reported() {
    let env = Env::new();
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "runs"), "configured");
    assert_eq!(c["runs"]["detail"], "no runs yet");

    let meta = |id: &str| zforge::run::record::RunMeta {
        id: id.to_string(),
        created_at: chrono::Utc::now(),
        intake: "F".into(),
        handover: "HANDOVER-001".into(),
        task: "TASK-001".into(),
        manifest_sha256: "m".into(),
        worktree: env.project.join(".zforge/worktrees").join(id),
        branch: format!("zforge/TASK-001/{id}"),
        baseline_commit: "c".into(),
        budget_usd: 1.0,
        max_iterations: 2,
        retry_of: None,
        start: None,
        kind: Default::default(),
        checks: None,
    };
    let run = zforge::run::record::create(&env.project, meta).unwrap();
    let mut gone = std::process::Command::new("true").spawn().unwrap();
    let dead_pid = gone.id();
    gone.wait().unwrap();
    run.append(&zforge::run::record::RunEvent::Started {
        at: chrono::Utc::now(),
        pid: dead_pid,
    })
    .unwrap();

    let (code, c) = env.doctor();
    assert_eq!(level(&c, "runs"), "broken");
    let detail = c["runs"]["detail"].as_str().unwrap();
    assert!(detail.contains("worker gone but not recorded"), "{detail}");
    assert!(c["runs"]["fix"]
        .as_str()
        .unwrap()
        .contains("zforge run status RUN-001"));
    assert_eq!(code, 0, "an optional check does not fail the run");

    // Finished run holding a worktree: worth cleaning, not broken.
    run.append(&zforge::run::record::RunEvent::Cancelled {
        at: chrono::Utc::now(),
        reason: "user".into(),
    })
    .unwrap();
    std::fs::create_dir_all(env.project.join(".zforge/worktrees/RUN-001")).unwrap();
    let (_, c) = env.doctor();
    assert_eq!(level(&c, "runs"), "working");
    assert!(c["runs"]["detail"]
        .as_str()
        .unwrap()
        .contains("still hold a worktree"));
    assert!(c["runs"]["fix"]
        .as_str()
        .unwrap()
        .contains("run clean RUN-001"));
}
