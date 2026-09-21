//! The individual Claude Code readiness checks (IMP-005).
//!
//! Each check reports the highest level it actually confirmed. A file being
//! written is `Configured`, not proof that Claude uses it; `Recognized`
//! needs Claude's own CLI to report it; `Working` needs a smoke test that
//! calls no model. What cannot be confirmed non-interactively is stated in
//! `not_checked` rather than assumed.

use super::claude_config::{hooks, parse_mcp_get, referenced_scripts, settings_files, McpStatus};
use super::report::{Check, Level};
use crate::config::Config;
use crate::fs::reader::MarkdownFile;
use crate::process::run_bounded;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// `claude mcp get` starts the server for its health check.
const MCP_CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const QUICK_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Ctx<'a> {
    pub config: &'a Config,
    pub root: PathBuf,
}

/// Run `program args` in the project, bounded. `None` if it cannot start.
fn run(
    ctx: &Ctx<'_>,
    program: &str,
    args: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
) -> Option<(i32, String)> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(&ctx.root);
    let out = run_bounded(cmd, stdin.map(|s| s.as_bytes().to_vec()), timeout).ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Some((out.exit_code(), text))
}

pub fn claude_cli(ctx: &Ctx<'_>) -> Check {
    match run(ctx, "claude", &["--version"], None, QUICK_TIMEOUT) {
        Some((0, out)) => Check::new("claude", true, Level::Working, out.trim().to_string()),
        Some((code, out)) => Check::new(
            "claude",
            true,
            Level::Broken,
            format!("`claude --version` exited {code}: {}", out.trim()),
        ),
        None => Check::new("claude", true, Level::Missing, "`claude` is not on PATH")
            .fix("install Claude Code: https://docs.claude.com/claude-code"),
    }
}

pub fn runner(ctx: &Ctx<'_>) -> Check {
    let runner = ctx.config.default_runner();
    let registry = match crate::registry::io::load() {
        Ok(r) => r,
        Err(e) => {
            return Check::new(
                "runner",
                true,
                Level::Broken,
                format!("registry unreadable: {e:#}"),
            )
        }
    };
    let Some(spec) = registry.resolved_agent(runner, &ctx.root) else {
        return Check::new(
            "runner",
            true,
            Level::Broken,
            format!("default runner `{runner}` is not in the zforge registry"),
        )
        .fix(format!("zforge init --agent {runner}"));
    };
    if which::which(&spec.command).is_err() {
        return Check::new(
            "runner",
            true,
            Level::Broken,
            format!(
                "default runner `{runner}` runs `{}`, which is not on PATH",
                spec.command
            ),
        );
    }
    let mut c = Check::new(
        "runner",
        true,
        Level::Configured,
        format!(
            "default `{runner}` → `{}` (registered, on PATH)",
            spec.command
        ),
    )
    .not_checked("launching the runner would call a model");
    if runner != "claude" {
        c.detail
            .push_str(" — note: this doctor checks the Claude Code setup");
    }
    c
}

pub fn agents(ctx: &Ctx<'_>) -> Check {
    let dir = ctx.root.join(".claude").join("agents");
    if !dir.is_dir() {
        return Check::new(
            "agents",
            true,
            Level::Missing,
            "no .claude/agents/ in this project",
        )
        .fix("zforge init --agent claude");
    }
    let mut problems = Vec::new();
    for phase in crate::cli::mcp_register::PHASES {
        let name = format!("{phase}-agent");
        let file = dir.join(format!("{name}.md"));
        match MarkdownFile::read(&file) {
            Err(_) => problems.push(format!("{name}.md missing or unparsable")),
            Ok(md) => {
                if md.get_str("name") != Some(name.as_str()) {
                    problems.push(format!("{name}.md: frontmatter name is not `{name}`"));
                }
                if md.get_str("model").map(str::trim).unwrap_or("").is_empty() {
                    problems.push(format!("{name}.md: no model"));
                }
            }
        }
    }
    let total = crate::cli::mcp_register::PHASES.len();
    if problems.is_empty() {
        Check::new(
            "agents",
            true,
            Level::Configured,
            format!("{total}/{total} phase definitions valid (dispatched with --agent)"),
        )
        .not_checked("Claude Code has no non-interactive command that lists subagent definitions")
    } else {
        Check::new("agents", true, Level::Broken, problems.join("; "))
            .fix("zforge init --agent claude --force")
    }
}

pub fn skills(ctx: &Ctx<'_>) -> Check {
    let Ok(claude_md) = std::fs::read_to_string(ctx.root.join("CLAUDE.md")) else {
        return Check::new(
            "skills",
            false,
            Level::Missing,
            "no CLAUDE.md in this project",
        )
        .fix("zforge init --agent claude");
    };
    let refs = skill_refs(&claude_md);
    let missing: Vec<&String> = refs
        .iter()
        .filter(|r| {
            let p = Path::new(r.as_str());
            !(if p.is_absolute() {
                p.to_path_buf()
            } else {
                ctx.root.join(p)
            })
            .is_file()
        })
        .collect();
    let base = if missing.is_empty() {
        Check::new(
            "skills",
            false,
            Level::Configured,
            format!(
                "{}/{} checklists referenced by CLAUDE.md exist",
                refs.len(),
                refs.len()
            ),
        )
    } else {
        Check::new(
            "skills",
            false,
            Level::Broken,
            format!(
                "{} of {} referenced checklists missing: {:?}",
                missing.len(),
                refs.len(),
                missing
            ),
        )
        .fix("zforge init --agent claude --force")
    };
    base.not_checked(
        "native skill discovery (.claude/skills/*/SKILL.md) is not implemented yet (IMP-004)",
    )
}

/// Skill files named in backticks in an instruction file.
fn skill_refs(text: &str) -> Vec<String> {
    regex::Regex::new(r"`([^`\s]*skills/[^`\s]+\.md)`")
        .expect("static regex")
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect()
}

pub fn zforge_mcp(ctx: &Ctx<'_>) -> Check {
    mcp_check(
        ctx,
        "zforge mcp",
        "zforge",
        |_| None,
        "zforge mcp register --agent claude",
    )
}

pub fn codegraph_mcp(ctx: &Ctx<'_>) -> Check {
    if which::which("codegraph").is_err() {
        return Check::new(
            "codegraph mcp",
            false,
            Level::Missing,
            "`codegraph` is not installed",
        )
        .fix("npm install -g @colbymchenry/codegraph && zforge init --agent claude --force");
    }
    let root = ctx.root.canonicalize().unwrap_or_else(|_| ctx.root.clone());
    let want = format!("--path {}", root.display());
    mcp_check(
        ctx,
        "codegraph mcp",
        "codegraph",
        move |args| {
            if args.contains(&want) {
                None
            } else {
                Some(format!("not pinned to this project (args: {args})"))
            }
        },
        "zforge init --agent claude --force",
    )
}

/// `claude mcp get <server>`: registered → Recognized, connected → Working.
fn mcp_check(
    ctx: &Ctx<'_>,
    name: &'static str,
    server: &str,
    args_problem: impl Fn(&str) -> Option<String>,
    fix: &str,
) -> Check {
    let Some((_, out)) = run(
        ctx,
        "claude",
        &["mcp", "get", server],
        None,
        MCP_CHECK_TIMEOUT,
    ) else {
        return Check::new(name, false, Level::Missing, "cannot run `claude mcp get`");
    };
    let Some(s) = parse_mcp_get(&out) else {
        return Check::new(
            name,
            false,
            Level::Missing,
            format!("`{server}` not registered with Claude Code"),
        )
        .fix(fix);
    };
    if let Some(problem) = args_problem(&s.args) {
        return Check::new(name, false, Level::Broken, problem).fix(fix);
    }
    match s.status {
        McpStatus::Connected => Check::new(
            name,
            false,
            Level::Working,
            "registered, connected (health check)",
        ),
        McpStatus::PendingApproval => Check::new(
            name,
            false,
            Level::Recognized,
            "registered, pending approval — Claude will not start it yet",
        )
        .fix("open `claude` in this project and approve the server"),
        McpStatus::Failed(why) | McpStatus::Unknown(why) => Check::new(
            name,
            false,
            Level::Broken,
            format!("registered, but: {why}"),
        )
        .fix(fix),
    }
}

pub fn rtk_hook(ctx: &Ctx<'_>) -> Check {
    if which::which("rtk").is_err() {
        return Check::new("rtk hook", false, Level::Missing, "`rtk` is not installed")
            .fix("brew install rtk && rtk init -g");
    }
    let all = hooks(&settings_files(&ctx.root));
    let Some(hook) = all
        .iter()
        .find(|h| h.event == "PreToolUse" && h.command.contains("rtk hook"))
    else {
        return Check::new(
            "rtk hook",
            false,
            Level::Present,
            "rtk installed, but no PreToolUse hook runs it",
        )
        .fix("rtk init -g");
    };
    // Smoke: feed the hook a Bash call and expect the documented rewrite.
    let sample = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"git status"}}"#;
    let words = shlex::split(&hook.command).unwrap_or_default();
    let Some((program, args)) = words.split_first() else {
        return Check::new("rtk hook", false, Level::Broken, "hook command is empty");
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(ctx, program, &args, Some(sample), QUICK_TIMEOUT) {
        Some((0, out)) if out.contains("rtk git status") => Check::new(
            "rtk hook",
            false,
            Level::Working,
            format!(
                "`{}` rewrites Bash commands (smoke: git status)",
                hook.command
            ),
        ),
        Some((code, out)) => Check::new(
            "rtk hook",
            false,
            Level::Broken,
            format!(
                "hook registered but smoke test failed (exit {code}): {}",
                out.trim()
            ),
        ),
        None => Check::new(
            "rtk hook",
            false,
            Level::Broken,
            "hook registered but cannot run",
        ),
    }
}

pub fn caveman_hook(ctx: &Ctx<'_>) -> Check {
    let all = hooks(&settings_files(&ctx.root));
    let cave: Vec<_> = all
        .iter()
        .filter(|h| h.command.contains("caveman"))
        .collect();
    if cave.is_empty() {
        return Check::new(
            "caveman hook",
            false,
            Level::Missing,
            "no caveman hook registered",
        )
        .fix("see https://github.com/JuliusBrussee/caveman");
    }
    let missing: Vec<PathBuf> = cave
        .iter()
        .flat_map(|h| referenced_scripts(&h.command))
        .filter(|p| !p.is_file())
        .collect();
    if !missing.is_empty() {
        return Check::new(
            "caveman hook",
            false,
            Level::Broken,
            format!("hook registered but script missing: {missing:?}"),
        );
    }
    let events: Vec<&str> = cave.iter().map(|h| h.event.as_str()).collect();
    Check::new(
        "caveman hook",
        false,
        Level::Configured,
        format!("registered on {} with scripts present", events.join(", ")),
    )
    .not_checked("hook invocation (its effect is on the model's output)")
}

pub fn evidence(ctx: &Ctx<'_>) -> Check {
    let tasks_dir = ctx.config.tasks_dir();
    let Ok(entries) = std::fs::read_dir(&tasks_dir) else {
        return Check::new("evidence", false, Level::Configured, "no tasks yet");
    };
    let mut verified = 0;
    let mut stale = Vec::new();
    for e in entries.flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        let Ok(ts) = crate::state::TaskState::load(&tasks_dir, &id) else {
            continue;
        };
        if ts.state != crate::state::State::Verified {
            continue;
        }
        verified += 1;
        let status = crate::evidence::current_status(
            &tasks_dir.join(&id).join("verify.md"),
            &ctx.root,
            &ctx.config.project.test_command,
        );
        if let crate::evidence::EvidenceStatus::Invalid { reason } = status {
            stale.push(format!("{id}: {reason}"));
        }
    }
    if verified == 0 {
        return Check::new(
            "evidence",
            false,
            Level::Configured,
            "no tasks awaiting review",
        );
    }
    if stale.is_empty() {
        Check::new(
            "evidence",
            false,
            Level::Working,
            format!("{verified} verified task(s); evidence matches the current code"),
        )
    } else {
        Check::new("evidence", false, Level::Broken, stale.join("; "))
            .fix("re-run `zf verify <ID>` for the tasks listed")
    }
}
