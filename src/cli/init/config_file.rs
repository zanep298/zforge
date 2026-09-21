//! `.zforge/config.yaml` ownership (FIX-017).
//!
//! `init --force` used to rewrite the whole file from the template, and
//! `models.yaml` with it — so the documented refresh flow ("edit
//! models.yaml, then `init --force`") reset the very models it was meant to
//! apply. Refresh now leaves user-owned content alone:
//!
//! - zforge-managed keys — `runner.default`, `paths.agents`,
//!   `paths.skills` — are set from the current init, because they encode
//!   the choices made by this run (`--agent`, `--default-runner`, local vs
//!   shared);
//! - everything else (project name, language, test command, review
//!   settings, other paths) is only written when the file is created.

use super::store_paths::StorePaths;
use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::path::Path;

/// Template for a fresh config. Placeholders are filled by [`render_fresh`].
const CONFIG_TEMPLATE: &str = r#"project:
  name: ""
  language: "{{language}}"
  test_command: "{{test_command}}"
  root_dir: "."
opencode:
  model: "claude-sonnet-4-6"
  context_files: []
runner:
  default: "{{default_runner}}"
paths:
  tasks: "./.zforge/tasks"
  agents: "{{paths_agents}}"
  memory: "./.zforge/memory"
  skills: "{{paths_skills}}"
review:
  auto_approve: false
"#;

pub(crate) struct Managed<'a> {
    pub default_runner: &'a str,
    pub paths: &'a StorePaths,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConfigWrite {
    Created,
    /// Existing file kept; managed keys changed.
    Updated,
    /// Existing file kept; managed keys already matched.
    Unchanged,
}

pub(crate) fn render_fresh(language: &str, test_command: &str, managed: &Managed<'_>) -> String {
    CONFIG_TEMPLATE
        .replace("{{language}}", &yaml_escape(language))
        .replace("{{test_command}}", &yaml_escape(test_command))
        .replace("{{default_runner}}", &yaml_escape(managed.default_runner))
        .replace(
            "{{paths_agents}}",
            &yaml_escape(&managed.paths.config_agents),
        )
        .replace(
            "{{paths_skills}}",
            &yaml_escape(&managed.paths.config_skills),
        )
}

/// Create the config, or update only the managed keys of an existing one.
pub(crate) fn write_config(
    path: &Path,
    language: &str,
    test_command: &str,
    managed: &Managed<'_>,
) -> Result<ConfigWrite> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = render_fresh(language, test_command, managed);
        crate::state::write_atomic(path, content.as_bytes())?;
        return Ok(ConfigWrite::Created);
    }

    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path:?}"))?;
    let mut doc: Value = serde_yaml::from_str(&raw).with_context(|| {
        format!("{path:?} is not valid YAML; fix or delete it before re-running init")
    })?;
    let before = doc.clone();
    set(&mut doc, &["runner", "default"], managed.default_runner);
    set(&mut doc, &["paths", "agents"], &managed.paths.config_agents);
    set(&mut doc, &["paths", "skills"], &managed.paths.config_skills);
    if doc == before {
        return Ok(ConfigWrite::Unchanged);
    }
    let out = serde_yaml::to_string(&doc)?;
    crate::state::write_atomic(path, out.as_bytes())?;
    Ok(ConfigWrite::Updated)
}

/// Set a nested string key, creating intermediate mappings as needed.
fn set(doc: &mut Value, keys: &[&str], value: &str) {
    if !doc.is_mapping() {
        *doc = Value::Mapping(Mapping::new());
    }
    let mut node = doc;
    for (i, key) in keys.iter().enumerate() {
        let map = node.as_mapping_mut().expect("ensured mapping");
        let k = Value::String((*key).to_string());
        if i == keys.len() - 1 {
            map.insert(k, Value::String(value.to_string()));
            return;
        }
        let child = map
            .entry(k)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if !child.is_mapping() {
            *child = Value::Mapping(Mapping::new());
        }
        node = child;
    }
}

/// Escape for a double-quoted YAML scalar.
fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_from;

    fn managed<'a>(runner: &'a str, paths: &'a StorePaths) -> Managed<'a> {
        Managed {
            default_runner: runner,
            paths,
        }
    }

    #[test]
    fn fresh_config_parses_and_carries_all_values() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".zforge/config.yaml");
        let paths = StorePaths::shared(Path::new("/opt/zf"), None);
        let w = write_config(&path, "go", "go test ./...", &managed("codex", &paths)).unwrap();
        assert_eq!(w, ConfigWrite::Created);

        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.language, "go");
        assert_eq!(cfg.project.test_command, "go test ./...");
        assert_eq!(cfg.default_runner(), "codex");
        assert_eq!(cfg.paths.skills, Path::new("/opt/zf/skills"));
        assert_eq!(cfg.paths.agents, Path::new("/opt/zf/agents"));
    }

    // FIX-017: re-init keeps what the user edited, updates what init owns.
    #[test]
    fn reinit_preserves_user_keys_and_updates_managed_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".zforge/config.yaml");
        let local = StorePaths::local();
        write_config(&path, "rust", "cargo test", &managed("claude", &local)).unwrap();

        // User edits.
        let edited = std::fs::read_to_string(&path)
            .unwrap()
            .replace("name: \"\"", "name: \"my-app\"")
            .replace("cargo test", "cargo nextest run")
            .replace("auto_approve: false", "auto_approve: true");
        std::fs::write(&path, edited).unwrap();

        let shared = StorePaths::shared(Path::new("/opt/zf"), None);
        let w = write_config(&path, "rust", "cargo test", &managed("codex", &shared)).unwrap();
        assert_eq!(w, ConfigWrite::Updated);

        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.name, "my-app");
        assert_eq!(cfg.project.test_command, "cargo nextest run");
        assert!(cfg.review.auto_approve);
        assert_eq!(cfg.default_runner(), "codex");
        assert_eq!(cfg.paths.skills, Path::new("/opt/zf/skills"));
    }

    #[test]
    fn reinit_with_same_choices_leaves_the_file_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".zforge/config.yaml");
        let local = StorePaths::local();
        write_config(&path, "rust", "cargo test", &managed("claude", &local)).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let w = write_config(&path, "rust", "cargo test", &managed("claude", &local)).unwrap();
        assert_eq!(w, ConfigWrite::Unchanged);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn invalid_existing_yaml_is_reported_not_clobbered() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.yaml");
        std::fs::write(&path, "project: [unclosed").unwrap();
        let local = StorePaths::local();
        let err =
            write_config(&path, "rust", "cargo test", &managed("claude", &local)).unwrap_err();
        assert!(format!("{err:#}").contains("not valid YAML"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "project: [unclosed"
        );
    }

    #[test]
    fn test_command_with_quotes_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.yaml");
        let local = StorePaths::local();
        write_config(
            &path,
            "shell",
            r#"sh -c "echo \"x\"""#,
            &managed("claude", &local),
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.test_command, r#"sh -c "echo \"x\"""#);
    }
}
