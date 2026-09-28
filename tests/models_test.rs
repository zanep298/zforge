//! `zforge models`: the user, not zforge, picks each phase's model.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("proj");
        let home = dir.path().canonicalize().unwrap().join("zf");
        let agents = root.join(".zforge/agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        for phase in ["spec", "testspec", "plan", "code", "review"] {
            let src = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("templates/agents/{phase}-agent.md"));
            std::fs::copy(src, agents.join(format!("{phase}-agent.md"))).unwrap();
        }
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n\
             paths:\n  agents: ./.zforge/agents\n  skills: ./.zforge/skills\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".claude/agents")).unwrap();
        std::fs::create_dir_all(root.join(".codex/agents")).unwrap();
        Self {
            _dir: dir,
            root,
            home,
        }
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.zforge(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn rows(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["models", "--json"])).unwrap()
    }

    fn agent(&self, client: &str, phase: &str) -> String {
        std::fs::read_to_string(self.root.join(format!(".{client}/agents/{phase}-agent.md")))
            .unwrap()
    }
}

fn row(rows: &serde_json::Value, client: &str, phase: &str) -> (serde_json::Value, String) {
    let r = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["client"] == client && r["phase"] == phase)
        .unwrap();
    (r["model"].clone(), r["from"].as_str().unwrap().to_string())
}

#[test]
fn nothing_is_chosen_until_the_user_chooses() {
    let p = Project::new();
    let rows = p.rows();
    assert_eq!(
        row(&rows, "claude", "code"),
        (serde_json::Value::Null, "client default".into())
    );

    p.ok(&["models", "set", "code", "sonnet"]);
    let rows = p.rows();
    assert_eq!(
        row(&rows, "claude", "code"),
        ("sonnet".into(), "project".into())
    );
    let yaml = std::fs::read_to_string(p.root.join(".zforge/models.yaml")).unwrap();
    assert_eq!(yaml, "claude:\n  code: sonnet\n");
    // The agent definitions follow at once; the rest keep the default.
    assert!(p.agent("claude", "code").contains("\nmodel: sonnet\n"));
    assert!(p.agent("claude", "plan").contains("\nmodel: inherit\n"));
    assert!(
        !p.agent("codex", "code").contains("model:"),
        "codex chose nothing"
    );

    // For every project, below the project's own choice.
    p.ok(&["models", "set", "all", "opus", "--global"]);
    let rows = p.rows();
    assert_eq!(
        row(&rows, "claude", "plan"),
        ("opus".into(), "global".into())
    );
    assert_eq!(
        row(&rows, "claude", "code"),
        ("sonnet".into(), "project".into())
    );
    assert!(p.agent("claude", "plan").contains("\nmodel: opus\n"));

    p.ok(&["models", "unset", "code"]);
    assert_eq!(
        row(&p.rows(), "claude", "code"),
        ("opus".into(), "global".into())
    );
    assert!(p.agent("claude", "code").contains("\nmodel: opus\n"));

    p.ok(&["models", "set", "code", "gpt-5.5", "--client", "codex"]);
    assert!(p.agent("codex", "code").contains("\nmodel: gpt-5.5\n"));
}

#[test]
fn bad_input_changes_nothing() {
    let p = Project::new();
    let out = p.zforge(&["models", "set", "deploy", "opus"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown phase `deploy`"));
    let out = p.zforge(&["models", "set", "code", "x", "--client", "a b"]);
    assert!(!out.status.success());
    assert!(!p.root.join(".zforge/models.yaml").exists());
}
