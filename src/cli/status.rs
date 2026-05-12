use crate::config;
use crate::fs::reader;
use crate::state::{State, TaskState};
use anyhow::Result;
use chrono::{DateTime, Local};
use colored::Colorize;
use std::fmt::Write as _;
use walkdir::WalkDir;

pub fn run(task_id: Option<String>, json: bool, short: bool) -> Result<()> {
    let out = render(task_id, json, short)?;
    print!("{}", out);
    Ok(())
}

/// Render status output to a `String` without touching stdout. Used by the MCP server
/// (whose stdout is the JSON-RPC transport) and by `run()` above.
pub fn render(task_id: Option<String>, json: bool, short: bool) -> Result<String> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    if let Some(id) = task_id {
        render_single(&tasks_dir, &id, json, short)
    } else {
        render_all(&tasks_dir, json, short)
    }
}

fn render_single(
    tasks_dir: &std::path::Path,
    task_id: &str,
    json: bool,
    short: bool,
) -> Result<String> {
    let ts = TaskState::load(tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    let mut out = String::new();

    if short {
        writeln!(out, "{}", ts.state.as_str())?;
        return Ok(out);
    }

    if json {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "task_id": ts.task_id,
                "state": ts.state.as_str(),
                "updated_at": ts.updated_at.to_rfc3339(),
            })
        )?;
        return Ok(out);
    }

    // Read title and domain from task.md
    let task_md = reader::MarkdownFile::read(&tasks_dir.join(task_id).join("task.md")).ok();
    let title = task_md
        .as_ref()
        .and_then(|m| m.get_str("title"))
        .unwrap_or("")
        .to_string();
    let domain = task_md
        .as_ref()
        .and_then(|m| m.get_str("domain"))
        .unwrap_or("")
        .to_string();

    let sep = "━".repeat(40);
    writeln!(out, "{}", sep)?;
    let header = if title.is_empty() {
        task_id.to_string()
    } else {
        format!("{} · {}", task_id, title)
    };
    writeln!(out, "  {}", header.bold())?;
    writeln!(out, "{}", sep)?;
    writeln!(out, "  State:   {} ✓", ts.state.as_str().green())?;
    if !domain.is_empty() {
        writeln!(out, "  Domain:  {}", domain)?;
    }
    writeln!(out, "  Updated: {}", format_relative(&ts.updated_at))?;
    writeln!(out)?;
    writeln!(out, "  Artifacts:")?;

    let artifacts = [
        "task.md",
        "spec.md",
        "testspec.md",
        "plan.md",
        "implementation-log.md",
        "verify.md",
        "review-summary.md",
    ];

    for artifact in artifacts {
        let path = tasks_dir.join(task_id).join(artifact);
        if path.exists() {
            let meta = std::fs::metadata(&path)?;
            let modified: DateTime<Local> = meta.modified()?.into();
            let date_str = modified.format("%b %d %H:%M").to_string();

            let mut extra = String::new();
            if artifact == "testspec.md" || artifact == "plan.md" {
                if let Ok(md) = reader::MarkdownFile::read(&path) {
                    if md.get_bool("reviewed") {
                        extra = " [reviewed]".cyan().to_string();
                    }
                }
            }
            if artifact == "verify.md" {
                if let Ok(md) = reader::MarkdownFile::read(&path) {
                    let passed = md.get_bool("passed");
                    let p = md
                        .frontmatter
                        .get("passed_tests")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let t = md
                        .frontmatter
                        .get("total_tests")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if passed {
                        extra = format!(" [PASSED {}/{}]", p, t).green().to_string();
                    } else {
                        extra = format!(" [FAILED {}/{}]", p, t).red().to_string();
                    }
                }
            }

            writeln!(
                out,
                "    {} {:<28} {}{}",
                "✓".green(),
                artifact,
                date_str,
                extra
            )?;
        } else {
            writeln!(out, "    {} {}", "✗".red(), artifact)?;
        }
    }

    writeln!(out)?;
    writeln!(out, "  Next:    {}", ts.state.hint())?;
    writeln!(out, "{}", sep)?;

    Ok(out)
}

fn render_all(tasks_dir: &std::path::Path, json: bool, short: bool) -> Result<String> {
    let mut out = String::new();

    if !tasks_dir.exists() {
        writeln!(out, "No tasks found.")?;
        return Ok(out);
    }

    let mut tasks: Vec<(String, TaskState)> = Vec::new();

    for entry in WalkDir::new(tasks_dir).max_depth(1).min_depth(1) {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            continue;
        }
        let task_id = entry.file_name().to_string_lossy().to_string();
        if let Ok(ts) = TaskState::load(tasks_dir, &task_id) {
            tasks.push((task_id, ts));
        }
    }

    if tasks.is_empty() {
        writeln!(out, "No tasks found.")?;
        return Ok(out);
    }

    if json {
        let arr: Vec<_> = tasks
            .iter()
            .map(|(id, ts)| {
                serde_json::json!({
                    "task_id": id,
                    "state": ts.state.as_str(),
                })
            })
            .collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&arr)?)?;
        return Ok(out);
    }

    let sep = "━".repeat(40);
    if !short {
        writeln!(out, "Tasks in {}/", tasks_dir.display())?;
        writeln!(out, "{}", sep)?;
    }

    let complete_count = tasks
        .iter()
        .filter(|(_, ts)| ts.state == State::Reviewed)
        .count();

    for (id, ts) in &tasks {
        let title = reader::MarkdownFile::read(&tasks_dir.join(id).join("task.md"))
            .ok()
            .and_then(|m| m.get_str("title").map(str::to_string))
            .unwrap_or_default();

        let state_str = ts.state.as_str();
        let colored_state = if ts.state == State::Reviewed {
            state_str.green().to_string()
        } else {
            state_str.yellow().to_string()
        };

        if short {
            writeln!(out, "{:<12} {}", id, colored_state)?;
        } else {
            writeln!(out, "  {:<12} {:<18} {}", id.bold(), colored_state, title)?;
        }
    }

    if !short {
        writeln!(out, "{}", sep)?;
        writeln!(
            out,
            "  {} tasks · {} complete · {} in progress",
            tasks.len(),
            complete_count,
            tasks.len() - complete_count
        )?;
    }

    Ok(out)
}

fn format_relative(dt: &DateTime<Local>) -> String {
    let now = Local::now();
    let diff = now.signed_duration_since(*dt);
    let secs = diff.num_seconds();

    if secs < 60 {
        format!("{} (just now)", dt.format("%Y-%m-%d %H:%M"))
    } else if secs < 3600 {
        format!(
            "{} ({} minutes ago)",
            dt.format("%Y-%m-%d %H:%M"),
            secs / 60
        )
    } else if secs < 86400 {
        format!(
            "{} ({} hours ago)",
            dt.format("%Y-%m-%d %H:%M"),
            secs / 3600
        )
    } else {
        format!(
            "{} ({} days ago)",
            dt.format("%Y-%m-%d %H:%M"),
            secs / 86400
        )
    }
}
