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

pub fn run(
    agent: Agent,
    force: bool,
    local: bool,
    no_register: bool,
    name: Option<String>,
    switch: bool,
) -> Result<()> {
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

    setup_rtk();
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

    if !no_register {
        // Seed default AgentSpec rows for the agents the user enabled so the
        // orchestrator can resolve them without manual registry.yaml edits.
        let wanted: Vec<&str> = {
            let mut v = vec!["claude"];
            if want_codex {
                v.push("codex");
            }
            if want_opencode {
                v.push("opencode");
            }
            v
        };
        match crate::registry::auto::ensure_default_agents(&wanted) {
            Ok(inserted) if !inserted.is_empty() => println!(
                "registered default agents in registry.yaml: {}",
                inserted.join(", ")
            ),
            Ok(_) => {}
            Err(e) => eprintln!("warning: default-agent seeding failed: {e:#}"),
        }
        match crate::registry::auto::auto_register(&cwd, name.as_deref(), switch) {
            Ok(crate::registry::auto::AutoResult::Registered { name }) => {
                println!("registered project {name} in ~/.zforge/registry.yaml");
            }
            Ok(crate::registry::auto::AutoResult::AlreadyExists { name }) => {
                println!("project {name} already registered");
            }
            Ok(crate::registry::auto::AutoResult::Suffixed {
                requested,
                final_name,
            }) => {
                eprintln!(
                    "warning: name {requested:?} already in registry; registered as {final_name:?}"
                );
            }
            Ok(crate::registry::auto::AutoResult::Updated { old_name, new_name }) => {
                println!("renamed registry entry: {old_name} -> {new_name}");
            }
            Err(e) => {
                eprintln!("warning: registry update failed: {e:#}");
            }
        }
    }

    Ok(())
}

// --- rtk setup ---

fn rtk_is_installed() -> bool {
    std::process::Command::new("rtk")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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

fn setup_rtk() {
    println!();
    println!("Setting up rtk…");

    if !rtk_is_installed() {
        // Try brew first, fall back to curl install script
        let brew = std::process::Command::new("brew")
            .args(["install", "rtk"])
            .status();
        match brew {
            Ok(s) if s.success() => println!("{} rtk installed via brew", "✓".green()),
            _ => {
                println!("  brew unavailable, trying curl installer…");
                let curl = std::process::Command::new("sh")
                    .args(["-c", "curl -fsSL https://raw.githubusercontent.com/rtk-ai/rtk/refs/heads/master/install.sh | sh"])
                    .status();
                match curl {
                    Ok(s) if s.success() => println!("{} rtk installed via curl", "✓".green()),
                    Ok(s) => {
                        eprintln!("  {} rtk install exited with {s}", "⚠".yellow());
                        return;
                    }
                    Err(e) => {
                        eprintln!("  {} rtk install failed: {e}", "⚠".yellow());
                        return;
                    }
                }
            }
        }
    } else {
        println!("{} rtk already installed", "–".dimmed());
    }

    // rtk init -g wires Claude Code hooks globally
    let init = std::process::Command::new("rtk")
        .args(["init", "-g"])
        .status();
    match init {
        Ok(s) if s.success() => {
            println!("{} rtk init -g — Claude Code hooks configured", "✓".green())
        }
        Ok(s) => eprintln!("  {} rtk init exited with {s}", "⚠".yellow()),
        Err(e) => eprintln!("  {} rtk init failed: {e}", "⚠".yellow()),
    }
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

    let servers = root.as_object_mut().and_then(|o| {
        if !o.contains_key("mcpServers") {
            o.insert("mcpServers".to_string(), serde_json::json!({}));
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

    // .claude/agents/ — per-target rendered copies (model: line resolved from
    // models.yaml when present, falling back to template frontmatter).
    let claude_agents_dir = claude_dir.join("agents");
    std::fs::create_dir_all(&claude_agents_dir)?;
    let (agent_count, target_label) = materialize_agents_for(
        "claude",
        cwd,
        zforge_dir,
        &claude_agents_dir,
        global_store,
        force,
    )?;
    println!(
        "{} .claude/agents/ — {} files (model resolved per claude) ← {}",
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

    // .claude/commands/ — slash commands (e.g. /zforge <TASK-ID>)
    let claude_commands_dir = claude_dir.join("commands");
    std::fs::create_dir_all(&claude_commands_dir)?;
    let mut cmd_count = 0usize;
    for (name, content) in command_templates() {
        let created = write_safe(&claude_commands_dir.join(name), content, force)?;
        stats.record(created);
        if created {
            cmd_count += 1;
        }
    }
    println!(
        "{} .claude/commands/ — {} slash commands",
        label(cmd_count > 0),
        command_templates().len()
    );

    // caveman — terse output mode (saves ~65% output tokens)
    ensure_caveman();

    Ok(())
}

fn ensure_caveman() {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let activate = home
        .join(".claude")
        .join("hooks")
        .join("caveman-activate.js");
    if activate.exists() {
        println!("{} caveman already installed", "–".dimmed());
        return;
    }

    println!();
    println!("Installing caveman (terse output mode)…");

    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm https://raw.githubusercontent.com/JuliusBrussee/caveman/main/install.ps1 | iex",
        ])
        .status();

    #[cfg(not(target_os = "windows"))]
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg("curl -fsSL https://raw.githubusercontent.com/JuliusBrussee/caveman/main/install.sh | bash")
        .status();

    let manual = if cfg!(target_os = "windows") {
        "irm https://raw.githubusercontent.com/JuliusBrussee/caveman/main/install.ps1 | iex"
    } else {
        "curl -fsSL https://raw.githubusercontent.com/JuliusBrussee/caveman/main/install.sh | bash"
    };

    match status {
        Ok(s) if s.success() => {
            println!("{} caveman installed", "✓".green());
        }
        Ok(s) => {
            eprintln!(
                "  {} caveman install exited {s}. Run manually: {manual}",
                "⚠".yellow()
            );
        }
        Err(e) => {
            eprintln!(
                "  {} caveman install failed: {e}. Run manually: {manual}",
                "⚠".yellow()
            );
        }
    }
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

    // .codex/agents/ — per-target rendered copies. `model:` is resolved
    // from models.yaml → frontmatter `codex_model:` → default gpt-5-codex.
    let codex_dir = cwd.join(".codex");
    std::fs::create_dir_all(&codex_dir)?;
    let codex_agents_dir = codex_dir.join("agents");
    std::fs::create_dir_all(&codex_agents_dir)?;
    let (codex_agent_count, target_label) = materialize_agents_for(
        "codex",
        cwd,
        zforge_dir,
        &codex_agents_dir,
        global_store,
        force,
    )?;
    println!(
        "{} .codex/agents/ — {} files (model resolved per codex) ← {}",
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
    match crate::cli::mcp_register::write_codex_profiles(&codex_agents_source) {
        Err(e) => eprintln!(
            "  {} Codex profile write reported errors: {e}",
            "⚠".yellow()
        ),
        Ok(written) if written.is_empty() => eprintln!(
            "  {} Codex profile write produced no profiles (no agent templates found).",
            "⚠".yellow()
        ),
        Ok(written) => {
            let profiles = written
                .iter()
                .map(|(phase, model)| format!("zforge_{phase}={model}"))
                .collect::<Vec<_>>()
                .join(", ");
            println!("{} ~/.codex/config.toml — {}", "✓".green(), profiles);
        }
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

    // .opencode/agents/ — per-target rendered copies. `model:` resolved from
    // models.yaml → frontmatter `opencode_model:` → default claude-sonnet-4-6.
    let opencode_dir = cwd.join(".opencode");
    std::fs::create_dir_all(&opencode_dir)?;
    let opencode_agents_dir = opencode_dir.join("agents");
    std::fs::create_dir_all(&opencode_agents_dir)?;
    let (opencode_agent_count, target_label) = materialize_agents_for(
        "opencode",
        cwd,
        zforge_dir,
        &opencode_agents_dir,
        global_store,
        force,
    )?;
    println!(
        "{} .opencode/agents/ — {} files (model resolved per opencode) ← {}",
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

mod agent_render;
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

/// Render per-target agent files into `dst_dir` from the appropriate source
/// agents directory (global store at `~/.zforge/agents/` when present, else
/// project-local `.zforge/agents/`). Each file gets a single `model:` line
/// resolved from `models.yaml` → template frontmatter → hard default.
///
/// Replaces the older symlink-based approach (`symlink_agents_into`) for
/// agent-CLI directories so each tool reads its own model rather than seeing
/// all three `model:` / `codex_model:` / `opencode_model:` keys at once.
fn materialize_agents_for(
    target_agent: &str,
    cwd: &Path,
    zforge_dir: &Path,
    dst_dir: &Path,
    global_store: Option<&Path>,
    force: bool,
) -> Result<(usize, String)> {
    let models = crate::config::load_models_from_root(cwd);
    let (src_dir, label): (PathBuf, String) = if let Some(global) = global_store {
        let agents = global.join("agents");
        let label = match dirs::home_dir() {
            Some(home) => match agents.strip_prefix(&home) {
                Ok(rel) => format!("~/{}", rel.display()),
                Err(_) => agents.display().to_string(),
            },
            None => agents.display().to_string(),
        };
        (agents, label)
    } else {
        (zforge_dir.join("agents"), ".zforge/agents/".to_string())
    };
    let count = agent_render::materialize_agents_into(
        target_agent,
        &src_dir,
        dst_dir,
        models.as_ref(),
        force,
    )?;
    Ok((count, label))
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
use registry::{agent_templates, command_templates, prompt_templates, skill_templates, SKILLS};

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
    fn materialize_agents_for_writes_codex_only_model_line() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let zforge_dir = root.join(".zforge");
        let zforge_agents = zforge_dir.join("agents");
        std::fs::create_dir_all(&zforge_agents).unwrap();
        std::fs::write(
            zforge_agents.join("code-agent.md"),
            "---\nname: code-agent\nmodel: claude-sonnet-4-6\ncodex_model: gpt-5-codex\nopencode_model: claude-sonnet-4-6\n---\nbody\n",
        ).unwrap();

        let codex_agents = root.join(".codex").join("agents");
        let (count, _label) =
            materialize_agents_for("codex", root, &zforge_dir, &codex_agents, None, false).unwrap();
        assert_eq!(count, 1);
        let written = std::fs::read_to_string(codex_agents.join("code-agent.md")).unwrap();
        assert!(written.contains("model: gpt-5-codex"));
        assert!(!written.contains("codex_model:"));
        assert!(!written.contains("opencode_model:"));
    }

    #[test]
    fn materialize_agents_for_preserves_existing_without_force() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let zforge_dir = root.join(".zforge");
        let zforge_agents = zforge_dir.join("agents");
        std::fs::create_dir_all(&zforge_agents).unwrap();
        std::fs::write(
            zforge_agents.join("spec-agent.md"),
            "---\nmodel: claude-haiku-4-5-20251001\ncodex_model: gpt-5-codex\n---\n",
        )
        .unwrap();
        let codex_agents = root.join(".codex").join("agents");
        std::fs::create_dir_all(&codex_agents).unwrap();
        std::fs::write(codex_agents.join("spec-agent.md"), "preexisting\n").unwrap();

        let (count, _) =
            materialize_agents_for("codex", root, &zforge_dir, &codex_agents, None, false).unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            std::fs::read_to_string(codex_agents.join("spec-agent.md"))
                .unwrap()
                .trim(),
            "preexisting"
        );
    }
}
