//! Reading Claude Code's own view of its configuration.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// `claude mcp get <name>` parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub status: McpStatus,
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    Connected,
    PendingApproval,
    Failed(String),
    Unknown(String),
}

/// Parse `claude mcp get` output; `None` when the server is not configured.
pub fn parse_mcp_get(output: &str) -> Option<McpServer> {
    if output.contains("No MCP server named") {
        return None;
    }
    let field = |key: &str| {
        output
            .lines()
            .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
    };
    let status_raw = field("Status:")?;
    let status = if status_raw.contains("Connected") {
        McpStatus::Connected
    } else if status_raw.contains("Pending approval") {
        McpStatus::PendingApproval
    } else if status_raw.contains("Failed") || status_raw.starts_with('✗') {
        McpStatus::Failed(status_raw.clone())
    } else {
        McpStatus::Unknown(status_raw.clone())
    };
    Some(McpServer {
        status,
        args: field("Args:").unwrap_or_default(),
    })
}

/// One hook command registered in a Claude settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub event: String,
    pub matcher: String,
    pub command: String,
    pub source: PathBuf,
}

/// Settings files Claude reads for this project, most general first: the
/// user's (`$CLAUDE_CONFIG_DIR` or `~/.claude`), then the project's shared
/// and local files.
pub fn settings_files(project_root: &Path) -> Vec<PathBuf> {
    let user_dir = user_config_dir();
    let mut files = Vec::new();
    if let Some(d) = user_dir {
        files.push(d.join("settings.json"));
    }
    files.push(project_root.join(".claude").join("settings.json"));
    files.push(project_root.join(".claude").join("settings.local.json"));
    files
}

/// Claude Code's global state file, which records per-project trust:
/// `$CLAUDE_CONFIG_DIR/.claude.json`, else `~/.claude.json`.
pub fn state_file() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir).join(".claude.json")),
        None => dirs::home_dir().map(|h| h.join(".claude.json")),
    }
}

/// The directory whose accepted trust dialog covers `project_root` — the
/// project itself or its nearest parent with `hasTrustDialogAccepted` —
/// or `None`. Keys are canonical paths. A parent's trust covers its
/// subdirectories: Claude Code 2.1.278 printed no untrusted warning in a
/// subdirectory without an entry of its own. That an explicit `false` on
/// the subdirectory does not override the parent is assumed, not observed.
pub fn trusted_by(state: &Value, project_root: &Path) -> Option<PathBuf> {
    let projects = state.get("projects")?.as_object()?;
    let root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    root.ancestors()
        .find(|dir| {
            projects
                .get(&dir.display().to_string())
                .and_then(|p| p.get("hasTrustDialogAccepted"))
                .and_then(Value::as_bool)
                == Some(true)
        })
        .map(Path::to_path_buf)
}

/// Every hook command across `files` (missing or unparsable files skipped).
pub fn hooks(files: &[PathBuf]) -> Vec<Hook> {
    files
        .iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(f).ok()?;
            let v: Value = serde_json::from_str(&text).ok()?;
            Some(hooks_in(&v, f))
        })
        .flatten()
        .collect()
}

fn hooks_in(settings: &Value, source: &Path) -> Vec<Hook> {
    let Some(events) = settings.get("hooks").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (event, groups) in events {
        for group in groups.as_array().into_iter().flatten() {
            let matcher = group
                .get("matcher")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for h in group
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(command) = h.get("command").and_then(Value::as_str) {
                    out.push(Hook {
                        event: event.clone(),
                        matcher: matcher.clone(),
                        command: command.to_string(),
                        source: source.to_path_buf(),
                    });
                }
            }
        }
    }
    out
}

/// Claude's user configuration directory: `$CLAUDE_CONFIG_DIR`, else
/// `~/.claude`.
fn user_config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
}

/// Plugins enabled across `files` (`enabledPlugins: {"name@marketplace":
/// true}`); a later file overrides an earlier one, so a project can turn a
/// user's plugin off.
pub fn enabled_plugins(files: &[PathBuf]) -> Vec<String> {
    let mut state: Vec<(String, bool)> = Vec::new();
    for f in files {
        let Some(v) = std::fs::read_to_string(f)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        for (id, on) in v
            .get("enabledPlugins")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let on = on.as_bool().unwrap_or(false);
            match state.iter_mut().find(|(i, _)| i == id) {
                Some(entry) => entry.1 = on,
                None => state.push((id.clone(), on)),
            }
        }
    }
    state
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(id, _)| id)
        .collect()
}

/// Where Claude installed plugin `id`, from
/// `<config dir>/plugins/installed_plugins.json` (the newest entry).
pub fn plugin_install_path(id: &str) -> Option<PathBuf> {
    let file = user_config_dir()?
        .join("plugins")
        .join("installed_plugins.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let entries = v.get("plugins").unwrap_or(&v).get(id)?.as_array()?;
    entries
        .iter()
        .rev()
        .find_map(|e| e.get("installPath").and_then(Value::as_str))
        .map(PathBuf::from)
}

/// The hooks a plugin installed at `root` declares — in
/// `.claude-plugin/plugin.json` or `hooks/hooks.json` — with
/// `${CLAUDE_PLUGIN_ROOT}` resolved to `root`.
pub fn plugin_hooks(root: &Path) -> Vec<Hook> {
    let root_text = root.display().to_string();
    [
        root.join(".claude-plugin").join("plugin.json"),
        root.join("hooks").join("hooks.json"),
    ]
    .iter()
    .filter_map(|f| {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
        Some(hooks_in(&v, f))
    })
    .flatten()
    .map(|mut h| {
        h.command = h
            .command
            .replace("${CLAUDE_PLUGIN_ROOT}", &root_text)
            .replace("$CLAUDE_PLUGIN_ROOT", &root_text);
        h
    })
    .collect()
}

/// Scripts a plugin hook runs, resolved under the plugin's `root`: the
/// absolute paths [`referenced_scripts`] finds, plus `$VAR/…` paths, since
/// plugins commonly copy `CLAUDE_PLUGIN_ROOT` into a variable of their own
/// first (caveman: `HOOK_ROOT=…; node "$HOOK_ROOT/src/hooks/…"`).
pub fn plugin_scripts(command: &str, root: &Path) -> Vec<PathBuf> {
    let mut out = referenced_scripts(command);
    for token in shlex::split(command).unwrap_or_default() {
        let is_script = [".js", ".sh", ".py", ".mjs"]
            .iter()
            .any(|e| token.ends_with(e));
        if let (true, Some(rest)) = (is_script, token.strip_prefix('$')) {
            if let Some((_, path)) = rest.split_once('/') {
                out.push(root.join(path));
            }
        }
    }
    out
}

/// Script paths a hook command points at (quoted or bare absolute paths
/// ending in `.js`/`.sh`/`.py`), so their existence can be checked.
pub fn referenced_scripts(command: &str) -> Vec<PathBuf> {
    shlex::split(command)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            t.starts_with('/') && [".js", ".sh", ".py", ".mjs"].iter().any(|e| t.ends_with(e))
        })
        .map(PathBuf::from)
        .collect()
}

/// `claude plugin validate <dir>` output → issues per validated file.
/// Only files with problems are listed by Claude; an empty result with a
/// "Validation passed" line means everything in the directory is clean.
pub fn parse_validate(output: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in output.lines() {
        let t = line.trim();
        if let Some(path) = t
            .strip_prefix("Validating skill:")
            .or_else(|| t.strip_prefix("Validating agent:"))
        {
            out.push((path.trim().to_string(), Vec::new()));
        } else if let Some(issue) = t.strip_prefix('❯') {
            if let Some((_, issues)) = out.last_mut() {
                issues.push(issue.trim().to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_plugins_hook_scripts_resolve_under_its_root() {
        let cmd = r#"HOOK_ROOT=$(printf %s "/p/caveman" | sed 's|x|y|'); node "$HOOK_ROOT/src/hooks/caveman-activate.js""#;
        assert_eq!(
            plugin_scripts(cmd, Path::new("/p/caveman")),
            [PathBuf::from("/p/caveman/src/hooks/caveman-activate.js")]
        );
    }

    #[test]
    fn a_later_settings_file_turns_a_plugin_off() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user.json");
        let project = dir.path().join("project.json");
        std::fs::write(
            &user,
            r#"{"enabledPlugins":{"caveman@caveman":true,"x@m":true}}"#,
        )
        .unwrap();
        std::fs::write(&project, r#"{"enabledPlugins":{"x@m":false}}"#).unwrap();
        assert_eq!(enabled_plugins(&[user, project]), ["caveman@caveman"]);
    }

    #[test]
    fn trust_is_found_on_the_project_or_its_nearest_trusted_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().canonicalize().unwrap();
        let project = parent.join("a").join("b");
        std::fs::create_dir_all(&project).unwrap();
        let state = |entries: &[(&Path, bool)]| {
            let projects: serde_json::Map<String, Value> = entries
                .iter()
                .map(|(p, ok)| {
                    (
                        p.display().to_string(),
                        serde_json::json!({ "hasTrustDialogAccepted": ok }),
                    )
                })
                .collect();
            serde_json::json!({ "projects": projects })
        };

        assert_eq!(
            trusted_by(&state(&[(&project, true)]), &project),
            Some(project.clone())
        );
        assert_eq!(
            trusted_by(&state(&[(&project, false), (&parent, true)]), &project),
            Some(parent.clone()),
            "an explicit false on the project does not hide a trusted parent"
        );
        assert_eq!(trusted_by(&state(&[(&project, false)]), &project), None);
        assert_eq!(trusted_by(&serde_json::json!({}), &project), None);
    }

    use super::*;

    #[test]
    fn parses_a_connected_server() {
        let out = "codegraph:\n  Scope: Local config (private to you in this project)\n  Status: ✔ Connected\n  Type: stdio\n  Command: codegraph\n  Args: serve --mcp --path /p/app\n  Environment:\n";
        let s = parse_mcp_get(out).unwrap();
        assert_eq!(s.status, McpStatus::Connected);
        assert_eq!(s.args, "serve --mcp --path /p/app");
    }

    #[test]
    fn parses_pending_and_failed_and_missing() {
        let pending = "x:\n  Status: ⏸ Pending approval (run `claude` to approve)\n  Args: a\n";
        assert_eq!(
            parse_mcp_get(pending).unwrap().status,
            McpStatus::PendingApproval
        );
        let failed = "x:\n  Status: ✗ Failed to connect\n";
        assert!(matches!(
            parse_mcp_get(failed).unwrap().status,
            McpStatus::Failed(_)
        ));
        assert_eq!(
            parse_mcp_get("No MCP server named \"x\". Configured servers: a"),
            None
        );
    }

    #[test]
    fn collects_hooks_from_settings() {
        let v: Value = serde_json::from_str(
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}],
                "SessionStart":[{"hooks":[{"type":"command","command":"\"/usr/bin/node\" \"/h/.claude/hooks/caveman-activate.js\""}]}]}}"#,
        )
        .unwrap();
        let hooks = hooks_in(&v, Path::new("s.json"));
        assert_eq!(hooks.len(), 2);
        let rtk = hooks.iter().find(|h| h.event == "PreToolUse").unwrap();
        assert_eq!(
            (rtk.matcher.as_str(), rtk.command.as_str()),
            ("Bash", "rtk hook claude")
        );
        let cave = hooks.iter().find(|h| h.event == "SessionStart").unwrap();
        assert_eq!(
            referenced_scripts(&cave.command),
            vec![PathBuf::from("/h/.claude/hooks/caveman-activate.js")]
        );
    }

    #[test]
    fn parses_validate_issues_per_file() {
        let out = "Validating components in: /p/.claude/skills\n\nValidating skill: /p/.claude/skills/mine/SKILL.md\n\n⚠ Found 1 warning:\n\n  ❯ frontmatter: No frontmatter block found.\n\nValidating skill: /p/.claude/skills/zforge-x/SKILL.md\n\n  ❯ description: No description in frontmatter.\n\n✔ Validation passed with warnings\n";
        let v = parse_validate(out);
        assert_eq!(v.len(), 2);
        assert!(v[1].0.ends_with("zforge-x/SKILL.md"));
        assert_eq!(v[1].1, vec!["description: No description in frontmatter."]);
        assert!(parse_validate("Validating components in: /p\n\n✔ Validation passed\n").is_empty());
    }
}
