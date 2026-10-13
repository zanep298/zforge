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
//!   ([`RETIRED_ALLOW`]) are removed — no other entry is ever dropped;
//! - each existing `knowledge.extra` directory is appended to
//!   `permissions.additionalDirectories` (see [`write_settings`]).
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
      "mcp__zforge__knowledge_get",
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

/// `extra_dirs` are the reference-doc directories (`knowledge.extra`) that
/// exist, absolute: they go into `permissions.additionalDirectories` so
/// reading them asks nothing. They are merged like the allow entries —
/// the user's own entries kept, missing ones appended — and, being the
/// project's own configuration rather than zforge's defaults, also into
/// an existing file without `--force` (only that key is touched then).
pub(crate) fn write_settings(
    path: &Path,
    force: bool,
    extra_dirs: &[String],
) -> Result<SettingsWrite> {
    let wanted_dirs: Vec<Value> = extra_dirs.iter().map(|d| Value::from(d.as_str())).collect();
    if !path.exists() {
        if wanted_dirs.is_empty() {
            crate::fs::write_atomic(path, CLAUDE_SETTINGS_JSON.as_bytes())?;
        } else {
            let mut template: Value =
                serde_json::from_str(CLAUDE_SETTINGS_JSON).expect("valid template");
            merge_list(&mut template, "additionalDirectories", &wanted_dirs)?;
            write_json(path, &template)?;
        }
        return Ok(SettingsWrite::Created);
    }
    if !force && wanted_dirs.is_empty() {
        return Ok(SettingsWrite::Skipped);
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path:?}"))?;
    let existing: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        // Without --force an existing file is the user's; a broken one is
        // left for them to fix, as before.
        Err(_) if !force => return Ok(SettingsWrite::Skipped),
        Err(e) => {
            return Err(e).with_context(|| {
                format!("{path:?} is not valid JSON; fix or delete it before re-running init")
            })
        }
    };
    let mut merged = if force {
        merge(&existing).with_context(|| format!("{path:?}"))?
    } else {
        existing.clone()
    };
    if !wanted_dirs.is_empty() {
        merge_list(&mut merged, "additionalDirectories", &wanted_dirs)
            .with_context(|| format!("{path:?}"))?;
    }
    if merged == existing {
        return Ok(if force {
            SettingsWrite::Unchanged
        } else {
            SettingsWrite::Skipped
        });
    }
    write_json(path, &merged)?;
    Ok(SettingsWrite::Merged)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut out = serde_json::to_string_pretty(value)?;
    out.push('\n');
    crate::fs::write_atomic(path, out.as_bytes())
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
            write_settings(&path, false, &[]).unwrap(),
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
            write_settings(&path, false, &[]).unwrap(),
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

        assert_eq!(
            write_settings(&path, true, &[]).unwrap(),
            SettingsWrite::Merged
        );

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
        write_settings(&path, false, &[]).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            write_settings(&path, true, &[]).unwrap(),
            SettingsWrite::Unchanged
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn invalid_json_is_reported_not_clobbered() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = write_settings(&path, true, &[]).unwrap_err();
        assert!(format!("{err:#}").contains("not valid JSON"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn unexpected_shape_is_reported_not_clobbered() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{\"permissions\": {\"allow\": \"everything\"}}").unwrap();
        let err = write_settings(&path, true, &[]).unwrap_err();
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
        write_settings(&path, false, &[]).unwrap();
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

        write_settings(&path, true, &[]).unwrap();
        let out = read(&path);
        let entries = out["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], theirs["hooks"]["UserPromptSubmit"][0]);
        assert_eq!(entries[1]["hooks"][0]["command"], PROMPT_HOOK);
        let deny = out["permissions"]["deny"].as_array().unwrap();
        assert!(deny.contains(&json!("Bash(zforge hook*)")), "{deny:?}");

        assert_eq!(
            write_settings(&path, true, &[]).unwrap(),
            SettingsWrite::Unchanged,
            "not added twice"
        );
    }

    /// `knowledge.extra` directories: in a new file, and merged into an
    /// existing one — even without `--force` — keeping the user's entries
    /// first and adding each directory once.
    #[test]
    fn extra_dirs_go_into_additional_directories_once() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let dirs = vec!["/work/documents".to_string()];
        assert_eq!(
            write_settings(&path, false, &dirs).unwrap(),
            SettingsWrite::Created
        );
        let out = read(&path);
        assert_eq!(
            out["permissions"]["additionalDirectories"],
            json!(["/work/documents"])
        );
        assert_eq!(out["permissions"]["allow"], json!(template_allow()));

        let theirs = json!({"permissions": {
            "allow": ["Bash(make *)"],
            "additionalDirectories": ["/mine"]
        }});
        std::fs::write(&path, theirs.to_string()).unwrap();
        let dirs = vec!["/mine".to_string(), "/work/documents".to_string()];
        assert_eq!(
            write_settings(&path, false, &dirs).unwrap(),
            SettingsWrite::Merged
        );
        let out = read(&path);
        assert_eq!(
            out["permissions"]["additionalDirectories"],
            json!(["/mine", "/work/documents"])
        );
        assert_eq!(
            out["permissions"]["allow"],
            json!(["Bash(make *)"]),
            "without --force only additionalDirectories changes"
        );
        assert_eq!(
            write_settings(&path, false, &dirs).unwrap(),
            SettingsWrite::Skipped,
            "nothing to add the second time"
        );

        assert_eq!(
            write_settings(&path, true, &dirs).unwrap(),
            SettingsWrite::Merged
        );
        let out = read(&path);
        assert_eq!(
            out["permissions"]["additionalDirectories"],
            json!(["/mine", "/work/documents"])
        );
        assert!(out["permissions"]["allow"]
            .as_array()
            .unwrap()
            .contains(&json!("mcp__zforge__knowledge_get")));
    }

    #[test]
    fn extra_dirs_leave_a_broken_file_alone_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(
            write_settings(&path, false, &["/d".to_string()]).unwrap(),
            SettingsWrite::Skipped
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }
}
