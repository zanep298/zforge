use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = r#"project:
  name: ""
  language: "rust"
  test_command: "cargo test"
  root_dir: "."
opencode:
  model: "claude-sonnet-4-5"
  context_files: []
paths:
  tasks: "./.zforge/tasks"
  agents: "./.zforge/agents"
  memory: "./.zforge/memory"
  skills: "./.zforge/skills"
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
      "mcp__zforge__status"
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

pub fn run(force: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let detected = detect_project(&cwd);

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

    // Config
    let config_content = DEFAULT_CONFIG
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
    let lang_skills = lang_skill_templates(&detected.language);
    let vars = Vars {
        language: detected.language.clone(),
        test_command: detected.test_command,
        project_name: detected.project_name,
    };
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
        "{} .zforge/skills/ — {} workflow + {} {} skills",
        label(stats.created > 0),
        SKILLS.len(),
        lang_count,
        vars.language
    );

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

    // CLAUDE.md at project root — auto-loaded by Claude Code
    let lang_skills_section = build_lang_skills_section(&vars.language, &lang_skills);
    let vars_with_lang = VarsExt {
        vars: &vars,
        lang_skills_section: &lang_skills_section,
    };
    let claude_md = apply_vars_ext(include_str!("../../templates/CLAUDE.md"), &vars_with_lang);
    let created = write_safe(&cwd.join("CLAUDE.md"), &claude_md, force)?;
    stats.record(created);
    print_file_status(created, "CLAUDE.md");

    if !created {
        eprintln!(
            "  {} CLAUDE.md already exists — not overwritten. Run with --force to replace.",
            "⚠".yellow()
        );
    }

    // .claude/settings.json — Claude Code permissions for zforge commands and MCP tools
    let claude_dir = cwd.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");
    let created = write_safe(&settings_path, CLAUDE_SETTINGS_JSON, force)?;
    stats.record(created);
    print_file_status(created, ".claude/settings.json");

    // .claude/agents/ — symlinks to .zforge/agents/*.md (single source of truth)
    let claude_agents_dir = claude_dir.join("agents");
    std::fs::create_dir_all(&claude_agents_dir)?;
    let agent_count = symlink_claude_agents(&zforge_dir.join("agents"), &claude_agents_dir)?;
    println!(
        "{} .claude/agents/ — {} symlinks → .zforge/agents/",
        label(agent_count > 0),
        agent_count
    );

    // .claude/rules/ — coding standards (from ~/.claude/rules or bundled fallbacks)
    let claude_rules_dir = claude_dir.join("rules");
    std::fs::create_dir_all(&claude_rules_dir)?;
    let rule_count = install_rules_for_claude(&claude_rules_dir, force, &mut stats)?;
    println!(
        "{} .claude/rules/ — {} rule files",
        label(rule_count > 0),
        rule_count
    );

    // .zforge/tasks/
    let tasks_dir = zforge_dir.join("tasks");
    std::fs::create_dir_all(&tasks_dir)?;
    println!("{} .zforge/tasks/", "✓".green());

    println!();
    println!(
        "  {} created, {} skipped (already existed)",
        stats.created, stats.skipped
    );
    println!();
    println!("Next steps:");
    println!("  1. Edit .zforge/config.yaml — set project.name");
    println!("  2. Edit CLAUDE.md — verify project details are correct");
    println!("  3. Register MCP server with Claude Code: zforge mcp register");
    println!("  4. Run: zf task import TASK-123");
    println!("  5. Claude permissions preconfigured in .claude/settings.json");

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

pub(crate) struct DetectedProject {
    pub(crate) language: String,
    pub(crate) test_command: String,
    pub(crate) project_name: String,
}

pub(crate) fn detect_project(root: &std::path::Path) -> DetectedProject {
    if root.join("Cargo.toml").exists() {
        let name = read_cargo_name(root).unwrap_or_default();
        return DetectedProject {
            language: "rust".into(),
            test_command: "cargo test".into(),
            project_name: name,
        };
    }
    if root.join("go.mod").exists() {
        let name = read_go_module_name(root).unwrap_or_default();
        return DetectedProject {
            language: "go".into(),
            test_command: "go test ./...".into(),
            project_name: name,
        };
    }
    if root.join("package.json").exists() {
        let name = read_package_json_name(root).unwrap_or_default();
        return DetectedProject {
            language: "typescript".into(),
            test_command: "npm test".into(),
            project_name: name,
        };
    }
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        return DetectedProject {
            language: "python".into(),
            test_command: "pytest".into(),
            project_name: String::new(),
        };
    }
    if let Some(d) = detect_android_project(root) {
        return d;
    }
    if let Some(d) = detect_ios_project(root) {
        return d;
    }
    DetectedProject {
        language: "rust".into(),
        test_command: "cargo test".into(),
        project_name: String::new(),
    }
}

fn detect_android_project(root: &std::path::Path) -> Option<DetectedProject> {
    // Android project: has settings.gradle.kts (or .gradle) and an app/ subdirectory
    let has_settings = root.join("settings.gradle.kts").exists()
        || root.join("settings.gradle").exists();
    let has_app_dir = root.join("app").is_dir();
    if !(has_settings && has_app_dir) {
        return None;
    }
    let name = read_android_app_name(root).unwrap_or_default();
    Some(DetectedProject {
        language: "android".into(),
        test_command: "./gradlew test".into(),
        project_name: name,
    })
}

fn read_android_app_name(root: &std::path::Path) -> Option<String> {
    let settings = root.join("settings.gradle.kts");
    let content = std::fs::read_to_string(&settings)
        .or_else(|_| std::fs::read_to_string(root.join("settings.gradle")))
        .ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("rootProject.name") {
            if let Some(val) = line.splitn(2, '=').nth(1) {
                return Some(
                    val.trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_matches(',')
                        .to_string(),
                );
            }
        }
    }
    None
}

fn detect_ios_project(root: &std::path::Path) -> Option<DetectedProject> {
    // Swift Package Manager project
    if root.join("Package.swift").exists() {
        let name = read_swift_package_name(root).unwrap_or_default();
        return Some(DetectedProject {
            language: "ios".into(),
            test_command: "swift test".into(),
            project_name: name,
        });
    }
    // Xcode project / workspace
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut xcodeproj: Option<String> = None;
        let mut xcworkspace: Option<String> = None;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".xcworkspace") && !name.contains(".xcodeproj") {
                xcworkspace = Some(name.trim_end_matches(".xcworkspace").to_string());
            } else if name.ends_with(".xcodeproj") {
                xcodeproj = Some(name.trim_end_matches(".xcodeproj").to_string());
            }
        }
        let scheme = xcworkspace.or(xcodeproj);
        if let Some(scheme_name) = scheme {
            let test_command = format!(
                "xcodebuild test -scheme {scheme_name} -destination 'platform=iOS Simulator,name=iPhone 16'"
            );
            return Some(DetectedProject {
                language: "ios".into(),
                test_command,
                project_name: scheme_name,
            });
        }
    }
    None
}

fn read_swift_package_name(root: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("Package.swift")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name:") {
            return Some(
                line.trim_start_matches("name:")
                    .trim()
                    .trim_matches('"')
                    .trim_matches(',')
                    .to_string(),
            );
        }
    }
    None
}

fn read_cargo_name(root: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name") {
            if let Some(val) = line.splitn(2, '=').nth(1) {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn read_go_module_name(root: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("go.mod")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("module ") {
            let module = line.trim_start_matches("module ").trim();
            // take last path segment as the name
            return Some(module.rsplit('/').next().unwrap_or(module).to_string());
        }
    }
    None
}

fn read_package_json_name(root: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("package.json")).ok()?;
    // simple extract — avoid pulling in serde_json just for this
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("\"name\"") {
            if let Some(val) = line.splitn(2, ':').nth(1) {
                return Some(
                    val.trim()
                        .trim_matches(',')
                        .trim()
                        .trim_matches('"')
                        .to_string(),
                );
            }
        }
    }
    None
}

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

fn lang_skill_templates(language: &str) -> Vec<(String, &'static str)> {
    match language {
        "rust" => vec![
            (
                "rust-patterns.md".into(),
                include_str!("../../templates/skills/lang/rust-patterns.md"),
            ),
            (
                "rust-testing.md".into(),
                include_str!("../../templates/skills/lang/rust-testing.md"),
            ),
        ],
        "go" => vec![
            (
                "go-patterns.md".into(),
                include_str!("../../templates/skills/lang/go-patterns.md"),
            ),
            (
                "go-testing.md".into(),
                include_str!("../../templates/skills/lang/go-testing.md"),
            ),
        ],
        "typescript" => vec![
            (
                "typescript-patterns.md".into(),
                include_str!("../../templates/skills/lang/typescript-patterns.md"),
            ),
            (
                "typescript-testing.md".into(),
                include_str!("../../templates/skills/lang/typescript-testing.md"),
            ),
        ],
        "python" => vec![
            (
                "python-patterns.md".into(),
                include_str!("../../templates/skills/lang/python-patterns.md"),
            ),
            (
                "python-testing.md".into(),
                include_str!("../../templates/skills/lang/python-testing.md"),
            ),
        ],
        "ios" => vec![
            (
                "ios-patterns.md".into(),
                include_str!("../../templates/skills/lang/ios-patterns.md"),
            ),
            (
                "ios-testing.md".into(),
                include_str!("../../templates/skills/lang/ios-testing.md"),
            ),
            (
                "ios-ui-patterns.md".into(),
                include_str!("../../templates/skills/lang/ios-ui-patterns.md"),
            ),
        ],
        "android" => vec![
            (
                "android-patterns.md".into(),
                include_str!("../../templates/skills/lang/android-patterns.md"),
            ),
            (
                "android-testing.md".into(),
                include_str!("../../templates/skills/lang/android-testing.md"),
            ),
        ],
        _ => vec![],
    }
}

fn build_lang_skills_section(language: &str, skills: &[(String, &str)]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let rows: Vec<String> = skills
        .iter()
        .map(|(name, _)| {
            format!(
                "| `.zforge/skills/{}` | {} patterns and testing |",
                name, language
            )
        })
        .collect();
    format!(
        "## Language Skills\n\nRead these before writing any {} code:\n\n| File | Purpose |\n|------|---------|\\n{}\n",
        language,
        rows.join("\n")
    )
}

/// Create symlinks in `claude_agents_dir` pointing to `*.md` files in `zforge_agents_dir`.
/// `.zforge/agents/` is the single source of truth — Claude Code reads via symlinks.
fn symlink_claude_agents(_zforge_agents_dir: &Path, claude_agents_dir: &Path) -> Result<usize> {
    let agent_names = [
        "spec-agent.md",
        "testspec-agent.md",
        "plan-agent.md",
        "code-agent.md",
        "review-agent.md",
    ];
    let mut count = 0;
    for name in agent_names {
        let link = claude_agents_dir.join(name);
        if link.exists() || link.symlink_metadata().is_ok() {
            continue; // already linked or file exists
        }
        // Relative path: .claude/agents/X.md → ../../.zforge/agents/X.md
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

// --- embedded file lists ---

const PROMPT_TEMPLATES: &[(&str, &str)] = &[
    ("spec.tmpl", include_str!("../../templates/spec.tmpl")),
    (
        "testspec.tmpl",
        include_str!("../../templates/testspec.tmpl"),
    ),
    ("plan.tmpl", include_str!("../../templates/plan.tmpl")),
    ("code.tmpl", include_str!("../../templates/code.tmpl")),
    ("review.tmpl", include_str!("../../templates/review.tmpl")),
    (
        "verify-analysis.tmpl",
        include_str!("../../templates/verify-analysis.tmpl"),
    ),
];

const AGENTS: &[(&str, &str)] = &[
    (
        "spec-agent.md",
        include_str!("../../templates/agents/spec-agent.md"),
    ),
    (
        "testspec-agent.md",
        include_str!("../../templates/agents/testspec-agent.md"),
    ),
    (
        "plan-agent.md",
        include_str!("../../templates/agents/plan-agent.md"),
    ),
    (
        "code-agent.md",
        include_str!("../../templates/agents/code-agent.md"),
    ),
    (
        "review-agent.md",
        include_str!("../../templates/agents/review-agent.md"),
    ),
];

const SKILLS: &[(&str, &str)] = &[
    (
        "clarify-spec.md",
        include_str!("../../templates/skills/clarify-spec.md"),
    ),
    (
        "derive-test-cases.md",
        include_str!("../../templates/skills/derive-test-cases.md"),
    ),
    (
        "implementation-planning.md",
        include_str!("../../templates/skills/implementation-planning.md"),
    ),
    (
        "write-tests-first.md",
        include_str!("../../templates/skills/write-tests-first.md"),
    ),
    (
        "implement-minimal-patch.md",
        include_str!("../../templates/skills/implement-minimal-patch.md"),
    ),
    (
        "review-patch.md",
        include_str!("../../templates/skills/review-patch.md"),
    ),
];

fn prompt_templates() -> &'static [(&'static str, &'static str)] {
    PROMPT_TEMPLATES
}

fn agent_templates() -> &'static [(&'static str, &'static str)] {
    AGENTS
}

fn skill_templates() -> &'static [(&'static str, &'static str)] {
    SKILLS
}
