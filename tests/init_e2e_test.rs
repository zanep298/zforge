#![cfg(unix)]
//! `zforge init` end to end, per client and mode (FIX-012 … FIX-017).
//!
//! Before these, nothing exercised `init` as a whole: it installs tools over
//! the network and writes to global client config, so every init bug in the
//! backlog shipped unnoticed. Here the real binary runs with every config
//! location redirected into a temp dir, and with `PATH` holding only stubs
//! for claude / codex / opencode / rtk / codegraph that record their argv.
//! That checks what init *asks each tool to do*.
//!
//! Whether the real clients then *load* that configuration is a separate
//! question; it is covered by `real_clients_load_the_codegraph_registration`
//! (ignored by default, needs the real CLIs installed).

use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const CLIENTS: [&str; 3] = ["claude", "codex", "opencode"];

struct Env {
    root: tempfile::TempDir,
    project: PathBuf,
    stub_dir: PathBuf,
    log: PathBuf,
}

impl Env {
    /// `on_path`: which client stubs to install (rtk and codegraph always).
    fn new(on_path: &[&str]) -> Self {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        let project = r.join("proj");
        std::fs::create_dir_all(&project).unwrap();
        // A Rust project, so language skills are part of the references.
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        for d in ["home", "zf", "codex", "xdg", "claude"] {
            std::fs::create_dir_all(r.join(d)).unwrap();
        }
        let stub_dir = r.join("bin");
        std::fs::create_dir_all(&stub_dir).unwrap();
        let log = r.join("stub.log");
        let env = Self {
            root,
            project,
            stub_dir,
            log,
        };
        for name in on_path.iter().chain(&["rtk", "codegraph"]) {
            env.install_stub(name);
        }
        env
    }

    fn install_stub(&self, name: &str) {
        let path = self.stub_dir.join(name);
        // Records argv; answers --version; `codegraph init` creates the
        // index dir the way the real tool does.
        let body = format!(
            r#"#!/bin/sh
echo "{name} $*" >> "{log}"
[ "$1" = "--version" ] && {{ echo "{name} stub 0.0.0"; exit 0; }}
[ "{name}" = "codegraph" ] && [ "$1" = "init" ] && mkdir -p .codegraph
exit 0
"#,
            log = self.log.display()
        );
        std::fs::write(&path, body).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.project)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.stub_dir.display()))
            .env("HOME", self.dir("home"))
            .env("ZFORGE_HOME", self.dir("zf"))
            .env("CODEX_HOME", self.dir("codex"))
            .env("XDG_CONFIG_HOME", self.dir("xdg"))
            .env("CLAUDE_CONFIG_DIR", self.dir("claude"))
            .stdin(Stdio::null())
            .output()
            .expect("run zforge")
    }

    fn init(&self, extra: &[&str]) -> Output {
        let mut args = vec!["init", "--no-install", "--force"];
        args.extend_from_slice(extra);
        let out = self.zforge(&args);
        assert!(
            out.status.success(),
            "init {extra:?} failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn log_lines(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn clear_log(&self) {
        let _ = std::fs::remove_file(&self.log);
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.project.join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e}"))
    }

    fn config(&self) -> serde_yaml::Value {
        serde_yaml::from_str(&self.read(".zforge/config.yaml")).unwrap()
    }

    /// The project path as tools receive it (`--path`).
    fn root_arg(&self) -> String {
        self.project.canonicalize().unwrap().display().to_string()
    }
}

fn agent_flag(client: &str) -> Vec<&str> {
    vec!["--agent", client]
}

// ─── FIX-013: every skill reference resolves ────────────────────────────────

/// Skill files named in backticks in a generated instruction file.
fn skill_refs(text: &str) -> Vec<String> {
    Regex::new(r"`([^`\s]*skills/[^`\s]+\.md)`")
        .unwrap()
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect()
}

fn assert_all_skill_refs_resolve(env: &Env, file: &str) {
    let text = env.read(file);
    let refs = skill_refs(&text);
    assert!(
        refs.len() >= 15,
        "{file}: expected skill table, found {refs:?}"
    );
    let missing: Vec<&String> = refs
        .iter()
        .filter(|r| {
            let p = Path::new(r.as_str());
            let resolved = if p.is_absolute() {
                p.to_path_buf()
            } else {
                env.project.join(p)
            };
            !resolved.is_file()
        })
        .collect();
    assert!(
        missing.is_empty(),
        "{file}: {} of {} skill references do not exist: {missing:?}",
        missing.len(),
        refs.len()
    );
}

// The backlog repro: shared mode left 20/20 references dangling.
// (CLAUDE.md now names native skills instead — see the IMP-004 tests.)
#[test]
fn shared_mode_skill_references_all_resolve() {
    let env = Env::new(&CLIENTS);
    env.init(&["--agent", "all"]);
    assert_all_skill_refs_resolve(&env, "AGENTS.md");
    // Shared mode points at the store actually in use ($ZFORGE_HOME).
    assert!(env
        .read("AGENTS.md")
        .contains(&env.dir("zf").display().to_string()));
}

#[test]
fn local_mode_skill_references_all_resolve() {
    let env = Env::new(&CLIENTS);
    env.init(&["--agent", "all", "--local"]);
    assert_all_skill_refs_resolve(&env, "AGENTS.md");
}

// ─── IMP-004: native Claude skills ───────────────────────────────────────────

fn native_skill_names(env: &Env) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(env.project.join(".claude/skills"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn frontmatter(text: &str) -> serde_yaml::Value {
    let fm = text
        .strip_prefix("---\n")
        .and_then(|r| r.split("\n---\n").next())
        .unwrap();
    serde_yaml::from_str(fm).unwrap()
}

#[test]
fn claude_init_installs_native_skills_in_both_modes() {
    for local in [false, true] {
        let env = Env::new(&CLIENTS);
        let mut args = vec!["--agent", "claude"];
        if local {
            args.push("--local");
        }
        env.init(&args);
        let names = native_skill_names(&env);
        assert!(
            names.contains(&"zforge-clarify-spec".to_string()),
            "{names:?}"
        );
        assert!(
            names.contains(&"zforge-rust-patterns".to_string()),
            "language skills: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.contains("python")),
            "only this project's language"
        );
        for n in &names {
            let fm = frontmatter(&env.read(&format!(".claude/skills/{n}/SKILL.md")));
            assert_eq!(fm["name"].as_str(), Some(n.as_str()));
            assert!(!fm["description"].as_str().unwrap_or("").is_empty(), "{n}");
            assert_eq!(fm["metadata"]["generated-by"].as_str(), Some("zforge"));
        }
        // CLAUDE.md names every one of them.
        let claude_md = env.read("CLAUDE.md");
        for n in &names {
            assert!(
                claude_md.contains(&format!("`{n}`")),
                "CLAUDE.md misses {n}"
            );
        }
    }
}

/// The phase checklists are preloaded into the phase agents.
#[test]
fn phase_agents_preload_their_skills() {
    let env = Env::new(&CLIENTS);
    env.init(&agent_flag("claude"));
    let code = frontmatter(&env.read(".claude/agents/code-agent.md"));
    let skills: Vec<&str> = code["skills"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        skills,
        vec![
            "zforge-write-tests-first",
            "zforge-implement-minimal-patch",
            "zforge-rust-patterns",
            "zforge-rust-testing"
        ]
    );
    for s in skills {
        assert!(env
            .project
            .join(format!(".claude/skills/{s}/SKILL.md"))
            .is_file());
    }
    let spec = frontmatter(&env.read(".claude/agents/spec-agent.md"));
    assert_eq!(spec["skills"][0].as_str(), Some("zforge-clarify-spec"));
}

/// Stale zforge skills go; the user's own skills stay.
#[test]
fn refresh_removes_stale_zforge_skills_only() {
    let env = Env::new(&CLIENTS);
    env.init(&agent_flag("claude"));
    let skills = env.project.join(".claude/skills");
    std::fs::create_dir_all(skills.join("zforge-retired")).unwrap();
    std::fs::write(skills.join("zforge-retired/SKILL.md"), "old").unwrap();
    std::fs::create_dir_all(skills.join("my-own")).unwrap();
    std::fs::write(skills.join("my-own/SKILL.md"), "mine").unwrap();

    env.init(&agent_flag("claude"));

    assert!(!skills.join("zforge-retired").exists());
    assert_eq!(
        std::fs::read_to_string(skills.join("my-own/SKILL.md")).unwrap(),
        "mine"
    );
}

/// Codex / OpenCode setups do not get Claude skills.
#[test]
fn native_skills_are_claude_only() {
    let env = Env::new(&CLIENTS);
    env.init(&agent_flag("codex"));
    assert!(!env.project.join(".claude/skills").exists());
}

#[test]
fn shared_config_paths_point_at_the_store_in_use() {
    let env = Env::new(&["codex"]);
    env.init(&agent_flag("codex"));
    let cfg = env.config();
    let skills = cfg["paths"]["skills"].as_str().unwrap();
    assert_eq!(
        Path::new(skills),
        env.dir("zf").join("skills"),
        "config must name $ZFORGE_HOME, not a hardcoded ~/.zforge"
    );
}

// ─── FIX-015: default runner ─────────────────────────────────────────────────

fn runner(env: &Env) -> String {
    env.config()["runner"]["default"]
        .as_str()
        .unwrap()
        .to_string()
}

fn registered_agents(env: &Env) -> Vec<String> {
    let reg: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(env.dir("zf").join("registry.yaml")).unwrap(),
    )
    .unwrap();
    let mut names: Vec<String> = reg["agents"]
        .as_mapping()
        .unwrap()
        .keys()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn single_client_init_makes_it_the_default_runner_and_the_only_registered_agent() {
    for client in CLIENTS {
        let env = Env::new(&CLIENTS);
        env.init(&agent_flag(client));
        assert_eq!(runner(&env), client, "init --agent {client}");
        assert_eq!(
            registered_agents(&env),
            vec![client.to_string()],
            "init --agent {client} must not seed other agents (claude was always added)"
        );
    }
}

#[test]
fn all_picks_the_first_installed_client_in_order() {
    let env = Env::new(&CLIENTS);
    env.init(&["--agent", "all"]);
    assert_eq!(runner(&env), "claude");

    let env = Env::new(&["codex", "opencode"]);
    env.init(&["--agent", "all"]);
    assert_eq!(runner(&env), "codex", "claude is not installed");

    let env = Env::new(&["opencode"]);
    env.init(&["--agent", "all"]);
    assert_eq!(runner(&env), "opencode");
}

#[test]
fn default_runner_flag_overrides_and_is_validated() {
    let env = Env::new(&CLIENTS);
    env.init(&["--agent", "all", "--default-runner", "opencode"]);
    assert_eq!(runner(&env), "opencode");

    let out = env.zforge(&[
        "init",
        "--no-install",
        "--force",
        "--agent",
        "codex",
        "--default-runner",
        "claude",
    ]);
    assert!(!out.status.success(), "claude was not set up");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--agent claude"));
}

/// The backlog repro: codex-initialized project, task without `--agent`,
/// a non-interactive phase ran claude.
#[test]
fn agentless_task_runs_on_the_default_runner() {
    for client in CLIENTS {
        let env = Env::new(&CLIENTS);
        env.init(&agent_flag(client));
        let out = env.zforge(&["task", "import", "TASK-1", "--title", "demo task"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        env.clear_log();

        let out = env.zforge(&["spec", "TASK-1"]);
        assert!(
            out.status.success(),
            "spec on {client}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let invoked: Vec<String> = env
            .log_lines()
            .iter()
            .filter_map(|l| l.split_whitespace().next().map(str::to_string))
            .filter(|b| CLIENTS.contains(&b.as_str()))
            .collect();
        assert!(!invoked.is_empty(), "no client invoked for {client}");
        assert!(
            invoked.iter().all(|b| b == client),
            "project runner {client}, but invoked {invoked:?}"
        );
    }
}

// ─── FIX-014: the phase's named agent is selected ───────────────────────────

#[test]
fn claude_and_opencode_launch_the_phase_agent_definition() {
    for client in ["claude", "opencode"] {
        let env = Env::new(&CLIENTS);
        env.init(&agent_flag(client));
        env.zforge(&["task", "import", "TASK-1", "--title", "demo task"]);
        env.clear_log();
        env.zforge(&["spec", "TASK-1"]);
        let launch = env
            .log_lines()
            .into_iter()
            .find(|l| l.starts_with(client))
            .unwrap_or_else(|| panic!("{client} not launched"));
        assert!(
            launch.contains("--agent spec-agent"),
            "{client} launched without the spec-agent definition: {launch}"
        );
    }
}

// ─── FIX-016: rtk for the right client ───────────────────────────────────────

fn rtk_calls(env: &Env) -> Vec<String> {
    env.log_lines()
        .into_iter()
        .filter(|l| l.starts_with("rtk init"))
        .collect()
}

#[test]
fn rtk_is_set_up_for_each_scaffolded_client() {
    let cases: [(&str, &[&str]); 4] = [
        ("claude", &["rtk init -g"]),
        ("codex", &["rtk init -g --codex"]),
        ("opencode", &["rtk init -g --opencode"]),
        ("all", &["rtk init -g --opencode", "rtk init -g --codex"]),
    ];
    for (agent, expected) in cases {
        let env = Env::new(&CLIENTS);
        env.init(&["--agent", agent]);
        assert_eq!(rtk_calls(&env), expected.to_vec(), "--agent {agent}");
    }
}

// ─── FIX-012: CodeGraph registered natively per client ──────────────────────

fn claude_codegraph_add(env: &Env) -> Vec<String> {
    env.log_lines()
        .into_iter()
        .filter(|l| l.starts_with("claude mcp add"))
        .collect()
}

#[test]
fn codegraph_is_registered_for_every_client_in_both_modes() {
    for local in [false, true] {
        let env = Env::new(&CLIENTS);
        let mut args = vec!["--agent", "all"];
        if local {
            args.push("--local");
        }
        // Init twice: the second run must replace, not duplicate.
        env.init(&args);
        env.init(&args);
        let root = env.root_arg();

        // Claude: project-scoped via the CLI, pinned to this project.
        let adds = claude_codegraph_add(&env);
        assert_eq!(adds.len(), 2, "one add per init: {adds:?}");
        assert!(
            adds.iter().all(|a| a
                == &format!(
                    "claude mcp add --scope local codegraph -- codegraph serve --mcp --path {root}"
                )),
            "{adds:?}"
        );

        // Codex: project config, exactly one server block.
        let codex = env.read(".codex/config.toml");
        assert_eq!(
            codex.matches("[mcp_servers.codegraph]").count(),
            1,
            "{codex}"
        );
        assert!(
            codex.contains(&format!("\"--path\", \"{root}\"")),
            "{codex}"
        );

        // OpenCode: project config.
        let oc: Value = serde_json::from_str(&env.read("opencode.json")).unwrap();
        assert_eq!(
            oc["mcp"]["codegraph"]["command"],
            serde_json::json!(["codegraph", "serve", "--mcp", "--path", root])
        );

        // Claude's .mcp.json is no longer used for it.
        assert!(!env.project.join(".mcp.json").exists());
    }
}

#[test]
fn codegraph_is_only_registered_for_scaffolded_clients() {
    let env = Env::new(&CLIENTS);
    env.init(&agent_flag("opencode"));
    assert!(env.project.join("opencode.json").exists());
    assert!(!env.project.join(".codex/config.toml").exists());
    assert!(claude_codegraph_add(&env).is_empty());
}

#[test]
fn existing_project_client_config_is_merged_not_replaced() {
    let env = Env::new(&CLIENTS);
    std::fs::write(
        env.project.join("opencode.json"),
        r#"{"theme":"dark","mcp":{"mine":{"type":"local","command":["x"]}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(env.project.join(".codex")).unwrap();
    std::fs::write(
        env.project.join(".codex/config.toml"),
        "model = \"o3\"\n\n[mcp_servers.mine]\ncommand = \"x\"\n",
    )
    .unwrap();

    env.init(&["--agent", "all"]);

    let oc: Value = serde_json::from_str(&env.read("opencode.json")).unwrap();
    assert_eq!(oc["theme"], "dark");
    assert_eq!(oc["mcp"]["mine"]["command"][0], "x");
    assert!(oc["mcp"]["codegraph"].is_object());
    let codex = env.read(".codex/config.toml");
    assert!(codex.contains("model = \"o3\""));
    assert!(codex.contains("[mcp_servers.mine]"));
    assert!(codex.contains("[mcp_servers.codegraph]"));
}

#[test]
fn missing_codegraph_is_reported_not_registered() {
    let env = Env::new(&CLIENTS);
    std::fs::remove_file(env.stub_dir.join("codegraph")).unwrap();
    let out = env.init(&["--agent", "all"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("codegraph not installed"));
    assert!(!env.project.join("opencode.json").exists());
    assert!(!env.project.join(".codex/config.toml").exists());
    assert!(claude_codegraph_add(&env).is_empty());
}

// ─── FIX-017: refresh keeps user configuration ──────────────────────────────

#[test]
fn refresh_keeps_custom_models_and_renders_them() {
    let env = Env::new(&CLIENTS);
    env.init(&agent_flag("claude"));

    let models = env.project.join(".zforge/models.yaml");
    std::fs::write(&models, "claude:\n  spec: my-custom-model\n").unwrap();
    std::fs::write(
        env.project.join(".zforge/memory/patterns.md"),
        "# mine\n- keep: me\n",
    )
    .unwrap();

    env.init(&agent_flag("claude")); // --force refresh

    assert_eq!(
        std::fs::read_to_string(&models).unwrap(),
        "claude:\n  spec: my-custom-model\n",
        "models.yaml must survive init --force"
    );
    assert!(
        env.read(".claude/agents/spec-agent.md")
            .contains("my-custom-model"),
        "the refreshed agent definition must use the custom model"
    );
    assert!(env.read(".zforge/memory/patterns.md").contains("keep: me"));
}

#[test]
fn refresh_keeps_user_config_and_updates_runner() {
    let env = Env::new(&CLIENTS);
    env.init(&agent_flag("claude"));
    let cfg = env
        .read(".zforge/config.yaml")
        .replace("name: \"\"", "name: \"kept\"");
    std::fs::write(env.project.join(".zforge/config.yaml"), cfg).unwrap();

    env.init(&["--agent", "all", "--default-runner", "codex"]);
    let cfg = env.config();
    assert_eq!(cfg["project"]["name"], "kept");
    assert_eq!(cfg["runner"]["default"], "codex");
}

// ─── Real clients (opt-in) ──────────────────────────────────────────────────

/// Initializes with the *real* tools, then asks each real client whether it
/// sees the CodeGraph server. Needs claude, opencode and codegraph on PATH;
/// run with `cargo test --test init_e2e_test -- --ignored`. Codex is not
/// listed here: it only loads project config for trusted projects, which is
/// a user decision.
#[test]
#[ignore = "needs the real claude/opencode/codegraph CLIs"]
fn real_clients_load_the_codegraph_registration() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("a.js"), "function hello() { return 1 }\n").unwrap();
    let path = std::env::var("PATH").unwrap();
    std::fs::create_dir_all(root.path().join("home")).unwrap();
    let run = |bin: &str, args: &[&str]| -> String {
        let out = Command::new(bin)
            .args(args)
            .current_dir(&project)
            .env("PATH", &path)
            // Temp HOME: init also runs `rtk init -g`, which must not touch
            // the developer's real ~/.claude, ~/.codex or opencode config.
            .env("HOME", root.path().join("home"))
            .env("ZFORGE_HOME", root.path().join("zf"))
            .env("CODEX_HOME", root.path().join("codex"))
            .env("XDG_CONFIG_HOME", root.path().join("xdg"))
            .env("CLAUDE_CONFIG_DIR", root.path().join("claude"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    };
    run(
        env!("CARGO_BIN_EXE_zforge"),
        &[
            "init",
            "--agent",
            "all",
            "--no-install",
            "--force",
            "--no-register",
        ],
    );
    let claude = run("claude", &["mcp", "list"]);
    assert!(
        claude.contains("codegraph") && claude.contains("Connected"),
        "{claude}"
    );
    let opencode = run("opencode", &["mcp", "list"]);
    assert!(
        opencode.contains("codegraph") && opencode.contains("connected"),
        "{opencode}"
    );
}
