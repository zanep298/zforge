use crate::config::ModelsConfig;
use anyhow::Result;
use indexmap::IndexMap;
use std::path::Path;

pub struct MarkdownFile {
    pub frontmatter: IndexMap<String, serde_yaml::Value>,
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
            let frontmatter: IndexMap<String, serde_yaml::Value> = serde_yaml::from_str(fm_str)
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
        frontmatter: IndexMap::new(),
        body: raw.to_string(),
        raw: raw.to_string(),
    })
}

pub fn agent_model(agents_dir: &Path, phase: &str) -> String {
    let models = crate::config::load_models();
    agent_model_for_phase_with_models(agents_dir, "claude", phase, models.as_ref())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn agent_codex_model(agents_dir: &Path, phase: &str) -> Option<String> {
    let models = crate::config::load_models();
    agent_model_for_phase_with_models(agents_dir, "codex", phase, models.as_ref())
}

pub fn agent_opencode_model(agents_dir: &Path, phase: &str) -> Option<String> {
    let models = crate::config::load_models();
    agent_model_for_phase_with_models(agents_dir, "opencode", phase, models.as_ref())
}

/// Resolve the model to pass as `--model` when dispatching to an assistant.
/// Routes through the per-assistant reader so all callers share one path.
pub fn agent_model_for_dispatch(agents_dir: &Path, assistant: &str, phase: &str) -> Option<String> {
    let models = crate::config::load_models();
    agent_model_for_phase_with_models(agents_dir, assistant, phase, models.as_ref())
}

/// Resolve an assistant-specific model for one workflow phase.
///
/// Precedence:
/// 1. `models.yaml` for `(assistant, phase)`.
/// 2. Agent template frontmatter for that assistant.
///
/// Codex/OpenCode rendered agent directories contain a single `model:` key,
/// while the canonical `.zforge/agents` templates contain `codex_model:` and
/// `opencode_model:`. Accept both so CLI dispatch, artifact metadata, and
/// orchestrator telemetry can share one resolver.
pub fn agent_model_for_phase_with_models(
    agents_dir: &Path,
    assistant: &str,
    phase: &str,
    models: Option<&ModelsConfig>,
) -> Option<String> {
    if let Some(m) = models.and_then(|m| m.for_assistant(assistant, phase)) {
        return Some(m.to_string());
    }

    let path = agents_dir.join(format!("{}-agent.md", phase));
    let md = MarkdownFile::read(&path).ok()?;
    let model = match assistant {
        "claude" => md.get_str("model"),
        "codex" => md.get_str("codex_model").or_else(|| md.get_str("model")),
        "opencode" => md.get_str("opencode_model").or_else(|| md.get_str("model")),
        "agy" => md.get_str("agy_model"),
        other => {
            let key = format!("{other}_model");
            md.get_str(&key)
        }
    };
    model.map(String::from)
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

    #[test]
    fn agent_model_resolves_assistant_specific_frontmatter() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("plan-agent.md"),
            "---\nmodel: claude-sonnet\ncodex_model: gpt-5-codex\nopencode_model: qwen\n---\n",
        )
        .unwrap();

        assert_eq!(
            agent_model_for_phase_with_models(tmp.path(), "claude", "plan", None).as_deref(),
            Some("claude-sonnet")
        );
        assert_eq!(
            agent_model_for_phase_with_models(tmp.path(), "codex", "plan", None).as_deref(),
            Some("gpt-5-codex")
        );
        assert_eq!(
            agent_model_for_phase_with_models(tmp.path(), "opencode", "plan", None).as_deref(),
            Some("qwen")
        );
    }

    #[test]
    fn unknown_agent_does_not_fall_back_to_claude_model_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("plan-agent.md"),
            "---\nmodel: claude-sonnet\n---\n",
        )
        .unwrap();

        assert!(
            agent_model_for_phase_with_models(tmp.path(), "custom", "plan", None).is_none(),
            "custom agents need models.yaml or custom_model frontmatter"
        );
    }
}
