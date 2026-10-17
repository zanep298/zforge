//! Shared stakeholder questions through the real binary: `zforge questions
//! new|status`, a citation lint refuses at review, and MCP
//! `questions_status`.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();
        Self { _dir: dir, root }
    }

    fn zforge(&self, args: &[&str]) -> Output {
        self.zforge_with(args, None)
    }

    fn zforge_with(&self, args: &[&str], stdin: Option<&str>) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", self.root.join(".home"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = stdin {
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        child.wait_with_output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.zforge(args);
        assert!(
            out.status.success(),
            "zforge {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

#[test]
fn questions_are_created_cited_linted_and_listed() {
    let p = Project::new();
    let out = p.ok(&["questions", "new", "REF-V1"]);
    assert!(out.contains("Q:REF-V1/Q-1"), "{out}");
    let path = p.root.join(".zforge/questions/REF-V1.md");
    assert!(path.is_file());
    assert!(!p.zforge(&["questions", "new", "REF-V1"]).status.success());
    assert!(!p.zforge(&["questions", "new", "../x"]).status.success());

    std::fs::write(
        &path,
        "## Questions\n\n- [ ] Q-1: archived counts?\n- [x] Q-2: who sees it? => the referrer\n",
    )
    .unwrap();

    p.ok(&["intake", "new", "F-1"]);
    let outcome = p.root.join(".zforge/intakes/F-1/01-outcome.md");
    std::fs::write(
        &outcome,
        "# F-1\n\n## Requirements\n\n- REQ-001: count referrals (Q:REF-V1/Q-1, Q:REF-V1/Q-9)\n\n## Open questions\n",
    )
    .unwrap();
    let out = p.zforge(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cites Q:REF-V1/Q-9, which .zforge/questions/REF-V1.md does not define"),
        "{err}"
    );

    std::fs::write(
        &outcome,
        "# F-1\n\n## Requirements\n\n- REQ-001: count referrals (Q:REF-V1/Q-1)\n\n## Open questions\n",
    )
    .unwrap();
    p.ok(&["intake", "review", "F-1", "01-outcome.md"]);

    let status: Value = serde_json::from_str(&p.ok(&["questions", "status", "--json"])).unwrap();
    assert_eq!(status[0]["name"], "REF-V1");
    assert_eq!(status[0]["open"], 1);
    assert_eq!(status[0]["answered"], 1);
    assert_eq!(status[0]["questions"][0]["id"], "Q-1");
    assert_eq!(status[0]["questions"][0]["answered"], false);
    assert_eq!(
        status[0]["questions"][0]["cited_by"],
        json!([{"intake": "F-1", "file": "01-outcome.md"}])
    );
    assert_eq!(status[0]["questions"][1]["answer"], "the referrer");

    let text = p.ok(&["questions", "status", "REF-V1"]);
    assert!(text.contains("1 open, 1 answered"), "{text}");
    assert!(
        text.contains("[ ] Q-1: archived counts?  — cited by F-1/01-outcome.md"),
        "{text}"
    );
    assert!(!p.zforge(&["questions", "status", "NOPE"]).status.success());

    // Over MCP: the same report, read-only.
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "questions_status", "arguments": { "name": "REF-V1" } }
    });
    let out = p.zforge_with(&["mcp"], Some(&format!("{request}\n")));
    let frame: Value = serde_json::from_str(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap(),
    )
    .unwrap();
    let report: Value =
        serde_json::from_str(frame["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report[0]["open"], 1);
}
