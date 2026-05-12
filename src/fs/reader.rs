use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

pub struct MarkdownFile {
    pub frontmatter: HashMap<String, serde_yaml::Value>,
    #[allow(dead_code)]
    pub body: String,
    #[allow(dead_code)]
    pub raw: String,
}

impl MarkdownFile {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        parse_markdown(&raw)
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.frontmatter
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.frontmatter.get(key)?.as_str()
    }

    #[allow(dead_code)]
    pub fn get_strings(&self, key: &str) -> Vec<String> {
        self.frontmatter
            .get(key)
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn parse_markdown(raw: &str) -> Result<MarkdownFile> {
    let normalized = raw.replace("\r\n", "\n");

    if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let fm_str = &rest[..end];
            let body = rest[end + 5..].to_string();
            let frontmatter: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(fm_str)
                .map_err(|e| {
                    anyhow::anyhow!("failed to parse frontmatter YAML: {e} — input: {fm_str:?}")
                })?;
            return Ok(MarkdownFile {
                frontmatter,
                body,
                raw: raw.to_string(),
            });
        }
    }

    Ok(MarkdownFile {
        frontmatter: HashMap::new(),
        body: raw.to_string(),
        raw: raw.to_string(),
    })
}

pub fn agent_model(agents_dir: &Path, phase: &str) -> String {
    let path = agents_dir.join(format!("{}-agent.md", phase));
    MarkdownFile::read(&path)
        .ok()
        .and_then(|md| md.get_str("model").map(String::from))
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn agent_codex_model(agents_dir: &Path, phase: &str) -> Option<String> {
    let path = agents_dir.join(format!("{}-agent.md", phase));
    MarkdownFile::read(&path)
        .ok()
        .and_then(|md| md.get_str("codex_model").map(String::from))
}

pub fn artifact_exists(tasks_dir: &Path, task_id: &str, artifact: &str) -> bool {
    let path = tasks_dir.join(task_id).join(artifact);
    if !path.exists() {
        return false;
    }
    std::fs::metadata(&path)
        .map(|m| m.len() > 10)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_markdown_with_frontmatter() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "---\ntitle: hello\n---\n\nbody content").unwrap();
        let md = MarkdownFile::read(f.path()).unwrap();
        assert_eq!(md.get_str("title"), Some("hello"));
        assert!(md.body.contains("body content"));
    }

    #[test]
    fn test_read_markdown_no_frontmatter() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "just body content").unwrap();
        let md = MarkdownFile::read(f.path()).unwrap();
        assert!(md.frontmatter.is_empty());
        assert_eq!(md.body, "just body content");
    }

    #[test]
    fn test_artifact_exists_empty_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let task_dir = tmp.path().join("TASK-1");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("spec.md"), "").unwrap();
        assert!(!artifact_exists(tmp.path(), "TASK-1", "spec.md"));
    }
}
