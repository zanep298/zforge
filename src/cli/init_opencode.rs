use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::init::{apply_vars, detect_project, label, print_file_status, write_safe, Vars};

// opencode.json — the canonical OpenCode project config file
// Registers the zf MCP server and lists instruction/skill files loaded at startup.
// Valid keys come from https://opencode.ai/config.json schema.
// "instructions" = array of file paths loaded as system context.
// "skills.paths"  = array of directories OpenCode scans for skill files.
const OPENCODE_CONFIG_TMPL: &str = r#"{
  "$schema": "https://opencode.ai/config.json",
  "model": "{{model}}",
  "instructions": [
    ".opencode/instructions.md",
    ".opencode/rules/coding-style.md",
    ".opencode/rules/testing.md",
    ".opencode/rules/security.md",
    ".zforge/agents/spec-agent.md",
    ".zforge/agents/testspec-agent.md",
    ".zforge/agents/plan-agent.md",
    ".zforge/agents/code-agent.md",
    ".zforge/agents/review-agent.md"
  ],
  "skills": {
    "paths": [".opencode/skills", ".zforge/skills"]
  },
  "mcp": {
    "zforge": {
      "type": "local",
      "command": ["zf", "mcp"],
      "enabled": true
    }
  }
}
"#;

const INSTRUCTIONS_TMPL: &str = r#"# {{project_name}}

## Project Overview

- **Language**: {{language}}
- **Test command**: `{{test_command}}`
- **Workflow tool**: zforge (TDD-first AI development)

## Development Workflow

This project follows a gated TDD-first pipeline:

| Step | Command | Output |
|------|---------|--------|
| 1. Spec | `zf spec <id>` | `tasks/<id>/spec.md` |
| 2. Test spec | `zf testspec <id>` | `tasks/<id>/testspec.md` |
| 3. Plan | `zf plan <id>` | `tasks/<id>/plan.md` |
| 4. Code | `zf code <id>` | implementation |
| 5. Verify | `zf verify <id>` | test results |
| 6. Review | `zf review <id>` | patterns extracted |

Each phase is gated: spec and testspec require explicit approval before proceeding.

## Agents

See `.opencode/agents/` for workflow-specific agent prompts.

## Skills

See `.opencode/skills/` for language patterns and TDD workflow guides.

## Rules

See `.opencode/rules/` for coding standards and security guidelines.

## Memory

Project-specific patterns are accumulated in `.zforge/memory/`:
- `patterns.md` — approved coding patterns
- `domain-glossary.md` — project terminology
- `anti-patterns.md` — known problems to avoid

Always read memory files before implementing to apply established patterns.
"#;

// Fallback rule content when ~/.claude/rules/ is not available
const FALLBACK_CODING_STYLE: &str = r#"# Coding Style

- Prefer immutability: create new values, don't mutate in place
- Functions under 50 lines; files under 800 lines
- Handle errors explicitly — no silent swallowing
- Validate at system boundaries (user input, external APIs)
- No magic numbers; use named constants
"#;

const FALLBACK_TESTING: &str = r#"# Testing

- Minimum 80% test coverage
- Write tests first (TDD): RED → GREEN → REFACTOR
- Use AAA pattern: Arrange → Act → Assert
- Descriptive test names that explain the scenario
- Unit tests alongside source; integration tests in separate directory
"#;

const FALLBACK_SECURITY: &str = r#"# Security

- Never hardcode secrets — use environment variables
- Validate all user input at boundaries
- Use parameterized queries to prevent SQL injection
- Never expose internal errors to end users
- Run security audits as part of CI (`cargo audit`, etc.)
"#;

const FALLBACK_TDD_WORKFLOW: &str = r#"# TDD Workflow

## Red-Green-Refactor

1. **RED** — Write a failing test that describes the desired behavior
2. **GREEN** — Write the minimal code to make it pass
3. **REFACTOR** — Clean up while keeping tests green

## Rules

- Never write production code without a failing test
- Commit after each green phase
- Keep tests fast and independent
- Mock only at architectural boundaries

## Coverage Target: 80%+
"#;

struct InitStats {
    created: usize,
    skipped: usize,
}

impl InitStats {
    fn new() -> Self {
        Self { created: 0, skipped: 0 }
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
    let opencode_dir = cwd.join(".opencode");

    if opencode_dir.exists() && !force {
        print!("{} .opencode/ already exists. Overwrite? [y/N] ", "⚠".yellow());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let vars = Vars {
        language: detected.language.clone(),
        test_command: detected.test_command,
        project_name: if detected.project_name.is_empty() {
            cwd.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string()
        } else {
            detected.project_name
        },
    };

    let mut stats = InitStats::new();

    // .opencode/config.json
    let model = "claude-sonnet-4-5";
    let config = OPENCODE_CONFIG_TMPL
        .replace("{{model}}", model)
        .replace("{{language}}", &vars.language);
    let created = write_safe(&opencode_dir.join("opencode.json"), &config, force)?;
    stats.record(created);
    print_file_status(created, ".opencode/opencode.json");

    // .opencode/instructions.md
    let instructions = apply_vars(INSTRUCTIONS_TMPL, &vars);
    let created = write_safe(&opencode_dir.join("instructions.md"), &instructions, force)?;
    stats.record(created);
    print_file_status(created, ".opencode/instructions.md");

    // agents loaded from .zforge/agents/ via opencode.json instructions[] — no copy needed

    // .opencode/skills/ — ECC skills from ~/.claude/skills or bundled fallbacks
    let skills_dir = opencode_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)?;
    let skill_count = install_skills(&skills_dir, &vars.language, force, &mut stats)?;
    println!("{} .opencode/skills/ — {} skills", label(skill_count > 0), skill_count);

    // .opencode/rules/ — ECC rules from ~/.claude/rules or bundled fallbacks
    let rules_dir = opencode_dir.join("rules");
    std::fs::create_dir_all(&rules_dir)?;
    let rule_count = install_rules(&rules_dir, force, &mut stats)?;
    println!("{} .opencode/rules/ — {} rule files", label(rule_count > 0), rule_count);

    println!();
    println!(
        "  {} created, {} skipped (already existed)",
        stats.created, stats.skipped
    );
    println!();
    println!("OpenCode setup complete. Next steps:");
    println!("  1. Open this project in OpenCode");
    println!("  2. OpenCode will load .opencode/instructions.md and all context files");
    println!("  3. Run: zf task import TASK-001 — to start your first task");
    println!("  4. Follow the spec → testspec → plan → code → verify → review pipeline");

    Ok(())
}

fn install_skills(
    skills_dir: &Path,
    language: &str,
    force: bool,
    stats: &mut InitStats,
) -> Result<usize> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let ecc_skills = home.join(".claude").join("skills");
    let mut count = 0;

    // Language-specific skills
    let lang_skills = [
        format!("{}-patterns", language),
        format!("{}-testing", language),
    ];
    for skill_name in &lang_skills {
        let content = read_ecc_skill(&ecc_skills, skill_name)
            .or_else(|| bundled_lang_skill(language, skill_name));
        if let Some(content) = content {
            let filename = format!("{}.md", skill_name);
            let created = write_safe(&skills_dir.join(&filename), &content, force)?;
            stats.record(created);
            if created {
                count += 1;
            }
        }
    }

    // Workflow skills from ECC or fallback
    let workflow_skills: &[(&str, &str)] = &[
        ("tdd-workflow", FALLBACK_TDD_WORKFLOW),
        ("verification-loop", ""),
        ("security-review", ""),
    ];
    for (skill_name, fallback) in workflow_skills {
        let content = read_ecc_skill(&ecc_skills, skill_name)
            .or_else(|| if fallback.is_empty() { None } else { Some(fallback.to_string()) });
        if let Some(content) = content {
            let filename = format!("{}.md", skill_name);
            let created = write_safe(&skills_dir.join(&filename), &content, force)?;
            stats.record(created);
            if created {
                count += 1;
            }
        }
    }

    Ok(count)
}

fn install_rules(rules_dir: &Path, force: bool, stats: &mut InitStats) -> Result<usize> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let ecc_rules = home.join(".claude").join("rules");
    let mut count = 0;

    let rule_defs: &[(&str, &str)] = &[
        ("coding-style", FALLBACK_CODING_STYLE),
        ("testing", FALLBACK_TESTING),
        ("security", FALLBACK_SECURITY),
    ];

    for (name, fallback) in rule_defs {
        // Try language-specific rule first (e.g., ~/.claude/rules/rust/coding-style.md)
        // Then flat rule (e.g., ~/.claude/rules/coding-style.md)
        // Then use bundled fallback
        let content = read_ecc_rule_flat(&ecc_rules, name)
            .or_else(|| Some(fallback.to_string()));
        if let Some(content) = content {
            let filename = format!("{}.md", name);
            let created = write_safe(&rules_dir.join(&filename), &content, force)?;
            stats.record(created);
            if created {
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Read SKILL.md from ~/.claude/skills/<name>/SKILL.md
fn read_ecc_skill(ecc_skills: &Path, skill_name: &str) -> Option<String> {
    let skill_file = ecc_skills.join(skill_name).join("SKILL.md");
    std::fs::read_to_string(&skill_file).ok()
}

/// Read a flat rule file from ~/.claude/rules/<name>.md
fn read_ecc_rule_flat(ecc_rules: &Path, name: &str) -> Option<String> {
    let rule_file = ecc_rules.join(format!("{}.md", name));
    std::fs::read_to_string(&rule_file).ok()
}

/// Bundled language skill templates embedded at compile time
fn bundled_lang_skill(language: &str, skill_name: &str) -> Option<String> {
    match (language, skill_name) {
        ("rust", s) if s.ends_with("-patterns") => {
            Some(include_str!("../../templates/skills/lang/rust-patterns.md").to_string())
        }
        ("rust", s) if s.ends_with("-testing") => {
            Some(include_str!("../../templates/skills/lang/rust-testing.md").to_string())
        }
        ("go", s) if s.ends_with("-patterns") => {
            Some(include_str!("../../templates/skills/lang/go-patterns.md").to_string())
        }
        ("go", s) if s.ends_with("-testing") => {
            Some(include_str!("../../templates/skills/lang/go-testing.md").to_string())
        }
        ("typescript", s) if s.ends_with("-patterns") => {
            Some(include_str!("../../templates/skills/lang/typescript-patterns.md").to_string())
        }
        ("typescript", s) if s.ends_with("-testing") => {
            Some(include_str!("../../templates/skills/lang/typescript-testing.md").to_string())
        }
        ("python", s) if s.ends_with("-patterns") => {
            Some(include_str!("../../templates/skills/lang/python-patterns.md").to_string())
        }
        ("python", s) if s.ends_with("-testing") => {
            Some(include_str!("../../templates/skills/lang/python-testing.md").to_string())
        }
        _ => None,
    }
}
