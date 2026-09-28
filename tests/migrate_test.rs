//! Moving from the task pipeline to v1.5: the global store on `zforge
//! install`, a project on `zforge migrate`. Real binary, temp HOME and
//! ZFORGE_HOME, a PATH without any AI client.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const V1_CONFIG: &str = "# my project\nproject:\n  name: app # shown in status\n  \
    language: rust\n  test_command: \"cargo test\"\n  root_dir: \".\"\nopencode:\n  \
    model: \"claude-sonnet-4-6\"\n  context_files: []\nrunner:\n  default: \"claude\"\n\
    paths:\n  tasks: \"./.zforge/tasks\"\n  agents: \"STORE/agents\"\n  \
    memory: \"./.zforge/memory\"\n  skills: \"STORE/skills\"\nreview:\n  auto_approve: false\n";

const V1_REGISTRY: &str = "projects: []\nagents:\n  claude:\n    command: claude\n    args: [\"-p\"]\n\
    fallback_policy:\n  max_retries: 2\n  cooldown_seconds: 30\n  retryable_exit_codes:\n  - 124\n  \
    retryable_stderr_patterns:\n  - (?i)rate.?limit\n  spawn_timeout_secs: 900\n";

struct Env {
    _dir: tempfile::TempDir,
    home: PathBuf,
    store: PathBuf,
    project: PathBuf,
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let env = Self {
            home: base.join("home"),
            store: base.join("home/.zforge"),
            project: base.join("proj"),
            _dir: dir,
        };
        std::fs::create_dir_all(&env.home).unwrap();
        std::fs::create_dir_all(&env.project).unwrap();
        env
    }

    /// A store as the task pipeline left it.
    fn v1_store(&self) {
        let s = &self.store;
        write(&s.join("agents/spec.tmpl"), "{{task_id}}\n");
        write(
            &s.join("agents/spec-agent.md"),
            "---\nname: spec-agent\n---\n",
        );
        write(
            &s.join("agents/code-agent.md"),
            "---\nname: code-agent\n---\nold\n",
        );
        write(&s.join("skills/clarify-spec.md"), "# old\n");
        write(&s.join("skills/review-patch.md"), "# review spec.md\n");
        write(&s.join("registry.yaml"), V1_REGISTRY);
        write(
            &s.join("models.yaml"),
            "claude:\n  plan: opus\n  code: sonnet\n",
        );
    }

    /// A Claude project as the task pipeline left it.
    fn v1_project(&self) {
        let p = &self.project;
        let store = self.store.display().to_string();
        write(
            &p.join(".zforge/config.yaml"),
            &V1_CONFIG.replace("STORE", &store),
        );
        write(
            &p.join(".zforge/models.yaml"),
            "# mine\nclaude:\n  spec: haiku\n  code: sonnet\n",
        );
        write(&p.join(".zforge/tasks/T1/task.md"), "# T1\n");
        write(&p.join(".zforge/memory/patterns.md"), "- keep: me\n");
        write(&p.join(".zforge/cost-log.jsonl"), "{}\n");
        write(
            &p.join(".claude/agents/spec-agent.md"),
            "---\nname: spec-agent\n---\n",
        );
        write(&p.join(".claude/commands/zforge.md"), "/zforge\n");
        write(
            &p.join(".claude/settings.json"),
            "{\"permissions\":{\"allow\":[\"Bash(make *)\",\"mcp__zforge__ship\"]}}",
        );
        write(&p.join("CLAUDE.md"), "# old CLAUDE with task import\n");
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("ZFORGE_HOME", &self.store)
            .env("CLAUDE_CONFIG_DIR", self.home.join(".claude"))
            .env("CODEX_HOME", self.home.join(".codex"))
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.zforge(args);
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    /// The only archive directory under `base`.
    fn archive(&self, base: &Path) -> PathBuf {
        let dirs: Vec<PathBuf> = std::fs::read_dir(base.join("v1-archive"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1, "{dirs:?}");
        dirs[0].clone()
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn install_migrates_a_v1_store_once() {
    let env = Env::new();
    env.v1_store();

    let out = env.ok(&["install"]);
    assert!(out.contains("migrated"), "{out}");
    let s = &env.store;
    let archive = env.archive(s);
    for gone in [
        "agents/spec.tmpl",
        "agents/spec-agent.md",
        "skills/clarify-spec.md",
    ] {
        assert!(!s.join(gone).exists(), "{gone} still in the store");
        assert!(archive.join(gone).exists(), "{gone} not archived");
    }
    // Files zforge still ships are brought up to date; the old copy is kept.
    assert!(read(&s.join("agents/code-agent.md")).contains("task contract"));
    assert_eq!(
        read(&archive.join("agents/code-agent.md")),
        "---\nname: code-agent\n---\nold\n"
    );
    assert!(read(&s.join("skills/review-patch.md")).contains("VERDICT"));
    // Registry: retry settings gone, the timeout kept.
    let reg = read(&s.join("registry.yaml"));
    assert!(
        !reg.contains("max_retries") && !reg.contains("retryable"),
        "{reg}"
    );
    assert!(reg.contains("spawn_timeout_secs: 900"), "{reg}");
    assert!(read(&archive.join("registry.yaml")).contains("max_retries"));
    assert_eq!(read(&s.join("models.yaml")), "claude:\n  code: sonnet\n");

    let again = env.ok(&["install"]);
    assert!(!again.contains("migrated"), "{again}");
    assert_eq!(env.archive(s), archive, "nothing more archived");
}

#[test]
fn migrate_shows_its_plan_and_changes_nothing_without_consent() {
    let env = Env::new();
    env.v1_project();

    let out = env.ok(&["migrate", "--dry-run"]);
    assert!(out.contains(".zforge/tasks"), "{out}");
    assert!(out.contains("paths.tasks"), "{out}");
    assert!(out.contains("regenerate for claude"), "{out}");

    let out = env.zforge(&["migrate"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"));

    assert!(env.project.join(".zforge/tasks/T1/task.md").exists());
    assert!(!env.project.join(".zforge/v1-archive").exists());
}

#[test]
fn migrate_moves_a_project_to_v15() {
    let env = Env::new();
    env.v1_store();
    env.v1_project();
    let p = &env.project;

    let out = env.ok(&["migrate", "--yes"]);
    assert!(out.contains("migrated to v1.5"), "{out}");
    let archive = env.archive(&p.join(".zforge"));

    // Task-pipeline records and files: archived, not deleted.
    for moved in [
        ".zforge/tasks/T1/task.md",
        ".zforge/memory/patterns.md",
        ".zforge/cost-log.jsonl",
        ".claude/agents/spec-agent.md",
        ".claude/commands/zforge.md",
    ] {
        assert!(!p.join(moved).exists(), "{moved} left in place");
        assert!(archive.join(moved).exists(), "{moved} not archived");
    }

    // Config: v1 keys gone, comments kept, execution added.
    let config = read(&p.join(".zforge/config.yaml"));
    assert!(
        config.starts_with("# my project\nproject:\n  name: app # shown in status\n"),
        "{config}"
    );
    for gone in ["root_dir", "opencode", "tasks:", "memory:", "auto_approve"] {
        assert!(!config.contains(gone), "{gone}: {config}");
    }
    assert!(config.contains("execution:"), "{config}");
    assert!(read(&archive.join(".zforge/config.yaml")).contains("auto_approve"));
    assert_eq!(
        read(&p.join(".zforge/models.yaml")),
        "# mine\nclaude:\n  code: sonnet\n"
    );

    // Regenerated for Claude; the old instruction file kept in the archive.
    assert!(read(&p.join("CLAUDE.md")).contains("zforge intake new"));
    assert_eq!(
        read(&archive.join("CLAUDE.md")),
        "# old CLAUDE with task import\n"
    );
    assert!(p.join(".claude/agents/code-agent.md").is_file());
    assert!(p.join(".claude/agents/review-agent.md").is_file());
    let settings = read(&p.join(".claude/settings.json"));
    assert!(settings.contains("Bash(make *)"), "user entries kept");
    assert!(!settings.contains("mcp__zforge__ship"), "{settings}");
    assert!(settings.contains("mcp__zforge__status"), "{settings}");

    // The global store came along through init.
    assert!(!env.store.join("agents/spec.tmpl").exists());

    let again = env.ok(&["migrate", "--yes"]);
    assert!(again.contains("nothing to migrate"), "{again}");
    let status: serde_json::Value = serde_json::from_str(&env.ok(&["status", "--json"])).unwrap();
    assert!(status.get("needs_migration").is_none(), "{status}");
}

#[test]
fn status_points_a_v1_project_at_migrate() {
    let env = Env::new();
    env.v1_project();
    let status: serde_json::Value = serde_json::from_str(&env.ok(&["status", "--json"])).unwrap();
    assert_eq!(status["needs_migration"], true);
    assert!(env.ok(&["status"]).contains("zforge migrate"));
}
