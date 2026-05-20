use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::mcp_register::Agent;

const DEFAULT_CONFIG: &str = r#"project:
  name: ""
  language: "rust"
  test_command: "cargo test"
  root_dir: "."
opencode:
  model: "claude-sonnet-4-6"
  context_files: []
paths:
  tasks: "./.zforge/tasks"
  agents: "./.zforge/agents"
  memory: "./.zforge/memory"
  skills: "./.zforge/skills"
review:
  auto_approve: false
"#;

/// Config written by `init --shared` — points agents/skills at the global
/// `~/.zforge/` store so multiple projects share a single source of truth.
/// Tasks and memory stay local because they are project-specific.
const SHARED_CONFIG: &str = r#"project:
  name: ""
  language: "rust"
  test_command: "cargo test"
  root_dir: "."
opencode:
  model: "claude-sonnet-4-6"
  context_files: []
paths:
  tasks: "./.zforge/tasks"
  agents: "~/.zforge/agents"
  memory: "./.zforge/memory"
  skills: "~/.zforge/skills"
review:
  auto_approve: false
"#;

const PATTERNS_MD: &str = "# Coding Patterns\n\n## Approved Patterns\n\n## Test Patterns\n";
const GLOSSARY_MD: &str = "# Domain Glossary\n";
const ANTI_PATTERNS_MD: &str = "# Anti-Patterns\n";

const CLAUDE_SETTINGS_JSON: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(zforge *)",
      "mcp__zforge__task_import",
      "mcp__zforge__get_prompt",
      "mcp__zforge__approve",
      "mcp__zforge__verify",
      "mcp__zforge__status",
      "mcp__codegraph__query",
      "mcp__codegraph__context",
      "mcp__codegraph__files",
      "mcp__codegraph__affected"
    ]
  }
}
"#;

const FALLBACK_CODING_STYLE: &str = "# Coding Style\n\n\
- Prefer immutability: create new values, don't mutate in place\n\
- Functions under 50 lines; files under 800 lines\n\
- Handle errors explicitly — no silent swallowing\n\
- Validate at system boundaries (user input, external APIs)\n\
- No magic numbers; use named constants\n";

const FALLBACK_TESTING: &str = "# Testing\n\n\
- Minimum 80% test coverage\n\
- Write tests first (TDD): RED → GREEN → REFACTOR\n\
- Use AAA pattern: Arrange → Act → Assert\n\
- Descriptive test names that explain the scenario\n\
- Unit tests alongside source; integration tests in separate directory\n";

const FALLBACK_SECURITY: &str = "# Security\n\n\
- Never hardcode secrets — use environment variables\n\
- Validate all user input at boundaries\n\
- Use parameterized queries to prevent SQL injection\n\
- Never expose internal errors to end users\n\
- Run security audits as part of CI (`cargo audit`, etc.)\n";

struct InitStats {
    created: usize,
    skipped: usize,
}

impl InitStats {
    fn new() -> Self {
        Self {
            created: 0,
            skipped: 0,
        }
    }

    fn record(&mut self, created: bool) {
        if created {
            self.created += 1;
        } else {
            self.skipped += 1;
        }
    }
}

pub fn run(agent: Agent, force: bool, local: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let detected = detect_project(&cwd);

    let want_claude = matches!(agent, Agent::Claude | Agent::All);
    let want_codex = matches!(agent, Agent::Codex | Agent::All);
    let want_opencode = matches!(agent, Agent::OpenCode | Agent::All);

    let agent_label = match agent {
        Agent::All => "all (claude, codex, opencode)",
        Agent::Claude => "claude",
        Agent::Codex => "codex",
        Agent::OpenCode => "opencode",
    };
    let mode_label = if local { " (local mode)" } else { "" };
    println!(
        "Scaffolding zforge for: {}{}",
        agent_label.cyan(),
        mode_label.dimmed()
    );

    // Default is shared mode: templates + agents + skills live in the
    // global ~/.zforge/ store and are referenced by symlink. `--local`
    // forks every file into the project. First-time install of the global
    // store happens automatically when shared.
    let global_store = if !local {
        Some(crate::cli::install::ensure_global_store()?)
    } else {
        None
    };

    let zforge_dir = cwd.join(".zforge");

    if zforge_dir.exists() && !force {
        print!(
            "{} .zforge/ already exists. Overwrite? [y/N] ",
            "⚠".yellow()
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let mut stats = InitStats::new();

    // Config — default (shared) mode points paths at ~/.zforge/{agents,skills}.
    // `--local` keeps everything inside the project at ./.zforge/.
    let base_config = if local { DEFAULT_CONFIG } else { SHARED_CONFIG };
    let config_content = base_config
        .replace(
            "language: \"rust\"",
            &format!("language: \"{}\"", detected.language),
        )
        .replace(
            "test_command: \"cargo test\"",
            &format!("test_command: \"{}\"", detected.test_command),
        );
    let created = write_safe(&zforge_dir.join("config.yaml"), &config_content, force)?;
    stats.record(created);
    print_file_status(created, ".zforge/config.yaml");

    let created = write_safe(
        &zforge_dir.join("models.yaml"),
        include_str!("../../templates/models.yaml"),
        force,
    )?;
    stats.record(created);
    print_file_status(created, ".zforge/models.yaml");

    let lang_skills = lang_skill_templates(&detected.language);
    let vars = Vars {
        language: detected.language.clone(),
        test_command: detected.test_command,
        project_name: detected.project_name,
    };

    // Local mode forks every template/agent/skill into the project so
    // each file can be customized standalone. Shared mode (default) skips
    // these writes; the global store at ~/.zforge/ already holds them.
    if local {
        // Prompt templates (.tmpl — used by zf spec/testspec/plan/code/verify/review)
        let tmpl_dir = zforge_dir.join("agents");
        std::fs::create_dir_all(&tmpl_dir)?;
        for (name, content) in prompt_templates() {
            let created = write_safe(&tmpl_dir.join(name), content, force)?;
            stats.record(created);
        }
        println!(
            "{} .zforge/agents/ — prompt templates",
            label(stats.created > 0)
        );

        // Agent definitions (.md — loaded by Claude Code via .claude/agents/ symlinks)
        for (name, raw) in agent_templates() {
            let rendered = apply_vars(raw, &vars);
            let created = write_safe(&tmpl_dir.join(name), &rendered, force)?;
            stats.record(created);
        }
        println!(
            "{} .zforge/agents/ — Claude Code agent definitions",
            label(stats.created > 0)
        );

        // Skills (workflow)
        let skills_dir = zforge_dir.join("skills");
        std::fs::create_dir_all(&skills_dir)?;
        for (name, raw) in skill_templates() {
            let rendered = apply_vars(raw, &vars);
            let created = write_safe(&skills_dir.join(name), &rendered, force)?;
            stats.record(created);
        }

        // Skills (language-specific)
        let lang_count = lang_skills.len();
        for (name, raw) in &lang_skills {
            let rendered = apply_vars(raw, &vars);
            let created = write_safe(&skills_dir.join(name), &rendered, force)?;
            stats.record(created);
        }
        println!(
            "{} .zforge/skills/ — {} base/supplementary + {} {} skills",
            label(stats.created > 0),
            SKILLS.len(),
            lang_count,
            vars.language
        );
    } else {
        println!(
            "{} agents + skills resolved from {}",
            "↪".cyan(),
            "~/.zforge/".dimmed()
        );
    }

    // Memory
    let mem_dir = zforge_dir.join("memory");
    std::fs::create_dir_all(&mem_dir)?;
    for (name, content) in [
        ("patterns.md", PATTERNS_MD),
        ("domain-glossary.md", GLOSSARY_MD),
        ("anti-patterns.md", ANTI_PATTERNS_MD),
    ] {
        let created = write_safe(&mem_dir.join(name), content, force)?;
        stats.record(created);
    }
    println!("{} .zforge/memory/ — 3 files", label(stats.created > 0));

    // .zforge/README.md
    let readme = apply_vars(include_str!("../../templates/zforge-readme.md"), &vars);
    let created = write_safe(&zforge_dir.join("README.md"), &readme, force)?;
    stats.record(created);
    print_file_status(created, ".zforge/README.md");

    let lang_skills_section = build_lang_skills_section(&vars.language, &lang_skills);
    let vars_with_lang = VarsExt {
        vars: &vars,
        lang_skills_section: &lang_skills_section,
    };

    // .zforge/tasks/ (always)
    let tasks_dir = zforge_dir.join("tasks");
    std::fs::create_dir_all(&tasks_dir)?;
    println!("{} .zforge/tasks/", "✓".green());

    // --- Claude Code scaffolding --------------------------------------------
    if want_claude {
        scaffold_claude(
            &cwd,
            &zforge_dir,
            &vars_with_lang,
            force,
            global_store.as_deref(),
            &mut stats,
        )?;
    }

    // --- Codex CLI scaffolding ----------------------------------------------
    if want_codex {
        scaffold_codex(
            &cwd,
            &zforge_dir,
            &vars,
            &vars_with_lang,
            force,
            global_store.as_deref(),
            &mut stats,
        )?;
    }

    // --- OpenCode scaffolding -----------------------------------------------
    if want_opencode {
        scaffold_opencode(
            &cwd,
            &zforge_dir,
            &vars_with_lang,
            force,
            global_store.as_deref(),
            &mut stats,
        )?;
    }

    setup_codegraph(&cwd);

    println!();
    println!(
        "  {} created, {} skipped (already existed)",
        stats.created, stats.skipped
    );
    println!();
    println!("Next steps:");
    println!("  1. Edit .zforge/config.yaml — set project.name");
    if want_claude {
        println!("  2. Verify CLAUDE.md — project details correct");
        println!("     Register Claude Code MCP: zforge mcp register --agent claude");
    }
    if want_codex {
        println!("  2. Verify AGENTS.md — project details correct");
        println!("     (Codex MCP + profiles were registered automatically above)");
    }
    if want_opencode {
        println!("  2. Verify AGENTS.md — project details correct");
        println!("     (OpenCode MCP was registered automatically above)");
    }
    println!("  3. Run: zforge task import TASK-123");
    if !local {
        println!();
        println!(
            "  {} Templates + skills served from {}",
            "ℹ".cyan(),
            "~/.zforge/".dimmed()
        );
        println!("     Refresh with: {}", "zforge install --force".cyan());
        println!(
            "     Drop {} or pass --local for project-local copies.",
            ".zforge/agents/<name>.tmpl".cyan()
        );
    }

    Ok(())
}

// --- codegraph setup ---

fn codegraph_is_installed() -> bool {
    std::process::Command::new("codegraph")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn setup_codegraph(cwd: &Path) {
    println!();
    println!("Setting up codegraph…");

    if !codegraph_is_installed() {
        println!("  Installing @colbymchenry/codegraph via npm…");
        let status = std::process::Command::new("npm")
            .args(["install", "-g", "@colbymchenry/codegraph"])
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("{} codegraph installed", "✓".green());
            }
            Ok(s) => {
                eprintln!(
                    "  {} npm install exited with {s} — skipping codegraph init",
                    "⚠".yellow()
                );
                return;
            }
            Err(e) => {
                eprintln!(
                    "  {} npm not found ({e}) — skipping codegraph init",
                    "⚠".yellow()
                );
                return;
            }
        }
    } else {
        println!("{} codegraph already installed", "–".dimmed());
    }

    let status = std::process::Command::new("codegraph")
        .arg("init")
        .current_dir(cwd)
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("{} codegraph init — .codegraph/ created", "✓".green());
        }
        Ok(s) => {
            eprintln!("  {} codegraph init exited with {s}", "⚠".yellow());
            return;
        }
        Err(e) => {
            eprintln!("  {} codegraph init failed: {e}", "⚠".yellow());
            return;
        }
    }

    println!("  Indexing codebase…");
    let status = std::process::Command::new("codegraph")
        .args(["index", "--quiet"])
        .current_dir(cwd)
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("{} codegraph index — codebase indexed", "✓".green());
        }
        Ok(s) => {
            eprintln!("  {} codegraph index exited with {s}", "⚠".yellow());
        }
        Err(e) => {
            eprintln!("  {} codegraph index failed: {e}", "⚠".yellow());
        }
    }

    register_codegraph_mcp(cwd);
}

fn register_codegraph_mcp(cwd: &Path) {
    let mcp_path = cwd.join(".mcp.json");
    let mut root: serde_json::Value = if mcp_path.exists() {
        match std::fs::read_to_string(&mcp_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            Some(v) => v,
            None => serde_json::json!({}),
        }
    } else {
        serde_json::json!({})
    };

    let servers = root
        .as_object_mut()
        .and_then(|o| {
            if !o.contains_key("mcpServers") {
                o.insert(
                    "mcpServers".to_string(),
                    serde_json::json!({}),
                );
            }
            o.get_mut("mcpServers")?.as_object_mut()
        });

    if let Some(servers) = servers {
        if servers.contains_key("codegraph") {
            println!("{} .mcp.json — codegraph already registered", "–".dimmed());
            return;
        }
        servers.insert(
            "codegraph".to_string(),
            serde_json::json!({
                "command": "codegraph",
                "args": ["serve", "--mcp"]
            }),
        );
    }

    match serde_json::to_string_pretty(&root) {
        Ok(content) => match std::fs::write(&mcp_path, content + "\n") {
            Ok(_) => println!("{} .mcp.json — codegraph MCP registered", "✓".green()),
            Err(e) => eprintln!("  {} write .mcp.json failed: {e}", "⚠".yellow()),
        },
        Err(e) => eprintln!("  {} serialize .mcp.json failed: {e}", "⚠".yellow()),
    }
}

// --- per-agent scaffolders ---

fn scaffold_claude(
    cwd: &Path,
    zforge_dir: &Path,
    vars_with_lang: &VarsExt<'_>,
    force: bool,
    global_store: Option<&Path>,
    stats: &mut InitStats,
) -> Result<()> {
    // CLAUDE.md at project root — auto-loaded by Claude Code
    let claude_md = apply_vars_ext(include_str!("../../templates/CLAUDE.md"), vars_with_lang);
    let created = write_safe(&cwd.join("CLAUDE.md"), &claude_md, force)?;
    stats.record(created);
    print_file_status(created, "CLAUDE.md");
    if !created {
        eprintln!(
            "  {} CLAUDE.md already exists — not overwritten. Run with --force to replace.",
            "⚠".yellow()
        );
    }

    // .claude/settings.json
    let claude_dir = cwd.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");
    let created = write_safe(&settings_path, CLAUDE_SETTINGS_JSON, force)?;
    stats.record(created);
    print_file_status(created, ".claude/settings.json");

    // .claude/agents/ — symlinks to the agent store (global or project-local)
    let claude_agents_dir = claude_dir.join("agents");
    std::fs::create_dir_all(&claude_agents_dir)?;
    let (agent_count, target_label) =
        symlink_agents_into(zforge_dir, &claude_agents_dir, global_store, force)?;
    println!(
        "{} .claude/agents/ — {} symlinks → {}",
        label(agent_count > 0),
        agent_count,
        target_label
    );

    // .claude/rules/
    let claude_rules_dir = claude_dir.join("rules");
    std::fs::create_dir_all(&claude_rules_dir)?;
    let rule_count = install_rules_for_claude(&claude_rules_dir, force, stats)?;
    println!(
        "{} .claude/rules/ — {} rule files",
        label(rule_count > 0),
        rule_count
    );

    Ok(())
}

fn scaffold_codex(
    cwd: &Path,
    zforge_dir: &Path,
    vars: &Vars,
    vars_with_lang: &VarsExt<'_>,
    force: bool,
    global_store: Option<&Path>,
    stats: &mut InitStats,
) -> Result<()> {
    // AGENTS.md at project root — auto-loaded by Codex CLI
    let agents_md = apply_vars_ext(include_str!("../../templates/AGENTS.md"), vars_with_lang);
    let created = write_safe(&cwd.join("AGENTS.md"), &agents_md, force)?;
    stats.record(created);
    print_file_status(created, "AGENTS.md");
    if !created {
        eprintln!(
            "  {} AGENTS.md already exists — not overwritten. Run with --force to replace.",
            "⚠".yellow()
        );
    }

    // .codex/agents/ — symlinks to the agent store (global or project-local)
    let codex_dir = cwd.join(".codex");
    std::fs::create_dir_all(&codex_dir)?;
    let codex_agents_dir = codex_dir.join("agents");
    std::fs::create_dir_all(&codex_agents_dir)?;
    let (codex_agent_count, target_label) =
        symlink_agents_into(zforge_dir, &codex_agents_dir, global_store, force)?;
    println!(
        "{} .codex/agents/ — {} symlinks → {}",
        label(codex_agent_count > 0),
        codex_agent_count,
        target_label
    );

    // .codex/README.md
    let codex_readme = apply_vars(include_str!("../../templates/codex-readme.md"), vars);
    let created = write_safe(&codex_dir.join("README.md"), &codex_readme, force)?;
    stats.record(created);
    print_file_status(created, ".codex/README.md");

    // Auto-register Codex MCP server + profiles
    println!();
    println!("Registering Codex MCP server…");
    if let Err(e) = crate::cli::mcp_register::run(Agent::Codex, force) {
        eprintln!(
            "  {} Codex MCP registration reported errors: {e}",
            "⚠".yellow()
        );
    }
    let codex_agents_source = global_store
        .map(|g| g.join("agents"))
        .unwrap_or_else(|| zforge_dir.join("agents"));
    if let Err(e) = crate::cli::mcp_register::write_codex_profiles(&codex_agents_source) {
        eprintln!(
            "  {} Codex profile write reported errors: {e}",
            "⚠".yellow()
        );
    } else {
        println!(
            "{} ~/.codex/config.toml — profiles: {}",
            "✓".green(),
            crate::cli::mcp_register::PHASES
                .iter()
                .map(|p| format!("zforge_{p}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    Ok(())
}

fn scaffold_opencode(
    cwd: &Path,
    zforge_dir: &Path,
    vars_with_lang: &VarsExt<'_>,
    force: bool,
    global_store: Option<&Path>,
    stats: &mut InitStats,
) -> Result<()> {
    // AGENTS.md at project root — OpenCode also reads AGENTS.md
    let agents_md = apply_vars_ext(include_str!("../../templates/AGENTS.md"), vars_with_lang);
    let created = write_safe(&cwd.join("AGENTS.md"), &agents_md, force)?;
    stats.record(created);
    print_file_status(created, "AGENTS.md");
    if !created {
        eprintln!(
            "  {} AGENTS.md already exists — not overwritten. Run with --force to replace.",
            "⚠".yellow()
        );
    }

    // .opencode/agents/ — symlinks to the agent store (global or project-local)
    let opencode_dir = cwd.join(".opencode");
    std::fs::create_dir_all(&opencode_dir)?;
    let opencode_agents_dir = opencode_dir.join("agents");
    std::fs::create_dir_all(&opencode_agents_dir)?;
    let (opencode_agent_count, target_label) =
        symlink_agents_into(zforge_dir, &opencode_agents_dir, global_store, force)?;
    println!(
        "{} .opencode/agents/ — {} symlinks → {}",
        label(opencode_agent_count > 0),
        opencode_agent_count,
        target_label
    );

    // Auto-register OpenCode MCP server in ~/.config/opencode/opencode.json
    println!();
    println!("Registering OpenCode MCP server…");
    if let Err(e) = crate::cli::mcp_register::run(Agent::OpenCode, force) {
        eprintln!(
            "  {} OpenCode MCP registration reported errors: {e}",
            "⚠".yellow()
        );
    }

    Ok(())
}

// --- helpers ---

pub(crate) fn label(any_created: bool) -> colored::ColoredString {
    if any_created {
        "✓".green()
    } else {
        "–".dimmed()
    }
}

pub(crate) fn print_file_status(created: bool, path: &str) {
    if created {
        println!("{} {}", "✓".green(), path);
    } else {
        println!("{} {} (skipped)", "–".dimmed(), path);
    }
}

/// Write content to path. Returns true if written, false if skipped.
/// Skips when file exists and force is false.
pub(crate) fn write_safe(path: &Path, content: &str, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(true)
}

pub(crate) struct Vars {
    pub(crate) language: String,
    pub(crate) test_command: String,
    pub(crate) project_name: String,
}

// Project-type detection lives in init::detect.
use detect::detect_project;

// Per-language skill templates live in init::lang_skills.
use lang_skills::{build_lang_skills_section, lang_skill_templates};

mod detect;
mod lang_skills;
mod registry;

pub(crate) fn apply_vars(template: &str, vars: &Vars) -> String {
    template
        .replace("{{language}}", &vars.language)
        .replace("{{test_command}}", &vars.test_command)
        .replace("{{project_name}}", &vars.project_name)
    // leave {{task_id}} and other runtime vars as-is
}

struct VarsExt<'a> {
    vars: &'a Vars,
    lang_skills_section: &'a str,
}

fn apply_vars_ext(template: &str, v: &VarsExt<'_>) -> String {
    apply_vars(template, v.vars).replace("{{lang_skills_section}}", v.lang_skills_section)
}

const AGENT_NAMES: [&str; 5] = [
    "spec-agent.md",
    "testspec-agent.md",
    "plan-agent.md",
    "code-agent.md",
    "review-agent.md",
];

/// Create symlinks in `link_dir` pointing to `*.md` files in `zforge_agents_dir`.
/// `.zforge/agents/` is the single source of truth — agent harnesses (Claude Code,
/// Codex CLI, …) read via symlinks under their own project directory.
///
/// `link_dir` is expected at depth 2 from the project root (e.g. `.claude/agents/`,
/// `.codex/agents/`) so the relative target `../../.zforge/agents/` resolves correctly.
fn symlink_zforge_agents(_zforge_agents_dir: &Path, link_dir: &Path, force: bool) -> Result<usize> {
    let mut count = 0;
    for name in AGENT_NAMES {
        let link = link_dir.join(name);
        let exists = link.exists() || link.symlink_metadata().is_ok();
        if exists {
            if force {
                std::fs::remove_file(&link)?;
            } else {
                continue;
            }
        }
        let target = Path::new("../../.zforge/agents").join(name);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        #[cfg(not(unix))]
        {
            // Windows: fall back to copy
            let src = _zforge_agents_dir.join(name);
            if src.exists() {
                std::fs::copy(&src, &link)?;
            }
        }
        count += 1;
    }
    Ok(count)
}

/// Create symlinks in `link_dir` pointing at the global agent store
/// (`~/.zforge/agents/`). Used by `init --shared` so multiple projects share
/// one set of agent definitions.
fn symlink_global_agents(global_agents_dir: &Path, link_dir: &Path, force: bool) -> Result<usize> {
    let mut count = 0;
    for name in AGENT_NAMES {
        let link = link_dir.join(name);
        let exists = link.exists() || link.symlink_metadata().is_ok();
        if exists {
            if force {
                std::fs::remove_file(&link)?;
            } else {
                continue;
            }
        }
        let target = global_agents_dir.join(name);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        #[cfg(not(unix))]
        {
            // Windows: fall back to copy from the absolute target
            if target.exists() {
                std::fs::copy(&target, &link)?;
            }
        }
        count += 1;
    }
    Ok(count)
}

/// Dispatch to the project-local or global symlink helper based on whether
/// the caller passed a `global_store` root. Returns `(count, label)` where
/// `label` is the human-readable target path printed in progress output.
fn symlink_agents_into(
    zforge_dir: &Path,
    link_dir: &Path,
    global_store: Option<&Path>,
    force: bool,
) -> Result<(usize, String)> {
    if let Some(global) = global_store {
        let agents = global.join("agents");
        let count = symlink_global_agents(&agents, link_dir, force)?;
        let label = match dirs::home_dir() {
            Some(home) => match agents.strip_prefix(&home) {
                Ok(rel) => format!("~/{}", rel.display()),
                Err(_) => agents.display().to_string(),
            },
            None => agents.display().to_string(),
        };
        Ok((count, label))
    } else {
        let count = symlink_zforge_agents(&zforge_dir.join("agents"), link_dir, force)?;
        Ok((count, ".zforge/agents/".to_string()))
    }
}

fn install_rules_for_claude(rules_dir: &Path, force: bool, stats: &mut InitStats) -> Result<usize> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let ecc_rules = home.join(".claude").join("rules");
    let mut count = 0;

    let rule_defs: &[(&str, &str)] = &[
        ("coding-style.md", FALLBACK_CODING_STYLE),
        ("testing.md", FALLBACK_TESTING),
        ("security.md", FALLBACK_SECURITY),
    ];

    for (filename, fallback) in rule_defs {
        let stem = filename.trim_end_matches(".md");
        let content = read_ecc_rule(&ecc_rules, stem).unwrap_or_else(|| fallback.to_string());
        let created = write_safe(&rules_dir.join(filename), &content, force)?;
        stats.record(created);
        if created {
            count += 1;
        }
    }
    Ok(count)
}

fn read_ecc_rule(ecc_rules: &Path, name: &str) -> Option<String> {
    // Try flat rule first: ~/.claude/rules/<name>.md
    let flat = ecc_rules.join(format!("{name}.md"));
    if let Ok(c) = std::fs::read_to_string(&flat) {
        return Some(c);
    }
    // Try common/ subdirectory
    let common = ecc_rules.join("common").join(format!("{name}.md"));
    std::fs::read_to_string(common).ok()
}

// Embedded template arrays live in init::registry.
use registry::{agent_templates, prompt_templates, skill_templates, SKILLS};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_vars() -> Vars {
        Vars {
            language: "rust".into(),
            test_command: "cargo test".into(),
            project_name: "demo".into(),
        }
    }

    #[test]
    fn agents_md_template_renders_project_vars() {
        let vars = sample_vars();
        let ext = VarsExt {
            vars: &vars,
            lang_skills_section: "",
        };
        let rendered = apply_vars_ext(include_str!("../../templates/AGENTS.md"), &ext);
        assert!(rendered.starts_with("# demo\n"));
        assert!(rendered.contains("**Language:** rust"));
        assert!(rendered.contains("`cargo test`"));
        // Codex-specific anchors must survive substitution
        assert!(rendered.contains("OpenAI Codex CLI"));
        assert!(rendered.contains("~/.codex/config.toml"));
        assert!(rendered.contains(".codex/agents/spec-agent.md"));
        assert!(!rendered.contains("{{"), "unsubstituted handlebar present");
    }

    #[test]
    fn codex_readme_template_renders_without_handlebars() {
        let vars = sample_vars();
        let rendered = apply_vars(include_str!("../../templates/codex-readme.md"), &vars);
        assert!(rendered.contains("`zforge mcp register --agent codex`"));
        assert!(rendered.contains("[mcp_servers.zforge]"));
        assert!(!rendered.contains("{{"));
    }

    #[test]
    fn symlink_zforge_agents_creates_relative_links_into_codex_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let zforge_agents = root.join(".zforge").join("agents");
        std::fs::create_dir_all(&zforge_agents).unwrap();
        for name in [
            "spec-agent.md",
            "testspec-agent.md",
            "plan-agent.md",
            "code-agent.md",
            "review-agent.md",
        ] {
            std::fs::write(zforge_agents.join(name), "stub\n").unwrap();
        }

        let codex_agents = root.join(".codex").join("agents");
        std::fs::create_dir_all(&codex_agents).unwrap();

        let count = symlink_zforge_agents(&zforge_agents, &codex_agents, false).unwrap();
        assert_eq!(count, 5);

        // Relative target resolves from .codex/agents/ back into .zforge/agents/
        #[cfg(unix)]
        {
            let link = codex_agents.join("spec-agent.md");
            let target = std::fs::read_link(&link).unwrap();
            assert_eq!(target, Path::new("../../.zforge/agents/spec-agent.md"));
            let resolved = std::fs::read_to_string(&link).unwrap();
            assert_eq!(resolved.trim(), "stub");
        }
    }

    #[test]
    fn symlink_zforge_agents_preserves_existing_link_targets() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let zforge_agents = root.join(".zforge").join("agents");
        std::fs::create_dir_all(&zforge_agents).unwrap();
        for name in [
            "spec-agent.md",
            "testspec-agent.md",
            "plan-agent.md",
            "code-agent.md",
            "review-agent.md",
        ] {
            std::fs::write(zforge_agents.join(name), "stub\n").unwrap();
        }

        let codex_agents = root.join(".codex").join("agents");
        std::fs::create_dir_all(&codex_agents).unwrap();
        // Pre-existing user-edited file — must be left alone
        std::fs::write(codex_agents.join("spec-agent.md"), "preexisting\n").unwrap();

        let count = symlink_zforge_agents(&zforge_agents, &codex_agents, false).unwrap();
        assert_eq!(
            count, 4,
            "should symlink 4 missing agents, skip the existing one"
        );
        let still_there = std::fs::read_to_string(codex_agents.join("spec-agent.md")).unwrap();
        assert_eq!(still_there.trim(), "preexisting");
    }
}
