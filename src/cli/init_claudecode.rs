use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::init::{apply_vars, detect_project, label, print_file_status, write_safe, Vars};

// .mcp.json — registers zf as a Claude Code MCP server at project scope.
// Claude Code discovers this file at the project root automatically.
const MCP_JSON: &str = r#"{
  "mcpServers": {
    "zforge": {
      "command": "zf",
      "args": ["mcp"]
    }
  }
}
"#;

// CLAUDE.md — loaded automatically by Claude Code as project instructions.
const CLAUDE_MD_TMPL: &str = r#"# {{project_name}}

## Project Overview

- **Language**: {{language}}
- **Test command**: `{{test_command}}`
- **Workflow tool**: zforge (TDD-first AI development)

## Development Workflow

This project follows a gated TDD-first pipeline driven by the `zforge` MCP server.
Prefer MCP tool calls over raw `zf` CLI commands — they return structured output.

| Phase | MCP tool | CLI equivalent |
|-------|----------|---------------|
| 1. Import task | `task_import` | `zf task import <id>` |
| 2. Spec | `get_prompt(phase="spec")` | `zf spec <id>` |
| 3. Test spec | `get_prompt(phase="testspec")` | `zf testspec <id>` |
| 4. Approve spec | `approve(artifact="spec")` | `zf approve <id> spec` |
| 5. Approve testspec | `approve(artifact="testspec")` | `zf approve <id> testspec` |
| 6. Plan | `get_prompt(phase="plan")` | `zf plan <id>` |
| 7. Code | `get_prompt(phase="code")` | `zf code <id>` |
| 8. Verify | `verify(task_id=<id>)` | `zf verify <id>` |
| 9. Review | `get_prompt(phase="review")` | `zf review <id>` |

Spec and testspec are **gated** — explicit approval required before plan/code can run.

## Agents

See `.claude/agents/` for workflow-specific agent prompts loaded by `/agent`.

## Memory

Project patterns accumulate in `.zforge/memory/`:
- `patterns.md` — approved coding patterns
- `domain-glossary.md` — project terminology
- `anti-patterns.md` — known problems to avoid

Read memory files before implementing to apply established patterns.
"#;

// .claude/settings.json — project-level permissions for Claude Code.
// Allows zf CLI commands and all zforge MCP tools without per-call prompts.
const SETTINGS_JSON: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(zf *)",
      "mcp__zforge__task_import",
      "mcp__zforge__get_prompt",
      "mcp__zforge__approve",
      "mcp__zforge__verify",
      "mcp__zforge__status"
    ]
  }
}
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
    let claude_dir = cwd.join(".claude");

    if claude_dir.exists() && !force {
        print!("{} .claude/ already exists. Overwrite? [y/N] ", "⚠".yellow());
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

    // .mcp.json at project root
    let created = write_safe(&cwd.join(".mcp.json"), MCP_JSON, force)?;
    stats.record(created);
    print_file_status(created, ".mcp.json");

    // CLAUDE.md at project root
    let claude_md = apply_vars(CLAUDE_MD_TMPL, &vars);
    let created = write_safe(&cwd.join("CLAUDE.md"), &claude_md, force)?;
    stats.record(created);
    print_file_status(created, "CLAUDE.md");

    // .claude/settings.json
    std::fs::create_dir_all(&claude_dir)?;
    let created = write_safe(&claude_dir.join("settings.json"), SETTINGS_JSON, force)?;
    stats.record(created);
    print_file_status(created, ".claude/settings.json");

    // .claude/agents/ — symlinks to .zforge/agents/*.md (single source of truth)
    let agents_dir = claude_dir.join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    let zforge_agents = cwd.join(".zforge").join("agents");
    let agent_count = symlink_agents(&zforge_agents, &agents_dir)?;
    println!("{} .claude/agents/ — {} symlinks → .zforge/agents/", label(agent_count > 0), agent_count);

    // .claude/rules/ — coding rules from ~/.claude/rules or bundled fallbacks
    let rules_dir = claude_dir.join("rules");
    std::fs::create_dir_all(&rules_dir)?;
    let rule_count = install_rules(&rules_dir, &vars.language, force, &mut stats)?;
    println!("{} .claude/rules/ — {} rule files", label(rule_count > 0), rule_count);

    println!();
    println!(
        "  {} created, {} skipped (already existed)",
        stats.created, stats.skipped
    );
    println!();
    println!("Claude Code setup complete. Next steps:");
    println!("  1. Open this project in Claude Code");
    println!("  2. The zforge MCP tools are now available (task_import, get_prompt, approve, verify, status)");
    println!("  3. Run: zf task import TASK-001 — to start your first task");
    println!("  4. Or use the MCP tool: mcp__zforge__task_import");
    println!("  5. Follow the spec → testspec → plan → code → verify → review pipeline");

    Ok(())
}

/// Symlink `.claude/agents/*.md` → `.zforge/agents/*.md`.
/// Falls back to copy on non-Unix (Windows). Skips if link already exists.
fn symlink_agents(_zforge_agents: &Path, claude_agents: &Path) -> Result<usize> {
    let names = [
        "spec-agent.md",
        "testspec-agent.md",
        "plan-agent.md",
        "code-agent.md",
        "review-agent.md",
    ];
    let mut count = 0;
    for name in names {
        let link = claude_agents.join(name);
        if link.exists() || link.symlink_metadata().is_ok() {
            continue;
        }
        let target = Path::new("../../.zforge/agents").join(name);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        #[cfg(not(unix))]
        {
            let src = zforge_agents.join(name);
            if src.exists() {
                std::fs::copy(&src, &link)?;
            }
        }
        count += 1;
    }
    Ok(count)
}

fn install_rules(
    rules_dir: &Path,
    language: &str,
    force: bool,
    stats: &mut InitStats,
) -> Result<usize> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let ecc_rules = home.join(".claude").join("rules");
    let mut count = 0;

    let rule_names = ["coding-style", "testing", "security"];

    for name in rule_names {
        // Try language-specific first (e.g. ~/.claude/rules/rust/coding-style.md)
        // then flat (e.g. ~/.claude/rules/coding-style.md)
        let content = read_ecc_rule_lang(&ecc_rules, language, name)
            .or_else(|| read_ecc_rule_flat(&ecc_rules, name));

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

fn read_ecc_rule_lang(ecc_rules: &Path, language: &str, name: &str) -> Option<String> {
    let path = ecc_rules.join(language).join(format!("{}.md", name));
    std::fs::read_to_string(path).ok()
}

fn read_ecc_rule_flat(ecc_rules: &Path, name: &str) -> Option<String> {
    let path = ecc_rules.join(format!("{}.md", name));
    std::fs::read_to_string(path).ok()
}
