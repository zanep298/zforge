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
//! - everything else (project name, language, test command, execution
//!   policy, knowledge baseline) is only written when the file is created.
//!
//! Managed keys are edited in the text itself, so the user's comments and
//! layout survive a re-init ([`edit_in_place`]).

use super::store_paths::StorePaths;
use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::path::Path;

/// Template for a fresh config. Placeholders are filled by [`render_fresh`].
const CONFIG_TEMPLATE: &str = r#"project:
  name: ""
  language: "{{language}}"
  test_command: "{{test_command}}"
runner:
  default: "{{default_runner}}"
paths:
  agents: "{{paths_agents}}"
  skills: "{{paths_skills}}"
execution:
  # Code → verify attempts per run.
  max_iterations: 3
  # What each task may spend, in USD, across all its runs in a handover.
  # `zforge readiness` refuses a handover until this is set.
  # budget_usd: 5.0
  # Have an agent review passing work against its contract before it counts.
  # review: true
knowledge:
  # The branch "integrated" is judged against.
  baseline: main
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
        crate::fs::write_atomic(path, content.as_bytes())?;
        return Ok(ConfigWrite::Created);
    }

    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path:?}"))?;
    let mut doc: Value = serde_yaml::from_str(&raw).with_context(|| {
        format!("{path:?} is not valid YAML; fix or delete it before re-running init")
    })?;
    let before = doc.clone();
    let updates = [
        ("runner", "default", managed.default_runner),
        ("paths", "agents", managed.paths.config_agents.as_str()),
        ("paths", "skills", managed.paths.config_skills.as_str()),
    ];
    for (section, key, value) in updates {
        set(&mut doc, &[section, key], value);
    }
    if doc == before {
        return Ok(ConfigWrite::Unchanged);
    }
    let out =
        edit_in_place(&raw, &updates, &doc).map_or_else(|| serde_yaml::to_string(&doc), Ok)?;
    crate::fs::write_atomic(path, out.as_bytes())?;
    Ok(ConfigWrite::Updated)
}

/// Apply `updates` to the text itself so comments, quoting and key order
/// survive — a `serde_yaml` round trip drops all comments. Only the value
/// of each managed line changes (a trailing comment stays); a missing key
/// or section is added. Returns `None` when the edited text does not parse
/// to `expected` (flow style, block scalars, anchors…); the caller then
/// falls back to re-serializing, which keeps the values but not the layout.
fn edit_in_place(raw: &str, updates: &[(&str, &str, &str)], expected: &Value) -> Option<String> {
    let mut lines: Vec<String> = raw.lines().map(str::to_string).collect();
    for (section, key, value) in updates {
        set_line(&mut lines, section, key, value);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    let parsed: Value = serde_yaml::from_str(&out).ok()?;
    (parsed == *expected).then_some(out)
}

/// Set `section.key` in block-style YAML lines.
fn set_line(lines: &mut Vec<String>, section: &str, key: &str, value: &str) {
    let quoted = format!("\"{}\"", yaml_escape(value));
    let is_content = |l: &str| !l.trim().is_empty() && !l.trim_start().starts_with('#');
    let Some(header) = lines.iter().position(|l| {
        indent(l) == 0 && value_after_key(l, section).is_some_and(|v| strip_comment(v).0.is_empty())
    }) else {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("{section}:"));
        lines.push(format!("  {key}: {quoted}"));
        return;
    };
    let end = (header + 1..lines.len())
        .find(|&i| is_content(&lines[i]) && indent(&lines[i]) == 0)
        .unwrap_or(lines.len());
    let child_indent = (header + 1..end)
        .find(|&i| is_content(&lines[i]))
        .map_or(2, |i| indent(&lines[i]));
    let pad = " ".repeat(child_indent);
    let existing = (header + 1..end)
        .find(|&i| indent(&lines[i]) == child_indent && value_after_key(&lines[i], key).is_some());
    match existing {
        Some(i) => {
            let rest = value_after_key(&lines[i], key).unwrap_or_default();
            let comment = strip_comment(rest).1.to_string();
            lines[i] = format!("{pad}{key}: {quoted}{comment}");
        }
        None => lines.insert(header + 1, format!("{pad}{key}: {quoted}")),
    }
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// For a `key: value` line, the text after the colon.
fn value_after_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.trim_start().strip_prefix(key)?.strip_prefix(':')?;
    (rest.is_empty() || rest.starts_with([' ', '\t'])).then_some(rest)
}

/// Split a value into (trimmed value, trailing comment with its leading
/// whitespace). A `#` counts as a comment only outside quotes and after
/// whitespace, as in YAML.
fn strip_comment(rest: &str) -> (&str, &str) {
    let mut quote = None;
    let mut prev_ws = true;
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '#') if prev_ws => {
                // A prefix of `rest`, so its length is where the comment's
                // leading whitespace starts.
                let value = rest[..i].trim_end();
                return (value.trim_start(), &rest[value.len()..]);
            }
            _ => {}
        }
        prev_ws = c.is_whitespace();
    }
    (rest.trim(), "")
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
            .replace("max_iterations: 3", "max_iterations: 5");
        std::fs::write(&path, edited).unwrap();

        let shared = StorePaths::shared(Path::new("/opt/zf"), None);
        let w = write_config(&path, "rust", "cargo test", &managed("codex", &shared)).unwrap();
        assert_eq!(w, ConfigWrite::Updated);

        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.name, "my-app");
        assert_eq!(cfg.project.test_command, "cargo nextest run");
        assert_eq!(cfg.execution.max_iterations, 5);
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

    /// Comments are part of what the user owns: re-init used to round-trip
    /// the file through `serde_yaml`, which drops every one of them.
    #[test]
    fn reinit_keeps_comments_and_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.yaml");
        let original = r#"# my project config
project:
  name: app # shown in status
  test_command: 'cargo nextest run' # faster

runner:
  # pick the runner here
  default: claude   # the usual
paths:
  tasks: ./.zforge/tasks
  agents: ./.zforge/agents
  skills: ./.zforge/skills
"#;
        std::fs::write(&path, original).unwrap();

        let shared = StorePaths::shared(Path::new("/opt/zf"), None);
        let w = write_config(&path, "rust", "cargo test", &managed("codex", &shared)).unwrap();
        assert_eq!(w, ConfigWrite::Updated);

        let out = std::fs::read_to_string(&path).unwrap();
        let expected = original
            .replace(
                "default: claude   # the usual",
                "default: \"codex\"   # the usual",
            )
            .replace("agents: ./.zforge/agents", "agents: \"/opt/zf/agents\"")
            .replace("skills: ./.zforge/skills", "skills: \"/opt/zf/skills\"");
        assert_eq!(out, expected);
    }

    #[test]
    fn reinit_adds_a_missing_managed_section_without_touching_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.yaml");
        let original = "# keep me\nproject:\n  name: app\npaths:\n  tasks: ./t # tasks\n";
        std::fs::write(&path, original).unwrap();

        let local = StorePaths::local();
        write_config(&path, "rust", "cargo test", &managed("claude", &local)).unwrap();

        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.starts_with("# keep me\n"), "{out}");
        assert!(out.contains("  tasks: ./t # tasks\n"), "{out}");
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.default_runner(), "claude");
        assert_eq!(cfg.paths.skills, Path::new(&local.config_skills));
    }

    /// Layouts the line editor does not handle still get correct values.
    #[test]
    fn reinit_of_flow_style_yaml_still_sets_managed_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.yaml");
        std::fs::write(
            &path,
            "project: {name: app, test_command: make test}\npaths: {tasks: ./t, agents: a, skills: s}\n",
        )
        .unwrap();

        let shared = StorePaths::shared(Path::new("/opt/zf"), None);
        write_config(&path, "rust", "cargo test", &managed("codex", &shared)).unwrap();

        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.test_command, "make test");
        assert_eq!(cfg.default_runner(), "codex");
        assert_eq!(cfg.paths.agents, Path::new("/opt/zf/agents"));
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
