use crate::config;
use crate::fs::reader;
use crate::state::TaskState;
use anyhow::Result;
use chrono::{DateTime, Local};
use colored::Colorize;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use walkdir::WalkDir;

pub fn run(task_id: Option<String>, json: bool, short: bool) -> Result<()> {
    let out = render(task_id, json, short)?;
    print!("{}", out);
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalRow {
    pub project: String,
    pub task_id: String,
    pub state: String,
    pub flow: String,
    pub active_agent: Option<String>,
}

pub fn run_global(timeout_ms: u64, json: bool) -> Result<()> {
    let (rows, warnings) = collect_global(timeout_ms)?;

    for w in &warnings {
        eprintln!("warning: {w}");
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        print_global_table(&rows);
    }
    Ok(())
}

pub fn collect_global(timeout_ms: u64) -> Result<(Vec<GlobalRow>, Vec<String>)> {
    let registry = crate::registry::io::load()?;
    let mut rows: Vec<GlobalRow> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for entry in &registry.projects {
        let path = entry.path.clone();
        let name = entry.name.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = scan_project_tasks(&path);
            let _ = tx.send(result);
        });
        match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok(Ok(tasks)) => {
                for t in tasks {
                    rows.push(GlobalRow {
                        project: name.clone(),
                        task_id: t.task_id,
                        state: t.state,
                        flow: t.flow,
                        active_agent: t.active_agent,
                    });
                }
            }
            Ok(Err(e)) => {
                warnings.push(format!("skip {name}: {e}"));
            }
            Err(_) => {
                warnings.push(format!("skip {name}: timeout after {timeout_ms}ms"));
            }
        }
    }

    Ok((rows, warnings))
}

struct TaskSummary {
    task_id: String,
    state: String,
    flow: String,
    active_agent: Option<String>,
}

fn scan_project_tasks(project_path: &Path) -> Result<Vec<TaskSummary>> {
    let tasks_dir = project_path.join(".zforge/tasks");
    if !tasks_dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&tasks_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let task_id = entry.file_name().to_string_lossy().into_owned();
        let state_file = entry.path().join(".state.yaml");
        if !state_file.exists() {
            continue;
        }
        let raw = match std::fs::read_to_string(&state_file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: TaskState = match serde_yaml::from_str(&raw) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if parsed.flow.next_after(&parsed.state).is_none() {
            // terminal in flow — exclude
            continue;
        }
        out.push(TaskSummary {
            task_id,
            state: parsed.state.as_str().to_string(),
            flow: parsed.flow.as_str().to_string(),
            // TODO(PR2): read `active_agent` from extended .state.yaml schema.
            active_agent: None,
        });
    }
    Ok(out)
}

fn print_global_table(rows: &[GlobalRow]) {
    let header = ("PROJECT", "TASK", "STATE", "FLOW", "AGENT");
    println!(
        "{:<24} {:<16} {:<18} {:<8} {}",
        header.0, header.1, header.2, header.3, header.4
    );
    for r in rows {
        println!(
            "{:<24} {:<16} {:<18} {:<8} {}",
            r.project,
            r.task_id,
            r.state,
            r.flow,
            r.active_agent.as_deref().unwrap_or("-")
        );
    }
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
    writeln!(out, "  Flow:    {}", ts.flow.as_str())?;
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
    writeln!(out, "  Next:    {}", ts.next_hint())?;
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

    let is_complete = |ts: &TaskState| ts.flow.next_after(&ts.state).is_none();
    let complete_count = tasks.iter().filter(|(_, ts)| is_complete(ts)).count();

    for (id, ts) in &tasks {
        let title = reader::MarkdownFile::read(&tasks_dir.join(id).join("task.md"))
            .ok()
            .and_then(|m| m.get_str("title").map(str::to_string))
            .unwrap_or_default();

        let state_str = ts.state.as_str();
        let colored_state = if is_complete(ts) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{State, TaskState};
    use tempfile::TempDir;

    fn write_task(tasks_dir: &std::path::Path, id: &str, state: State) {
        let dir = tasks_dir.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let mut ts = TaskState::new(id);
        // Walk the FSM to the requested state.
        let path = [
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
            State::PlanReviewed,
            State::Coded,
            State::Verified,
            State::Reviewed,
        ];
        for step in path {
            if ts.state >= state {
                break;
            }
            ts.advance(step, "test").unwrap();
        }
        ts.save(tasks_dir).unwrap();
    }

    #[test]
    fn render_single_short_returns_just_state() {
        let tmp = TempDir::new().unwrap();
        write_task(tmp.path(), "TASK-1", State::Coded);
        let out = render_single(tmp.path(), "TASK-1", false, true).unwrap();
        assert_eq!(out.trim(), "Coded");
    }

    #[test]
    fn render_single_json_includes_state() {
        let tmp = TempDir::new().unwrap();
        write_task(tmp.path(), "TASK-1", State::Planned);
        let out = render_single(tmp.path(), "TASK-1", true, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["task_id"], "TASK-1");
        assert_eq!(v["state"], "Planned");
    }

    #[test]
    fn render_single_missing_task_errors() {
        let tmp = TempDir::new().unwrap();
        let err = render_single(tmp.path(), "TASK-X", false, true).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn render_all_empty_dir_reports_no_tasks() {
        let tmp = TempDir::new().unwrap();
        let out = render_all(tmp.path(), false, false).unwrap();
        assert!(out.contains("No tasks found"));
    }

    #[test]
    fn render_all_missing_dir_reports_no_tasks() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let out = render_all(&missing, false, false).unwrap();
        assert!(out.contains("No tasks found"));
    }

    #[test]
    fn render_all_json_lists_every_task() {
        let tmp = TempDir::new().unwrap();
        write_task(tmp.path(), "TASK-1", State::SpecDone);
        write_task(tmp.path(), "TASK-2", State::Reviewed);
        let out = render_all(tmp.path(), true, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let ids: Vec<&str> = arr.iter().map(|t| t["task_id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"TASK-1"));
        assert!(ids.contains(&"TASK-2"));
    }

    #[test]
    fn render_all_short_lists_ids_and_states() {
        let tmp = TempDir::new().unwrap();
        write_task(tmp.path(), "TASK-7", State::Reviewed);
        let out = render_all(tmp.path(), false, true).unwrap();
        assert!(out.contains("TASK-7"));
        assert!(out.contains("Reviewed"));
    }

    #[test]
    fn render_all_skips_non_directory_entries() {
        let tmp = TempDir::new().unwrap();
        write_task(tmp.path(), "TASK-1", State::SpecDone);
        std::fs::write(tmp.path().join("stray-file.txt"), "ignore").unwrap();
        let out = render_all(tmp.path(), true, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
    }
}
