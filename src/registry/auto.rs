use crate::registry::{
    io, lock,
    schema::{AgentSpec, ProjectEntry, RegisteredBy, Registry},
    validate,
};
use anyhow::Result;
use chrono::Utc;
use std::path::Path;

/// Default CLI invocation for each well-known agent. Used by
/// `ensure_default_agents` to seed `registry.agents{}` on first init so the
/// orchestrator can resolve `--agent claude` / `--agent codex` /
/// `--agent opencode` without the user editing `~/.zforge/registry.yaml`
/// by hand.
///
/// Codex args: `exec` is the non-interactive subcommand. The orchestrator
/// auto-appends `--profile zforge_<phase>` at spawn time per phase.
///
/// Claude args: `-p --output-format stream-json --verbose` runs print mode
/// and emits one JSON event per line: a `system/init` event (version,
/// model, MCP server status, agent/skill catalogs), every tool call, and a
/// final `result` carrying the `usage` block that
/// `cost::usage::parse_claude_usage` reads for REAL token accounting.
/// `trace::claude` reads the rest into the per-task trace (IMP-006).
/// Plain `-p` prints text with no usage block, so every CostEntry would fall
/// back to a byte-length estimate that ignores claude's system prompt,
/// `/file` context, tool schemas and multi-turn tool output; the older
/// `--output-format json` carried usage but no tool calls. Keep these flags
/// or cost reports and traces become fiction.
fn default_agent_specs() -> &'static [(&'static str, &'static str, &'static [&'static str])] {
    &[
        (
            "claude",
            "claude",
            &["-p", "--output-format", "stream-json", "--verbose"],
        ),
        ("codex", "codex", &["exec"]),
        ("opencode", "opencode", &["run"]),
    ]
}

/// Earlier claude defaults, exactly as zforge wrote them: plain `-p` (no
/// usage block) and `-p --output-format json` (usage, no tool calls).
/// Installs carrying one are migrated to the current default so cost and
/// trace data become available. Any other claude args are treated as user
/// customization and left untouched.
const STALE_CLAUDE_ARGS: [&[&str]; 2] = [&["-p"], &["-p", "--output-format", "json"]];

fn default_specs_map() -> std::collections::BTreeMap<&'static str, AgentSpec> {
    default_agent_specs()
        .iter()
        .map(|(name, cmd, args)| {
            (
                *name,
                AgentSpec {
                    command: (*cmd).into(),
                    args: args.iter().map(|s| (*s).to_string()).collect(),
                },
            )
        })
        .collect()
}

/// Pure merge: seed missing defaults and migrate the stale claude spec.
/// Mutates `registry` in place; returns `(inserted_names, migrated)` so the
/// caller decides whether to persist. No IO / env — unit-testable directly.
fn apply_defaults(
    registry: &mut Registry,
    agent_names: &[&str],
    defaults: &std::collections::BTreeMap<&str, AgentSpec>,
) -> (Vec<String>, bool) {
    let mut inserted = Vec::new();
    let mut migrated = false;
    for name in agent_names {
        if let Some(existing) = registry.agents.get(*name) {
            if *name == "claude"
                && existing.command == "claude"
                && STALE_CLAUDE_ARGS
                    .iter()
                    .any(|stale| existing.args == *stale)
            {
                if let Some(spec) = defaults.get("claude") {
                    registry.agents.insert("claude".to_string(), spec.clone());
                    migrated = true;
                }
            }
            continue;
        }
        let Some(spec) = defaults.get(*name) else {
            continue;
        };
        registry.agents.insert((*name).to_string(), spec.clone());
        inserted.push((*name).to_string());
    }
    (inserted, migrated)
}

/// Insert default AgentSpec rows for the listed agent names when absent, and
/// migrate the stale claude default. Existing user customization survives.
/// Returns the names actually inserted.
pub fn ensure_default_agents(agent_names: &[&str]) -> Result<Vec<String>> {
    lock::with_lock(|| {
        let mut registry = io::load()?;
        let defaults = default_specs_map();
        let (inserted, migrated) = apply_defaults(&mut registry, agent_names, &defaults);
        if !inserted.is_empty() || migrated {
            io::save_atomic(&registry)?;
        }
        Ok(inserted)
    })
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    fn spec(command: &str, args: &[&str]) -> AgentSpec {
        AgentSpec {
            command: command.into(),
            args: args.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    const CURRENT: [&str; 4] = ["-p", "--output-format", "stream-json", "--verbose"];

    #[test]
    fn upgrades_every_earlier_claude_default_to_stream_json() {
        for stale in [&["-p"][..], &["-p", "--output-format", "json"]] {
            let mut reg = Registry::default();
            reg.agents
                .insert("claude".to_string(), spec("claude", stale));

            let (inserted, migrated) = apply_defaults(&mut reg, &["claude"], &default_specs_map());

            assert!(inserted.is_empty(), "existing key not re-inserted");
            assert!(migrated, "stale spec {stale:?} should migrate");
            assert_eq!(reg.agents["claude"].args, CURRENT);
        }
    }

    #[test]
    fn preserves_user_customized_claude_spec() {
        let mut reg = Registry::default();
        reg.agents.insert(
            "claude".to_string(),
            spec("claude", &["-p", "--model", "opus"]),
        );

        let (_, migrated) = apply_defaults(&mut reg, &["claude"], &default_specs_map());

        assert!(!migrated, "customized spec must not migrate");
        assert_eq!(reg.agents["claude"].args, vec!["-p", "--model", "opus"]);
    }

    #[test]
    fn seeds_missing_agents_with_stream_json_claude_default() {
        let mut reg = Registry::default();

        let (inserted, _) = apply_defaults(&mut reg, &["claude", "codex"], &default_specs_map());

        assert_eq!(inserted, vec!["claude", "codex"]);
        assert_eq!(reg.agents["claude"].args, CURRENT);
    }
}

#[derive(Debug)]
pub enum AutoResult {
    Registered {
        name: String,
    },
    AlreadyExists {
        name: String,
    },
    Suffixed {
        requested: String,
        final_name: String,
    },
    Updated {
        old_name: String,
        new_name: String,
    },
}

pub fn auto_register(cwd: &Path, name_override: Option<&str>, switch: bool) -> Result<AutoResult> {
    let canon = validate::canonicalize_path(cwd)?;
    validate::ensure_zforge_dir(&canon)?;

    let requested = match name_override {
        Some(n) => {
            validate::validate_name(n)?;
            n.to_string()
        }
        None => {
            let base = canon
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            validate::sanitize_basename(base)
        }
    };

    lock::with_lock(|| {
        let mut registry = io::load()?;
        let outcome = apply(&mut registry, &canon, &requested);
        if switch || registry.current_project.is_none() {
            let final_name = match &outcome {
                AutoResult::Registered { name }
                | AutoResult::AlreadyExists { name }
                | AutoResult::Updated { new_name: name, .. }
                | AutoResult::Suffixed {
                    final_name: name, ..
                } => name.clone(),
            };
            registry.current_project = Some(final_name);
        }
        io::save_atomic(&registry)?;
        Ok(outcome)
    })
}

fn apply(registry: &mut Registry, canon: &Path, requested: &str) -> AutoResult {
    if let Some(existing) = validate::find_by_path(registry, canon) {
        if existing.name == requested {
            return AutoResult::AlreadyExists {
                name: existing.name.clone(),
            };
        }
        // Rename in place: keep registered_at, registered_by, and any
        // per-project agent_overrides the user may have hand-edited. Only
        // the human-facing `name` changes.
        let old_name = existing.name.clone();
        let registered_at = existing.registered_at;
        let registered_by = existing.registered_by;
        let agent_overrides = existing.agent_overrides.clone();
        registry.projects.retain(|e| e.path != canon);
        registry.projects.push(ProjectEntry {
            name: requested.to_string(),
            path: canon.to_path_buf(),
            registered_at,
            registered_by,
            agent_overrides,
        });
        return AutoResult::Updated {
            old_name,
            new_name: requested.to_string(),
        };
    }

    let final_name = validate::next_available_name(registry, requested);
    let suffixed = final_name != requested;

    registry.projects.push(ProjectEntry {
        name: final_name.clone(),
        path: canon.to_path_buf(),
        registered_at: Utc::now(),
        registered_by: RegisteredBy::Init,
        agent_overrides: Default::default(),
    });

    if suffixed {
        AutoResult::Suffixed {
            requested: requested.to_string(),
            final_name,
        }
    } else {
        AutoResult::Registered { name: final_name }
    }
}
