use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::mcp_register::Agent;

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

/// Options for `zforge init`.
pub struct InitOptions {
    pub agent: Agent,
    /// Refresh generated files (agent definitions, instruction files,
    /// settings). Never resets config.yaml or models.yaml.
    pub force: bool,
    pub local: bool,
    pub no_register: bool,
    pub name: Option<String>,
    pub switch: bool,
    /// Override the runner used for tasks without `--agent`.
    pub default_runner: Option<String>,
    /// Install missing tools (rtk, codegraph, caveman). `--no-install`
    /// clears it; tools already present are still configured.
    pub install_missing: bool,
}

pub fn run(opts: InitOptions) -> Result<()> {
    let targets: Vec<&'static str> = match opts.agent {
        Agent::All => vec!["claude", "codex", "opencode"],
        Agent::Claude => vec!["claude"],
        Agent::Codex => vec!["codex"],
        Agent::OpenCode => vec!["opencode"],
    };
    run_for(&targets, opts)
}

/// [`run`] for exactly `targets` (`opts.agent` is ignored) — `zforge
/// migrate` refreshes the clients a project already has, which may be any
/// subset.
pub fn run_for(targets: &[&'static str], opts: InitOptions) -> Result<()> {
    let cwd = env::current_dir()?;
    let detected = detect_project(&cwd);
    let force = opts.force;
    let local = opts.local;
    let targets = targets.to_vec();
    let want = |t: &str| targets.contains(&t);

    // Resolve before writing anything, so a bad --default-runner fails
    // cleanly instead of leaving a half-scaffolded project.
    let runner = resolve_default_runner(&targets, opts.default_runner.as_deref(), on_path)?;

    let mode_label = if local { " (local mode)" } else { "" };
    println!(
        "Scaffolding zforge for: {}{}",
        targets.join(", ").cyan(),
        mode_label.dimmed()
    );

    // Default is shared mode: templates + agents + skills live in the
    // global store and config points there. `--local` forks every file
    // into the project. First-time install of the store happens here.
    let global_store = if !local {
        Some(crate::cli::install::ensure_global_store()?)
    } else {
        None
    };
    let store_paths = match &global_store {
        Some(root) => StorePaths::shared(root, dirs::home_dir().as_deref()),
        None => StorePaths::local(),
    };

    let zforge_dir = cwd.join(".zforge");

    if zforge_dir.exists() && !force {
        print!(
            "{} .zforge/ already exists. Update it? [y/N] ",
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

    // config.yaml: created on first init; afterwards only the keys init
    // owns (runner.default, paths.agents, paths.skills) change (FIX-017).
    let managed = Managed {
        default_runner: runner.name(),
        paths: &store_paths,
    };
    match write_config(
        &zforge_dir.join("config.yaml"),
        &detected.language,
        &detected.test_command,
        &managed,
    )? {
        ConfigWrite::Created => {
            stats.record(true);
            print_file_status(true, ".zforge/config.yaml");
        }
        ConfigWrite::Updated => {
            stats.record(false);
            println!(
                "{} .zforge/config.yaml — kept; updated runner/paths only",
                "✓".green()
            );
        }
        ConfigWrite::Unchanged => {
            stats.record(false);
            print_file_status(false, ".zforge/config.yaml");
        }
    }
    println!("{} default runner: {}", "▶".cyan(), runner.explain());

    // models.yaml is user-owned: never overwritten, even with --force, so
    // the documented "edit models.yaml, then init --force" refresh keeps
    // the edits and renders them into the agent files below (FIX-017).
    let created = write_safe(
        &zforge_dir.join("models.yaml"),
        include_str!("../../templates/models.yaml"),
        false,
    )?;
    stats.record(created);
    print_file_status(created, ".zforge/models.yaml");

    let lang_skills = lang_skill_templates(&detected.language);
    let vars = Vars {
        language: detected.language.clone(),
        test_command: detected.test_command,
        project_name: detected.project_name,
        skills_dir: store_paths.skills_ref.clone(),
    };

    // Local mode forks every template/agent/skill into the project so
    // each file can be customized standalone. Shared mode skips these
    // writes; the global store already holds them.
    if local {
        let tmpl_dir = zforge_dir.join("agents");
        std::fs::create_dir_all(&tmpl_dir)?;
        for (name, raw) in agent_templates() {
            let rendered = apply_vars(raw, &vars);
            let created = write_safe(&tmpl_dir.join(name), &rendered, force)?;
            stats.record(created);
        }
        println!(
            "{} .zforge/agents/ — agent definitions",
            label(stats.created > 0)
        );

        let skills_dir = zforge_dir.join("skills");
        std::fs::create_dir_all(&skills_dir)?;
        for (name, raw) in skill_templates() {
            let rendered = apply_vars(raw, &vars);
            let created = write_safe(&skills_dir.join(name), &rendered, force)?;
            stats.record(created);
        }
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
            store_paths.skills_ref.trim_end_matches("/skills").dimmed()
        );
    }

    let readme = apply_vars(include_str!("../../templates/zforge-readme.md"), &vars);
    let created = write_safe(&zforge_dir.join("README.md"), &readme, force)?;
    stats.record(created);
    print_file_status(created, ".zforge/README.md");

    let lang_skills_section =
        build_lang_skills_section(&vars.language, &lang_skills, &store_paths.skills_ref);
    let claude_skills_section = claude_skills::claude_md_section(&vars.language);
    let vars_with_lang = VarsExt {
        vars: &vars,
        lang_skills_section: &lang_skills_section,
        claude_skills_section: &claude_skills_section,
    };

    let tool_opts = ToolOptions {
        install_missing: opts.install_missing,
    };

    if want("claude") {
        scaffold_claude(
            &cwd,
            &zforge_dir,
            &vars_with_lang,
            force,
            global_store.as_deref(),
            tool_opts,
            &mut stats,
        )?;
    }
    if want("codex") {
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
    if want("opencode") {
        scaffold_opencode(
            &cwd,
            &zforge_dir,
            &vars_with_lang,
            force,
            global_store.as_deref(),
            &mut stats,
        )?;
    }

    setup_rtk(&targets, tool_opts);
    setup_codegraph(&cwd, &targets, tool_opts);

    println!();
    println!(
        "  {} created, {} skipped (already existed)",
        stats.created, stats.skipped
    );
    if crate::migrate::project::needs_migration(&cwd) {
        println!();
        println!(
            "{} This project still has files of the removed task pipeline — run {}",
            "ℹ".cyan(),
            "zforge migrate".cyan()
        );
    }

    println!();
    println!("Next steps:");
    println!("  1. Edit .zforge/config.yaml — set project.name");
    if want("claude") {
        println!("  2. Verify CLAUDE.md — project details correct");
        println!("     Register Claude Code MCP: zforge mcp register --agent claude");
    }
    if want("codex") || want("opencode") {
        println!("  2. Verify AGENTS.md — project details correct");
    }
    println!("  3. Start an intake: zforge intake new <ID>   (then: zforge status)");
    if !local {
        println!();
        println!(
            "  {} Templates + skills served from the global store; refresh with {}",
            "ℹ".cyan(),
            "zforge install --force".cyan()
        );
    }

    if !opts.no_register {
        register_project(&cwd, &targets, opts.name.as_deref(), opts.switch);
    }

    Ok(())
}

/// Seed registry rows for exactly the clients scaffolded — before FIX-015
/// `claude` was always added, whatever `--agent` said — and record the
/// project in the global registry. Best-effort: scaffolding is the contract.
fn register_project(cwd: &Path, targets: &[&str], name: Option<&str>, switch: bool) {
    match crate::registry::auto::ensure_default_agents(targets) {
        Ok(inserted) if !inserted.is_empty() => println!(
            "registered default agents in registry.yaml: {}",
            inserted.join(", ")
        ),
        Ok(_) => {}
        Err(e) => eprintln!("warning: default-agent seeding failed: {e:#}"),
    }
    match crate::registry::auto::auto_register(cwd, name, switch) {
        Ok(crate::registry::auto::AutoResult::Registered { name }) => {
            println!("registered project {name} in the zforge registry");
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
        Err(e) => eprintln!("warning: registry update failed: {e:#}"),
    }
}

// --- per-agent scaffolders ---

fn scaffold_claude(
    cwd: &Path,
    zforge_dir: &Path,
    vars_with_lang: &VarsExt<'_>,
    force: bool,
    global_store: Option<&Path>,
    tool_opts: ToolOptions,
    stats: &mut InitStats,
) -> Result<()> {
    // CLAUDE.md at project root — auto-loaded by Claude Code
    let claude_md = apply_vars_ext(include_str!("../../templates/CLAUDE.md"), vars_with_lang);
    let created = instructions::write_and_report(cwd, "CLAUDE.md", &claude_md, force)?;
    stats.record(created);

    // .claude/settings.json
    let claude_dir = cwd.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");
    // Shared with the user and other tools: `--force` merges zforge's
    // allow entries in instead of replacing the file.
    match write_settings(&settings_path, force)? {
        SettingsWrite::Created => {
            stats.record(true);
            print_file_status(true, ".claude/settings.json");
        }
        SettingsWrite::Merged => {
            stats.record(false);
            println!(
                "{} .claude/settings.json — kept; refreshed zforge permissions only",
                "✓".green()
            );
        }
        SettingsWrite::Unchanged | SettingsWrite::Skipped => {
            stats.record(false);
            print_file_status(false, ".claude/settings.json");
        }
    }

    // .claude/agents/ — per-target rendered copies (model: line resolved from
    // models.yaml when present, falling back to template frontmatter).
    let claude_agents_dir = claude_dir.join("agents");
    std::fs::create_dir_all(&claude_agents_dir)?;
    let language = vars_with_lang.vars.language.clone();
    let (agent_count, target_label) = materialize_agents_for(
        "claude",
        cwd,
        zforge_dir,
        &claude_agents_dir,
        global_store,
        force,
        &|phase| claude_skills::agent_frontmatter(phase, &language),
    )?;
    println!(
        "{} .claude/agents/ — {} files (model resolved per claude) ← {}",
        label(agent_count > 0),
        agent_count,
        target_label
    );

    // .claude/skills/ — native skills (IMP-004)
    let skills = claude_skills::write(cwd, vars_with_lang.vars, force)?;
    stats.record(skills.written > 0);
    println!(
        "{} .claude/skills/ — {} zforge skills ({} written, {} kept){}",
        label(skills.written > 0),
        skills.total,
        skills.written,
        skills.kept,
        if skills.removed.is_empty() {
            String::new()
        } else {
            format!("; removed stale: {}", skills.removed.join(", "))
        }
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

    // caveman — terse output mode
    ensure_caveman(tool_opts);

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
    let created = instructions::write_and_report(cwd, "AGENTS.md", &agents_md, force)?;
    stats.record(created);

    // .codex/agents/ — per-target rendered copies. `model:` comes from
    // models.yaml → frontmatter `codex_model:`; none chosen → no line.
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
        &|_| String::new(),
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

    // Auto-register the Codex MCP server
    println!();
    println!("Registering Codex MCP server…");
    if let Err(e) = crate::cli::mcp_register::run(Agent::Codex, force) {
        eprintln!(
            "  {} Codex MCP registration reported errors: {e}",
            "⚠".yellow()
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
    let created = instructions::write_and_report(cwd, "AGENTS.md", &agents_md, force)?;
    stats.record(created);

    // .opencode/agents/ — per-target rendered copies. `model:` comes from
    // models.yaml → frontmatter `opencode_model:`; none chosen → no line.
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
        &|_| String::new(),
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
    /// Where skills live, as instruction files should name it (FIX-013).
    pub(crate) skills_dir: String,
}

// Project-type detection lives in init::detect.
use detect::detect_project;

// Per-language skill templates live in init::lang_skills.
use lang_skills::{build_lang_skills_section, lang_skill_templates};

mod agent_render;
mod claude_settings;
pub(crate) mod claude_skills;
pub(crate) mod config_file;
mod detect;
pub(crate) mod instructions;
mod lang_skills;
mod registry;
mod runner;
mod store_paths;
mod tools;

use claude_settings::{write_settings, SettingsWrite};
use config_file::{write_config, ConfigWrite, Managed};
use runner::{on_path, resolve_default_runner};
use store_paths::StorePaths;
use tools::{ensure_caveman, setup_codegraph, setup_rtk, ToolOptions};

pub(crate) fn apply_vars(template: &str, vars: &Vars) -> String {
    template
        .replace("{{language}}", &vars.language)
        .replace("{{test_command}}", &vars.test_command)
        .replace("{{project_name}}", &vars.project_name)
        .replace("{{skills_dir}}", &vars.skills_dir)
    // leave {{task_id}} and other runtime vars as-is
}

struct VarsExt<'a> {
    vars: &'a Vars,
    lang_skills_section: &'a str,
    claude_skills_section: &'a str,
}

fn apply_vars_ext(template: &str, v: &VarsExt<'_>) -> String {
    apply_vars(template, v.vars)
        .replace("{{lang_skills_section}}", v.lang_skills_section)
        .replace("{{claude_skills_section}}", v.claude_skills_section)
}

/// Render per-target agent files into `dst_dir` from the appropriate source
/// agents directory (global store at `~/.zforge/agents/` when present, else
/// project-local `.zforge/agents/`). Each file gets a single `model:` line
/// resolved from `models.yaml` → template frontmatter → hard default.
///
/// Replaces the older symlink-based approach (`symlink_agents_into`) for
/// agent-CLI directories so each tool reads its own model rather than seeing
/// all three `model:` / `codex_model:` / `opencode_model:` keys at once.
/// A `model:` the user wrote into the store's definition of `phase`'s agent
/// (read the way rendering reads it), when `models.yaml` chooses none.
pub(crate) fn model_in_definition(
    config: &crate::config::Config,
    client: &str,
    phase: &str,
) -> Option<String> {
    let text =
        std::fs::read_to_string(config.agents_dir().join(format!("{phase}-agent.md"))).ok()?;
    agent_render::frontmatter_target_model(&text, client)
}

/// Render the phase agents again for every client this project was set up
/// for (its `.<client>/agents/` exists), after a model choice changed.
/// Returns the directories rewritten, relative to the project.
pub(crate) fn rerender_agents(
    project_root: &Path,
    config: &crate::config::Config,
) -> Result<Vec<String>> {
    let models = crate::config::load_models_from_root(project_root);
    let src = config.agents_dir();
    let language = config.project.language.clone();
    let mut done = Vec::new();
    for client in ["claude", "codex", "opencode"] {
        let rel = format!(".{client}/agents");
        let dst = project_root.join(&rel);
        if !dst.is_dir() {
            continue;
        }
        let claude_skills = |phase: &str| match client {
            "claude" => claude_skills::agent_frontmatter(phase, &language),
            _ => String::new(),
        };
        agent_render::materialize_agents_into(
            client,
            &src,
            &dst,
            models.as_ref(),
            true,
            &claude_skills,
        )?;
        done.push(rel);
    }
    Ok(done)
}

fn materialize_agents_for(
    target_agent: &str,
    cwd: &Path,
    zforge_dir: &Path,
    dst_dir: &Path,
    global_store: Option<&Path>,
    force: bool,
    extra_frontmatter: &dyn Fn(&str) -> String,
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
        extra_frontmatter,
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
use registry::{agent_templates, skill_templates, SKILLS};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_vars() -> Vars {
        Vars {
            language: "rust".into(),
            test_command: "cargo test".into(),
            project_name: "demo".into(),
            skills_dir: "/store/skills".into(),
        }
    }

    // FIX-013: every skill reference in the instruction files must go
    // through the resolved skills dir — none may hardcode `.zforge/skills`,
    // which does not exist in shared mode.
    #[test]
    fn instruction_files_name_skills_through_the_resolved_dir() {
        let vars = sample_vars();
        let ext = VarsExt {
            vars: &vars,
            lang_skills_section: "",
            claude_skills_section: "",
        };
        // CLAUDE.md names native skills instead (IMP-004); see
        // `claude_md_names_native_skills_not_files`.
        for (name, tmpl) in [
            ("AGENTS.md", include_str!("../../templates/AGENTS.md")),
            (
                "zforge-readme.md",
                include_str!("../../templates/zforge-readme.md"),
            ),
        ] {
            let rendered = apply_vars_ext(tmpl, &ext);
            assert!(
                !rendered.contains(".zforge/skills"),
                "{name} still hardcodes .zforge/skills"
            );
            assert!(
                rendered.contains("/store/skills/"),
                "{name} should reference skills under the resolved dir"
            );
        }
    }

    // IMP-004: Claude reads skills natively, so CLAUDE.md lists skill names
    // (generated from the catalog) rather than file paths.
    #[test]
    fn claude_md_names_native_skills_not_files() {
        let vars = sample_vars();
        let section = claude_skills::claude_md_section("rust");
        let ext = VarsExt {
            vars: &vars,
            lang_skills_section: "",
            claude_skills_section: &section,
        };
        let rendered = apply_vars_ext(include_str!("../../templates/CLAUDE.md"), &ext);
        assert!(!rendered.contains("{{"), "unsubstituted placeholder");
        assert!(
            !rendered.contains("skills/review-patch.md"),
            "no file paths"
        );
        assert!(rendered.contains("`zforge-review-patch`"));
        assert!(rendered.contains("`zforge-rust-patterns`"));
    }

    #[test]
    fn agents_md_template_renders_project_vars() {
        let vars = sample_vars();
        let ext = VarsExt {
            vars: &vars,
            lang_skills_section: "",
            claude_skills_section: "",
        };
        let rendered = apply_vars_ext(include_str!("../../templates/AGENTS.md"), &ext);
        assert!(rendered.starts_with("# demo\n"));
        assert!(rendered.contains("**Language:** rust"));
        assert!(rendered.contains("`cargo test`"));
        // Codex-specific anchors must survive substitution
        assert!(rendered.contains("~/.codex/config.toml"));
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
    fn claude_settings_allow_the_v15_tools_only() {
        let s = claude_settings::CLAUDE_SETTINGS_JSON;
        for tool in ["status", "intake_review", "readiness", "run_start"] {
            assert!(s.contains(&format!("\"mcp__zforge__{tool}\"")), "{tool}");
        }
        assert!(!s.contains("mcp__zforge__ship"));
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
        let (count, _label) = materialize_agents_for(
            "codex",
            root,
            &zforge_dir,
            &codex_agents,
            None,
            false,
            &|_| String::new(),
        )
        .unwrap();
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
            zforge_agents.join("code-agent.md"),
            "---\nmodel: claude-haiku-4-5-20251001\ncodex_model: gpt-5-codex\n---\n",
        )
        .unwrap();
        let codex_agents = root.join(".codex").join("agents");
        std::fs::create_dir_all(&codex_agents).unwrap();
        std::fs::write(codex_agents.join("code-agent.md"), "preexisting\n").unwrap();

        let (count, _) = materialize_agents_for(
            "codex",
            root,
            &zforge_dir,
            &codex_agents,
            None,
            false,
            &|_| String::new(),
        )
        .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            std::fs::read_to_string(codex_agents.join("code-agent.md"))
                .unwrap()
                .trim(),
            "preexisting"
        );
    }
}
