//! `.claude/settings.json` ownership.
//!
//! The file is shared: zforge seeds `permissions.allow` with its own MCP
//! tools and CLI, but users (and other tools) add permissions, hooks, env
//! and more. `init --force` used to replace it with the template, so
//! picking up zforge's current allowlist — e.g. CodeGraph 0.9's tool names —
//! cost every customization in the file. A refresh now merges instead:
//!
//! - every key and allow entry already in the file stays, in its order;
//! - zforge's allow entries that are missing are appended;
//! - entries zforge itself used to write and no longer does
//!   ([`RETIRED_ALLOW`]) are removed — no other entry is ever dropped.
//!
//! Without `--force` an existing file is left alone, as before.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;

pub(crate) const CLAUDE_SETTINGS_JSON: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(zforge *)",
      "mcp__zforge__task_import",
      "mcp__zforge__get_prompt",
      "mcp__zforge__approve",
      "mcp__zforge__verify",
      "mcp__zforge__ship",
      "mcp__zforge__status",
      "mcp__codegraph__codegraph_search",
      "mcp__codegraph__codegraph_context",
      "mcp__codegraph__codegraph_files",
      "mcp__codegraph__codegraph_node",
      "mcp__codegraph__codegraph_explore",
      "mcp__codegraph__codegraph_callers",
      "mcp__codegraph__codegraph_callees",
      "mcp__codegraph__codegraph_impact",
      "mcp__codegraph__codegraph_trace",
      "mcp__codegraph__codegraph_status"
    ]
  }
}
"#;

/// Allow entries earlier zforge versions wrote that name tools which no
/// longer exist (CodeGraph before 0.9).
const RETIRED_ALLOW: [&str; 4] = [
    "mcp__codegraph__query",
    "mcp__codegraph__context",
    "mcp__codegraph__files",
    "mcp__codegraph__affected",
];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SettingsWrite {
    Created,
    /// Existing file kept; zforge's allow entries brought up to date.
    Merged,
    /// Existing file already had them.
    Unchanged,
    /// Existing file left alone (no `--force`).
    Skipped,
}

pub(crate) fn write_settings(path: &Path, force: bool) -> Result<SettingsWrite> {
    if !path.exists() {
        crate::state::write_atomic(path, CLAUDE_SETTINGS_JSON.as_bytes())?;
        return Ok(SettingsWrite::Created);
    }
    if !force {
        return Ok(SettingsWrite::Skipped);
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path:?}"))?;
    let existing: Value = serde_json::from_str(&raw).with_context(|| {
        format!("{path:?} is not valid JSON; fix or delete it before re-running init")
    })?;
    let merged = merge(&existing).with_context(|| format!("{path:?}"))?;
    if merged == existing {
        return Ok(SettingsWrite::Unchanged);
    }
    let mut out = serde_json::to_string_pretty(&merged)?;
    out.push('\n');
    crate::state::write_atomic(path, out.as_bytes())?;
    Ok(SettingsWrite::Merged)
}

fn merge(existing: &Value) -> Result<Value> {
    let template: Value = serde_json::from_str(CLAUDE_SETTINGS_JSON).expect("valid template");
    let wanted = template["permissions"]["allow"]
        .as_array()
        .expect("template has permissions.allow");

    let mut merged = existing.clone();
    let Some(root) = merged.as_object_mut() else {
        bail!("expected a JSON object at the top level");
    };
    let permissions = root
        .entry("permissions")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(permissions) = permissions.as_object_mut() else {
        bail!("`permissions` is not an object");
    };
    let allow = permissions
        .entry("allow")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(allow) = allow.as_array_mut() else {
        bail!("`permissions.allow` is not an array");
    };

    allow.retain(|entry| !entry.as_str().is_some_and(|e| RETIRED_ALLOW.contains(&e)));
    for entry in wanted {
        if !allow.contains(entry) {
            allow.push(entry.clone());
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template_allow() -> Vec<Value> {
        serde_json::from_str::<Value>(CLAUDE_SETTINGS_JSON).unwrap()["permissions"]["allow"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn read(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn missing_file_is_created_from_the_template() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        assert_eq!(
            write_settings(&path, false).unwrap(),
            SettingsWrite::Created
        );
        assert_eq!(read(&path)["permissions"]["allow"], json!(template_allow()));
    }

    #[test]
    fn existing_file_is_left_alone_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{\"mine\": true}\n").unwrap();
        assert_eq!(
            write_settings(&path, false).unwrap(),
            SettingsWrite::Skipped
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"mine\": true}\n"
        );
    }

    /// The bug: `--force` replaced the file, dropping everything the user
    /// had added to it.
    #[test]
    fn force_keeps_user_settings_and_refreshes_zforge_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let user = json!({
            "env": {"FOO": "1"},
            "permissions": {
                "allow": ["Bash(make *)", "mcp__codegraph__query", "mcp__zforge__ship"],
                "deny": ["Bash(rm -rf *)"]
            },
            "hooks": {"Stop": [{"command": "make lint"}]}
        });
        std::fs::write(&path, serde_json::to_string_pretty(&user).unwrap()).unwrap();

        assert_eq!(write_settings(&path, true).unwrap(), SettingsWrite::Merged);

        let out = read(&path);
        assert_eq!(out["env"], user["env"]);
        assert_eq!(out["hooks"], user["hooks"]);
        assert_eq!(out["permissions"]["deny"], user["permissions"]["deny"]);
        let allow = out["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow[0], "Bash(make *)", "user entries keep their place");
        assert_eq!(allow[1], "mcp__zforge__ship");
        assert!(
            !allow.contains(&json!("mcp__codegraph__query")),
            "retired CodeGraph name removed"
        );
        for entry in template_allow() {
            assert_eq!(
                allow.iter().filter(|e| **e == entry).count(),
                1,
                "{entry} present exactly once"
            );
        }
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["env", "permissions", "hooks"], "key order kept");
    }

    #[test]
    fn force_on_an_up_to_date_file_does_not_rewrite_it() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write_settings(&path, false).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            write_settings(&path, true).unwrap(),
            SettingsWrite::Unchanged
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn invalid_json_is_reported_not_clobbered() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = write_settings(&path, true).unwrap_err();
        assert!(format!("{err:#}").contains("not valid JSON"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn unexpected_shape_is_reported_not_clobbered() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{\"permissions\": {\"allow\": \"everything\"}}").unwrap();
        let err = write_settings(&path, true).unwrap_err();
        assert!(format!("{err:#}").contains("not an array"), "{err:#}");
    }
}
