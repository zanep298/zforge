use crate::config;
use crate::fs::scaffold::{self, TaskImportData};
use crate::fs::{tokens, writer};
use crate::state::TaskState;
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::path::Path;

fn validate_task_id(id: &str) -> bool {
    let re = Regex::new(r"^[A-Z][A-Z0-9]*-[0-9]+$").unwrap();
    re.is_match(id)
}

fn next_task_id(tasks_dir: &Path) -> Result<String> {
    let re = Regex::new(r"^([A-Z]+)-([0-9]+)$").unwrap();
    let mut max_num: u32 = 0;

    if tasks_dir.is_dir() {
        for entry in std::fs::read_dir(tasks_dir)? {
            let name = entry?.file_name();
            let s = name.to_string_lossy();
            if let Some(caps) = re.captures(&s) {
                if let Ok(n) = caps[2].parse::<u32>() {
                    max_num = max_num.max(n);
                }
            }
        }
    }

    Ok(format!("TASK-{:03}", max_num + 1))
}

pub fn run_import(
    task_id: Option<&str>,
    title: Option<String>,
    domain: Option<String>,
    description: Option<String>,
    jira_url: Option<String>,
    figma_url: Option<String>,
    figma_context: Option<String>,
) -> Result<String> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;

    let tasks_dir = config.tasks_dir();
    std::fs::create_dir_all(&tasks_dir)?;

    let (id, data) = if let Some(ref url) = jira_url {
        let ticket = crate::jira::fetch_ticket(url)?;
        let resolved_id = task_id.map(str::to_string).unwrap_or(ticket.key.clone());
        let import_data = TaskImportData {
            title: title.or(Some(ticket.summary)),
            domain: domain.or_else(|| ticket.components.first().cloned()),
            affected_services: ticket.components,
            description: description.or(ticket.description),
            jira_url: Some(url.clone()),
            figma_url,
            figma_context,
        };
        (resolved_id, import_data)
    } else {
        let resolved_id = match task_id {
            Some(id) => {
                if !validate_task_id(id) {
                    anyhow::bail!(
                        "Invalid task ID '{}'. Format: PROJECT-NUMBER (e.g. TASK-123)",
                        id
                    );
                }
                id.to_string()
            }
            None => next_task_id(&tasks_dir)?,
        };
        let import_data = TaskImportData {
            title,
            domain,
            description,
            figma_url,
            figma_context,
            ..Default::default()
        };
        (resolved_id, import_data)
    };

    if scaffold::task_exists(&tasks_dir, &id) {
        anyhow::bail!("Task {} already exists. Use: zf status {}", id, id);
    }

    scaffold::scaffold_task_with_data(&tasks_dir, &id, &config.project.language, &data)?;

    let task_path = tasks_dir.join(&id).join("task.md");
    let task_tokens = tokens::estimate(&std::fs::read_to_string(&task_path).unwrap_or_default());
    writer::set_frontmatter(
        &task_path,
        "tokens",
        serde_yaml::Value::Number(task_tokens.into()),
    )?;
    writer::set_frontmatter(
        &task_path,
        "model",
        serde_yaml::Value::String("human".to_string()),
    )?;

    let state = TaskState::new(&id);
    state.save(&tasks_dir)?;

    println!("{} Created tasks/{}/", "✓".green(), id);
    println!(
        "{} Created tasks/{}/task.md  ({} tokens)",
        "✓".green(),
        id,
        tokens::fmt(task_tokens)
    );
    println!("{} State: Imported", "✓".green());
    println!();
    println!("{}", "─".repeat(40));

    if jira_url.is_some() {
        println!(
            "{} Imported from Jira. Review tasks/{}/task.md",
            "📥".bold(),
            id
        );
    } else {
        println!("{} Fill in task details:", "📝".bold());
        println!("   tasks/{}/task.md", id);
    }

    println!();
    println!("When done, run: zf spec {}", id);
    println!("{}", "─".repeat(40));

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validate_accepts_valid_ids() {
        assert!(validate_task_id("TASK-001"));
        assert!(validate_task_id("AUTH-42"));
        assert!(validate_task_id("A-1"));
        assert!(validate_task_id("PROJ-123"));
    }

    #[test]
    fn validate_rejects_invalid_ids() {
        assert!(!validate_task_id("task-001"));
        assert!(!validate_task_id("TASK001"));
        assert!(!validate_task_id("TASK-"));
        assert!(!validate_task_id("-001"));
        assert!(!validate_task_id(""));
    }

    #[test]
    fn next_task_id_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let id = next_task_id(tmp.path()).unwrap();
        assert_eq!(id, "TASK-001");
    }

    #[test]
    fn next_task_id_increments_from_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("TASK-003")).unwrap();
        std::fs::create_dir(tmp.path().join("TASK-001")).unwrap();
        let id = next_task_id(tmp.path()).unwrap();
        assert_eq!(id, "TASK-004");
    }

    #[test]
    fn next_task_id_ignores_non_matching_dirs() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("not-a-task")).unwrap();
        std::fs::create_dir(tmp.path().join("TASK-002")).unwrap();
        let id = next_task_id(tmp.path()).unwrap();
        assert_eq!(id, "TASK-003");
    }
}
