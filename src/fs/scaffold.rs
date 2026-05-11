use anyhow::Result;
use chrono::Local;
use std::path::Path;

fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Default)]
pub struct TaskImportData {
    pub title: Option<String>,
    pub domain: Option<String>,
    pub affected_services: Vec<String>,
    pub description: Option<String>,
    pub jira_url: Option<String>,
    pub figma_url: Option<String>,
    pub figma_context: Option<String>,
}

#[allow(dead_code)]
pub fn init_zforge(root: &Path) -> Result<()> {
    let zforge = root.join(".zforge");
    std::fs::create_dir_all(zforge.join("agents"))?;
    std::fs::create_dir_all(zforge.join("memory"))?;
    std::fs::create_dir_all(zforge.join("skills"))?;
    std::fs::create_dir_all(zforge.join("tasks"))?;
    Ok(())
}

#[allow(dead_code)]
pub fn scaffold_task(tasks_dir: &Path, task_id: &str) -> Result<()> {
    scaffold_task_with_data(tasks_dir, task_id, &TaskImportData::default())
}

pub fn scaffold_task_with_data(
    tasks_dir: &Path,
    task_id: &str,
    data: &TaskImportData,
) -> Result<()> {
    let dir = tasks_dir.join(task_id);
    std::fs::create_dir_all(&dir)?;
    let task_path = dir.join("task.md");
    let now = Local::now().to_rfc3339();

    let title = escape_yaml(data.title.as_deref().unwrap_or(""));
    let domain = escape_yaml(data.domain.as_deref().unwrap_or(""));

    let services_yaml = if data.affected_services.is_empty() {
        "[]".to_string()
    } else {
        let items = data
            .affected_services
            .iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{items}]")
    };

    let description_body = data
        .description
        .as_deref()
        .unwrap_or("<!-- Describe what this task is and why it needs to be done -->");

    let figma_url_yaml = escape_yaml(data.figma_url.as_deref().unwrap_or(""));

    let notes_lines: Vec<String> = [
        data.jira_url.as_deref().map(|u| format!("Jira: {u}")),
        data.figma_url.as_deref().map(|u| format!("Figma: {u}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let notes_body = if notes_lines.is_empty() {
        "<!-- Additional context, links, references -->".to_string()
    } else {
        notes_lines.join("\n")
    };

    let content = format!(
        r#"---
id: "{task_id}"
title: "{title}"
domain: "{domain}"
affected_services: {services_yaml}
dependencies: []
created_at: "{now}"
figma_url: "{figma_url_yaml}"
---

## Description
{description_body}

## Acceptance Criteria
- [ ]

## Technical Constraints
- Language: rust
- Patterns: TDD

## Notes
{notes_body}
"#
    );

    std::fs::write(task_path, content)?;

    if let Some(figma) = &data.figma_context {
        let provenance = match &data.figma_url {
            Some(url) => format!("<!-- Source: {url} -->\n\n{figma}"),
            None => figma.clone(),
        };
        std::fs::write(dir.join("figma.md"), provenance)?;
    }

    Ok(())
}

pub fn task_exists(tasks_dir: &Path, task_id: &str) -> bool {
    tasks_dir.join(task_id).join("task.md").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn escape_yaml_escapes_double_quotes() {
        assert_eq!(escape_yaml(r#"my "title""#), r#"my \"title\""#);
    }

    #[test]
    fn escape_yaml_escapes_backslashes() {
        assert_eq!(escape_yaml(r"a\b"), r"a\\b");
    }

    #[test]
    fn escape_yaml_leaves_plain_strings_unchanged() {
        assert_eq!(escape_yaml("hello world"), "hello world");
    }

    #[test]
    fn figma_url_written_to_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let data = TaskImportData {
            figma_url: Some("https://figma.com/design/ABC/screen?node-id=1".into()),
            figma_context: Some("## Frame\nsize: 375x812".into()),
            ..Default::default()
        };
        scaffold_task_with_data(tmp.path(), "TASK-001", &data).unwrap();

        let task_md = std::fs::read_to_string(tmp.path().join("TASK-001").join("task.md")).unwrap();
        assert!(task_md.contains("figma_url: \"https://figma.com/design/ABC/screen?node-id=1\""));
    }

    #[test]
    fn figma_context_written_to_figma_md_with_provenance() {
        let tmp = TempDir::new().unwrap();
        let data = TaskImportData {
            figma_url: Some("https://figma.com/design/ABC/screen?node-id=1".into()),
            figma_context: Some("## Frame\nsize: 375x812".into()),
            ..Default::default()
        };
        scaffold_task_with_data(tmp.path(), "TASK-001", &data).unwrap();

        let figma_md =
            std::fs::read_to_string(tmp.path().join("TASK-001").join("figma.md")).unwrap();
        assert!(figma_md.contains("<!-- Source: https://figma.com/design/ABC/screen?node-id=1 -->"));
        assert!(figma_md.contains("## Frame"));
    }

    #[test]
    fn no_figma_md_when_figma_context_absent() {
        let tmp = TempDir::new().unwrap();
        scaffold_task_with_data(tmp.path(), "TASK-001", &TaskImportData::default()).unwrap();
        assert!(!tmp.path().join("TASK-001").join("figma.md").exists());
    }

    #[test]
    fn figma_url_with_quotes_escaped_in_yaml() {
        let tmp = TempDir::new().unwrap();
        let data = TaskImportData {
            figma_url: Some(r#"https://figma.com/design/"special""#.into()),
            ..Default::default()
        };
        scaffold_task_with_data(tmp.path(), "TASK-001", &data).unwrap();
        let task_md = std::fs::read_to_string(tmp.path().join("TASK-001").join("task.md")).unwrap();
        assert!(task_md.contains(r#"figma_url: "https://figma.com/design/\"special\"""#));
    }

    #[test]
    fn figma_context_without_url_has_no_provenance_header() {
        let tmp = TempDir::new().unwrap();
        let data = TaskImportData {
            figma_context: Some("raw context".into()),
            ..Default::default()
        };
        scaffold_task_with_data(tmp.path(), "TASK-001", &data).unwrap();
        let figma_md =
            std::fs::read_to_string(tmp.path().join("TASK-001").join("figma.md")).unwrap();
        assert!(!figma_md.contains("Source:"));
        assert_eq!(figma_md.trim(), "raw context");
    }
}
