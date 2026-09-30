//! The user's decisions typed to Claude Code (`/accept`, `/revise`,
//! `/handover`), recorded by the `UserPromptSubmit` hook `zforge hook
//! prompt` — the real binary, fed the JSON Claude Code sends.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TASK: &str = "---\nid: TASK-001\nparent: F\nrequirements: [REQ-001]\ndepends_on: []\n---\n\n# TASK-001\n\n\
    ## Goal\nadd returns the sum.\n## Input\nlib.sh.\n## Output\nadd is right.\n## Constraints\nOnly lib.sh.\n\
    ## Autonomy\nAny fix.\n## Acceptance and verification\n- AC-01: sh test.sh passes\n## Delivery\nLocal.\n\
    ## Amend the contract when\nA test must change.\n## Open questions\n";

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Project {
    /// A git repository with intake `F`: four stages and TASK-001, none
    /// sent for review yet.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let (root, home) = (base.join("proj"), base.join("zf"));
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: shell\n  test_command: \"sh test.sh\"\n\
             execution:\n  budget_usd: 1.0\n",
        )
        .unwrap();
        std::fs::write(root.join("test.sh"), "echo ok\n").unwrap();
        std::fs::write(root.join(".gitignore"), ".zforge/\n").unwrap();
        // The runner readiness looks for: `sh` is on every PATH.
        std::fs::write(
            home.join("registry.yaml"),
            "agents:\n  claude:\n    command: sh\n",
        )
        .unwrap();
        let p = Self {
            _dir: dir,
            root,
            home,
        };
        p.git(&["init", "-q", "-b", "main", "."]);
        p.git(&["add", "-A"]);
        p.git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ]);
        p.intake("F");
        p
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    fn intake(&self, id: &str) {
        self.zforge(&["intake", "new", id]);
        let d = self.dir(id);
        let files = [
            (
                "01-outcome.md",
                "# F\n\n## Requirements\n\n- REQ-001: add returns the sum\n\n## Open questions\n",
            ),
            (
                "02-behavior.md",
                "# B\n\n## Situations\nREQ-001: add 2 3 = 5.\n\n## Open questions\n",
            ),
            (
                "03-solution.md",
                "# S\n\n## Flow\nFix the sum.\n\n## Open questions\n",
            ),
            (
                "04-breakdown.md",
                "# K\n\n## Tasks\nTASK-001\n\n## Integration verification\nsh test.sh\n\n## Open questions\n",
            ),
            ("tasks/TASK-001.md", TASK),
        ];
        for (f, text) in files {
            std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
            std::fs::write(d.join(f), text).unwrap();
        }
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(".zforge/intakes").join(id)
    }

    fn zforge(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "zforge {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn review(&self, id: &str, files: &[&str]) {
        for f in files {
            self.zforge(&["intake", "review", id, f]);
        }
    }

    /// Run the hook as Claude Code would for a message the user typed.
    fn say(&self, prompt: &str) -> Option<Value> {
        hook(&self.root, &self.home, prompt)
    }

    fn decisions(&self, id: &str) -> Vec<Value> {
        std::fs::read_to_string(self.dir(id).join(".records/decisions.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn state(&self, id: &str, file: &str) -> String {
        let v: Value =
            serde_json::from_str(&self.zforge(&["intake", "status", id, "--json"])).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == file)
            .map(|f| f["state"].as_str().unwrap().to_string())
            .unwrap()
    }
}

fn hook(cwd: &Path, home: &Path, prompt: &str) -> Option<Value> {
    let input = json!({
        "session_id": "sess-1",
        "transcript_path": "/dev/null",
        "cwd": cwd,
        "hook_event_name": "UserPromptSubmit",
        "prompt": prompt,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_zforge"))
        .args(["hook", "prompt"])
        .current_dir(cwd)
        .env("ZFORGE_HOME", home)
        .env("USER", "alice")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    (!stdout.trim().is_empty()).then(|| serde_json::from_str(stdout.trim()).unwrap())
}

fn user_sees(v: &Value) -> &str {
    v["systemMessage"].as_str().unwrap()
}

fn model_sees(v: &Value) -> &str {
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
}

const ALL: [&str; 5] = [
    "01-outcome.md",
    "02-behavior.md",
    "03-solution.md",
    "04-breakdown.md",
    "tasks/TASK-001.md",
];

#[test]
fn other_messages_pass_through() {
    let p = Project::new();
    p.review("F", &ALL);
    for prompt in ["looks good, accept it", "accept all", "/help"] {
        assert_eq!(p.say(prompt), None, "{prompt}");
    }
    assert!(p.decisions("F").iter().all(|d| d["decision"] == "review"));
}

#[test]
fn accept_all_records_every_file_under_review_through_the_prompt() {
    let p = Project::new();
    p.review("F", &ALL);

    let v = p.say("/accept all").expect("an answer");
    for f in ALL {
        assert_eq!(p.state("F", f), "accepted", "{f}");
        assert!(
            user_sees(&v).contains(&format!("✓ accepted {f} rev 1")),
            "{v}"
        );
    }
    let accepted: Vec<Value> = p
        .decisions("F")
        .into_iter()
        .filter(|d| d["decision"] == "accepted")
        .collect();
    assert_eq!(accepted.len(), ALL.len());
    for d in &accepted {
        assert_eq!(d["channel"], "claude-prompt");
        assert_eq!(d["by"], "alice");
        assert_eq!(d["session"], "sess-1");
    }
    let model = model_sees(&v);
    assert!(model.contains("you did not record it"), "{model}");
    assert!(model.contains("Next for this intake:"), "{model}");
}

#[test]
fn accept_and_revise_name_files_the_short_way() {
    let p = Project::new();
    p.review("F", &ALL);

    p.say("/accept 01 02-behavior").unwrap();
    assert_eq!(p.state("F", "01-outcome.md"), "accepted");
    assert_eq!(p.state("F", "02-behavior.md"), "accepted");
    assert_eq!(p.state("F", "03-solution.md"), "in_review");

    let v = p.say("/revise TASK-001: split AC-01 in two").unwrap();
    assert_eq!(p.state("F", "tasks/TASK-001.md"), "needs_revision");
    assert!(user_sees(&v).contains("changes requested to tasks/TASK-001.md"));
    let last = p.decisions("F").pop().unwrap();
    assert_eq!(last["note"], "split AC-01 in two");
    assert_eq!(last["channel"], "claude-prompt");

    // Without `:`, the note starts after the file.
    p.say("/revise F 03 needs a diagram").unwrap();
    let last = p.decisions("F").pop().unwrap();
    assert_eq!(last["file"], "03-solution.md");
    assert_eq!(last["note"], "needs a diagram");
}

/// What was shown for review is what gets accepted: a file edited since is
/// refused, the others are still recorded.
#[test]
fn a_file_changed_after_review_is_not_accepted() {
    let p = Project::new();
    p.review("F", &ALL);
    let d = p.dir("F");
    let text = std::fs::read_to_string(d.join("03-solution.md")).unwrap();
    std::fs::write(d.join("03-solution.md"), text + "edited\n").unwrap();

    let v = p.say("/accept all").unwrap();
    assert!(
        user_sees(&v).contains("✗ 03-solution.md: 03-solution.md changed after revision 1"),
        "{v}"
    );
    assert_eq!(p.state("F", "01-outcome.md"), "accepted");
    assert_ne!(p.state("F", "03-solution.md"), "accepted");
}

#[test]
fn a_decision_that_names_nothing_clear_records_nothing() {
    let p = Project::new();
    p.intake("G");
    p.review("F", &["01-outcome.md"]);
    p.review("G", &["01-outcome.md"]);

    let v = p.say("/accept all").unwrap();
    assert!(user_sees(&v).starts_with("zforge: nothing recorded"), "{v}");
    assert!(user_sees(&v).contains("F, G"), "{v}");
    assert!(model_sees(&v).contains("recorded nothing"), "{v}");
    assert_eq!(p.state("F", "01-outcome.md"), "in_review");

    // Naming the intake settles it.
    p.say("/accept G all").unwrap();
    assert_eq!(p.state("G", "01-outcome.md"), "accepted");
    assert_eq!(p.state("F", "01-outcome.md"), "in_review");

    let v = p.say("/revise TASK-001").unwrap();
    assert!(user_sees(&v).contains("say what needs to change"), "{v}");
}

#[test]
fn handover_records_the_manifest_through_the_prompt() {
    let p = Project::new();
    let v = p.say("/handover").unwrap();
    assert!(user_sees(&v).contains("not ready to hand over"), "{v}");
    assert!(!p.dir("F").join(".records/handovers").exists());

    p.review("F", &ALL);
    p.say("/accept all").unwrap();
    let v = p.say("/handover").unwrap();
    assert!(
        user_sees(&v).contains("✓ HANDOVER-001 recorded: TASK-001"),
        "{v}"
    );
    assert!(model_sees(&v).contains("run_start"), "{v}");
    let m: Value = serde_json::from_str(
        &std::fs::read_to_string(p.dir("F").join(".records/handovers/HANDOVER-001.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(m["channel"], "claude-prompt");
    assert_eq!(m["session"], "sess-1");
    assert_eq!(m["files"].as_array().unwrap().len(), ALL.len());

    // Handed over as it is: a second one only when asked by name.
    let v = p.say("/handover").unwrap();
    assert!(
        user_sees(&v).contains("F is already handed over as it is (HANDOVER-001)"),
        "{v}"
    );
    let v = p.say("/handover F").unwrap();
    assert!(user_sees(&v).contains("✓ HANDOVER-002"), "{v}");
}

#[test]
fn outside_a_project_the_decision_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let v = hook(dir.path(), dir.path(), "/accept all").unwrap();
    assert!(user_sees(&v).contains("not in a zforge project"), "{v}");
}
