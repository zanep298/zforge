//! `.claude/settings.json` ownership.
//!
//! The file is shared: zforge seeds `permissions.allow` with its own MCP
//! tools and CLI, but users (and other tools) add permissions, hooks, env
//! and more. `init --force` used to replace it with the template, so
//! picking up zforge's current allowlist — e.g. CodeGraph 0.9's tool names —
//! cost every customization in the file. A refresh now merges instead:
//!
//! - every key and allow entry already in the file stays, in its order;
//! - zforge's allow and deny entries that are missing are appended (deny
//!   keeps agents from running the user's decision commands, D1);
//! - zforge's `UserPromptSubmit` hook ([`PROMPT_HOOK`]) is added when no
//!   hook runs it yet: it records the decisions the user types in chat
//!   (`/accept`, `/revise`, `/handover`) — never the model;
//! - entries zforge itself used to write and no longer does
//!   ([`RETIRED_ALLOW`]) are removed — no other entry is ever dropped.
//!
//! Every allowed zforge tool prepares or observes; the user's decisions are
//! denied (D1).
//!
//! Without `--force` an existing file is left alone, as before.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;

pub(crate) const CLAUDE_SETTINGS_JSON: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(zforge *)",
      "mcp__zforge__status",
      "mcp__zforge__intake_new",
      "mcp__zforge__intake_task",
      "mcp__zforge__intake_status",
      "mcp__zforge__intake_diff",
      "mcp__zforge__intake_review",
      "mcp__zforge__change_new",
      "mcp__zforge__readiness",
      "mcp__zforge__knowledge_index",
      "mcp__zforge__onboard_probe",
      "mcp__zforge__onboard_status",
      "mcp__zforge__onboard_review",
      "mcp__zforge__run_start",
      "mcp__zforge__run_status",
      "mcp__zforge__run_log",
      "mcp__zforge__run_list",
      "mcp__zforge__run_cancel",
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
    ],
    "deny": [
      "Bash(zforge intake accept*)",
      "Bash(zforge intake revise*)",
      "Bash(zforge handover*)",
      "Bash(zforge onboard accept*)",
      "Bash(zforge onboard revise*)",
      "Bash(zforge onboard baseline*)",
      "Bash(zf intake accept*)",
      "Bash(zf intake revise*)",
      "Bash(zf handover*)",
      "Bash(zf onboard accept*)",
      "Bash(zf onboard revise*)",
      "Bash(zf onboard baseline*)",
      "Bash(zforge hook*)",
      "Bash(zf hook*)"
    ]
  },
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "zforge hook prompt"
          }
        ]
      }
    ]
  }
}
"#;

/// The command Claude Code runs on every message the user sends.
pub(crate) const PROMPT_HOOK: &str = "zforge hook prompt";

/// Allow entries earlier zforge versions wrote that name tools which no
/// longer exist: CodeGraph before 0.9, and the v1 task pipeline's tools.
const RETIRED_ALLOW: [&str; 9] = [
    "mcp__codegraph__query",
    "mcp__codegraph__context",
    "mcp__codegraph__files",
    "mcp__codegraph__affected",
    "mcp__zforge__task_import",
    "mcp__zforge__get_prompt",
    "mcp__zforge__approve",
    "mcp__zforge__verify",
    "mcp__zforge__ship",
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
        crate::fs::write_atomic(path, CLAUDE_SETTINGS_JSON.as_bytes())?;
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
    crate::fs::write_atomic(path, out.as_bytes())?;
    Ok(SettingsWrite::Merged)
}

fn merge(existing: &Value) -> Result<Value> {
    let template: Value = serde_json::from_str(CLAUDE_SETTINGS_JSON).expect("valid template");
    let mut merged = existing.clone();
    for key in ["allow", "deny"] {
        let wanted = template["permissions"][key]
            .as_array()
            .expect("template has permissions.allow and .deny");
        merge_list(&mut merged, key, wanted)?;
    }
    merge_prompt_hook(&mut merged, &template["hooks"]["UserPromptSubmit"][0])?;
    Ok(merged)
}

/// Append zforge's `UserPromptSubmit` entry unless a hook already runs
/// [`PROMPT_HOOK`]; the user's other hooks stay as they are.
fn merge_prompt_hook(merged: &mut Value, entry: &Value) -> Result<()> {
    let Some(root) = merged.as_object_mut() else {
        bail!("expected a JSON object at the top level");
    };
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(hooks) = hooks.as_object_mut() else {
        bail!("`hooks` is not an object");
    };
    let list = hooks
        .entry("UserPromptSubmit")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(list) = list.as_array_mut() else {
        bail!("`hooks.UserPromptSubmit` is not an array");
    };
    let present = list.iter().any(|e| {
        e["hooks"]
            .as_array()
            .is_some_and(|hs| hs.iter().any(|h| h["command"] == PROMPT_HOOK))
    });
    if !present {
        list.push(entry.clone());
    }
    Ok(())
}

/// Append zforge's missing `permissions.<key>` entries; for `allow`, drop
/// the retired ones first.
fn merge_list(merged: &mut Value, key: &str, wanted: &[Value]) -> Result<()> {
    let Some(root) = merged.as_object_mut() else {
        bail!("expected a JSON object at the top level");
    };
    let permissions = root
        .entry("permissions")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(permissions) = permissions.as_object_mut() else {
        bail!("`permissions` is not an object");
    };
    let list = permissions
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(list) = list.as_array_mut() else {
        bail!("`permissions.{key}` is not an array");
    };

    if key == "allow" {
        list.retain(|entry| !entry.as_str().is_some_and(|e| RETIRED_ALLOW.contains(&e)));
    }
    for entry in wanted {
        if !list.contains(entry) {
            list.push(entry.clone());
        }
    }
    Ok(())
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
                "allow": ["Bash(make *)", "mcp__codegraph__query", "mcp__zforge__status"],
                "deny": ["Bash(rm -rf *)"]
            },
            "hooks": {"Stop": [{"command": "make lint"}]}
        });
        std::fs::write(&path, serde_json::to_string_pretty(&user).unwrap()).unwrap();

        assert_eq!(write_settings(&path, true).unwrap(), SettingsWrite::Merged);

        let out = read(&path);
        assert_eq!(out["env"], user["env"]);
        assert_eq!(
            out["hooks"]["Stop"], user["hooks"]["Stop"],
            "user hooks kept"
        );
        assert_eq!(
            out["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            PROMPT_HOOK
        );
        let deny = out["permissions"]["deny"].as_array().unwrap();
        assert_eq!(
            deny[0], "Bash(rm -rf *)",
            "user deny rules keep their place"
        );
        assert!(
            deny.contains(&json!("Bash(zforge intake accept*)")),
            "agents may not record the user's decisions"
        );
        let allow = out["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow[0], "Bash(make *)", "user entries keep their place");
        assert_eq!(allow[1], "mcp__zforge__status");
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

    /// ONBOARD TASK-009 AC-03: a generated settings file allows the three
    /// onboard MCP tools and denies the three onboard decision commands —
    /// accepting, revising or recording known baseline failures needs a
    /// terminal (D1), like intake accept/revise and handover.
    #[test]
    fn onboard_tools_allowed_and_onboard_decisions_denied() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write_settings(&path, false).unwrap();
        let out = read(&path);
        let allow = out["permissions"]["allow"].as_array().unwrap();
        for tool in [
            "mcp__zforge__onboard_probe",
            "mcp__zforge__onboard_status",
            "mcp__zforge__onboard_review",
        ] {
            assert!(
                allow.contains(&json!(tool)),
                "{tool} missing from {allow:?}"
            );
        }
        let deny = out["permissions"]["deny"].as_array().unwrap();
        for cmd in [
            "Bash(zforge onboard accept*)",
            "Bash(zforge onboard revise*)",
            "Bash(zforge onboard baseline*)",
        ] {
            assert!(deny.contains(&json!(cmd)), "{cmd} missing from {deny:?}");
        }
    }

    /// Chat decisions: the hook is added once, beside the user's own
    /// `UserPromptSubmit` hooks, and the model may not run it itself.
    #[test]
    fn the_prompt_hook_is_added_once_and_denied_to_the_model() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let theirs = json!({"hooks": {"UserPromptSubmit": [
            {"hooks": [{"type": "command", "command": "my-hook"}]}
        ]}});
        std::fs::write(&path, theirs.to_string()).unwrap();

        write_settings(&path, true).unwrap();
        let out = read(&path);
        let entries = out["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], theirs["hooks"]["UserPromptSubmit"][0]);
        assert_eq!(entries[1]["hooks"][0]["command"], PROMPT_HOOK);
        let deny = out["permissions"]["deny"].as_array().unwrap();
        assert!(deny.contains(&json!("Bash(zforge hook*)")), "{deny:?}");

        assert_eq!(
            write_settings(&path, true).unwrap(),
            SettingsWrite::Unchanged,
            "not added twice"
        );
    }
}
