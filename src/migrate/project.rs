//! Migrate one project from the task pipeline to v1.5 (`zforge migrate`).
//!
//! [`plan`] looks; [`apply_files`] changes what is plain file work:
//!
//! - the task pipeline's records (`.zforge/tasks`, `memory`, `jobs`,
//!   `cost-log.jsonl`) and files only it used (spec/testspec/plan agents,
//!   prompt templates, the `/zforge` command, v1 skills in local mode) move
//!   to `.zforge/v1-archive/<ts>/`;
//! - `config.yaml` loses the v1 keys and gains the `execution` block when
//!   it has none; `models.yaml` loses the removed phases — comments kept,
//!   both copied to the archive first;
//! - the instruction files `init` is about to regenerate are copied there
//!   too.
//!
//! Regenerating agents, skills, CLAUDE.md/AGENTS.md and settings is `init
//! --force` for the clients the project has, done by `cli::migrate`.

use super::archive::Archive;
use super::{models_text, yaml_text};
use anyhow::{Context, Result};
use serde_yaml::Value;
use std::path::{Path, PathBuf};

/// Paths of the task pipeline, relative to the project root.
pub const RETIRED: &[&str] = &[
    ".zforge/tasks",
    ".zforge/memory",
    ".zforge/jobs",
    ".zforge/cost-log.jsonl",
    ".claude/agents/spec-agent.md",
    ".claude/agents/testspec-agent.md",
    ".claude/agents/plan-agent.md",
    ".claude/commands/zforge.md",
    ".codex/agents/spec-agent.md",
    ".codex/agents/testspec-agent.md",
    ".codex/agents/plan-agent.md",
    ".opencode/agents/spec-agent.md",
    ".opencode/agents/testspec-agent.md",
    ".opencode/agents/plan-agent.md",
    ".zforge/agents/spec-agent.md",
    ".zforge/agents/testspec-agent.md",
    ".zforge/agents/plan-agent.md",
    ".zforge/agents/spec.tmpl",
    ".zforge/agents/testspec.tmpl",
    ".zforge/agents/plan.tmpl",
    ".zforge/agents/code.tmpl",
    ".zforge/agents/review.tmpl",
    ".zforge/agents/verify-analysis.tmpl",
    ".zforge/skills/clarify-spec.md",
    ".zforge/skills/derive-test-cases.md",
    ".zforge/skills/implementation-planning.md",
];

/// Config keys of the task pipeline: `(section, key)`, or `(key, "")` for
/// a top-level key.
const RETIRED_CONFIG: [(&str, &str); 5] = [
    ("opencode", ""),
    ("review", ""),
    ("project", "root_dir"),
    ("paths", "tasks"),
    ("paths", "memory"),
];

const CONFIG: &str = ".zforge/config.yaml";
const MODELS: &str = ".zforge/models.yaml";

/// Clients whose files a project has, in `init`'s order.
const CLIENT_DIRS: [(&str, &str); 3] = [
    ("claude", ".claude"),
    ("codex", ".codex"),
    ("opencode", ".opencode"),
];

#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Paths to move to the archive.
    pub archive: Vec<String>,
    /// v1 keys in `config.yaml`, as `section.key`.
    pub config_keys: Vec<String>,
    /// `config.yaml` has no `execution` block to add one to.
    pub add_execution: bool,
    /// `models.yaml` names removed phases.
    pub models: bool,
    /// Clients `init --force` regenerates.
    pub clients: Vec<&'static str>,
    /// Files copied to the archive before they are rewritten.
    pub backups: Vec<String>,
}

impl Plan {
    /// Nothing of the task pipeline is left.
    pub fn is_empty(&self) -> bool {
        self.archive.is_empty() && self.config_keys.is_empty() && !self.models
    }
}

/// Whether the project still holds anything of the task pipeline.
pub fn needs_migration(root: &Path) -> bool {
    plan(root).is_ok_and(|p| !p.is_empty())
}

pub fn plan(root: &Path) -> Result<Plan> {
    let archive: Vec<String> = RETIRED
        .iter()
        .filter(|r| root.join(r).exists())
        .map(|r| (*r).to_string())
        .collect();
    let config = read_config(root)?;
    let config_keys: Vec<String> = config
        .as_ref()
        .map(|v| {
            RETIRED_CONFIG
                .iter()
                .filter(|(s, k)| has_key(v, s, k))
                .map(|(s, k)| {
                    if k.is_empty() {
                        s.to_string()
                    } else {
                        format!("{s}.{k}")
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let models = models_text::has_removed_phases(&root.join(MODELS));
    let mut plan = Plan {
        archive,
        config_keys,
        models,
        ..Plan::default()
    };
    if plan.is_empty() {
        return Ok(plan);
    }
    plan.add_execution = config
        .as_ref()
        .is_some_and(|v| v.get("execution").is_none());
    plan.clients = CLIENT_DIRS
        .iter()
        .filter(|(_, dir)| root.join(dir).is_dir())
        .map(|(c, _)| *c)
        .collect();
    let mut backups: Vec<&str> = Vec::new();
    if !plan.config_keys.is_empty() || plan.add_execution {
        backups.push(CONFIG);
    }
    if plan.models {
        backups.push(MODELS);
    }
    if !plan.clients.is_empty() {
        backups.push(".zforge/README.md");
    }
    if plan.clients.contains(&"claude") {
        backups.extend(["CLAUDE.md", ".claude/settings.json"]);
    }
    if plan.clients.iter().any(|c| *c != "claude") {
        backups.push("AGENTS.md");
    }
    let mut backups: Vec<String> = backups.into_iter().map(String::from).collect();
    if !plan.clients.is_empty() && config.as_ref().is_some_and(is_local) {
        backups.extend(local_store_files());
    }
    plan.backups = backups
        .into_iter()
        .filter(|b| root.join(b).is_file())
        .filter(|b| !is_instruction(b) || zforge_wrote(&root.join(b)))
        .collect();
    Ok(plan)
}

fn is_instruction(rel: &str) -> bool {
    rel == "CLAUDE.md" || rel == "AGENTS.md"
}

/// An instruction file zforge generated; the project's own is never
/// rewritten (`cli::init::instructions`).
fn zforge_wrote(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| t.contains(crate::cli::init::instructions::MARKER))
}

/// Local mode keeps agents and skills in the project (`paths.agents` is
/// relative), where `init --force` rewrites them.
fn is_local(config: &Value) -> bool {
    config["paths"]["agents"]
        .as_str()
        .is_some_and(|p| !p.starts_with('/') && !p.starts_with('~'))
}

/// The project copies of the agents and skills zforge ships.
fn local_store_files() -> Vec<String> {
    let agents = crate::embedded::AGENTS
        .iter()
        .map(|(n, _)| format!(".zforge/agents/{n}"));
    let skills = crate::embedded::SKILLS
        .iter()
        .map(|(n, _)| (*n).to_string())
        .chain(
            crate::embedded::all_lang_skills()
                .into_iter()
                .map(|(n, _)| n),
        )
        .map(|n| format!(".zforge/skills/{n}"));
    agents.chain(skills).collect()
}

fn read_config(root: &Path) -> Result<Option<Value>> {
    let path = root.join(CONFIG);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    serde_yaml::from_str(&text)
        .map(Some)
        .with_context(|| format!("{} is not valid YAML", path.display()))
}

fn has_key(v: &Value, section: &str, key: &str) -> bool {
    match (v.get(section), key) {
        (Some(_), "") => true,
        (Some(sec), k) => sec.get(k).is_some(),
        (None, _) => false,
    }
}

/// Archive, back up and edit as planned. Returns the archive directory.
pub fn apply_files(root: &Path, plan: &Plan) -> Result<PathBuf> {
    let archive = Archive::new(root, &root.join(".zforge"));
    for rel in &plan.backups {
        archive.keep_copy(rel)?;
    }
    for rel in &plan.archive {
        archive.take(rel)?;
    }
    if !plan.config_keys.is_empty() || plan.add_execution {
        let path = root.join(CONFIG);
        let text = std::fs::read_to_string(&path)?;
        crate::fs::write_atomic(&path, migrate_config(&text, plan.add_execution)?.as_bytes())?;
    }
    if plan.models {
        models_text::drop_removed_phases(&root.join(MODELS))?;
    }
    Ok(archive.dir().to_path_buf())
}

/// `config.yaml` without the task pipeline's keys, with the `execution`
/// block appended when asked. Edited as text; a layout the text edit cannot
/// handle (flow style) is re-serialized instead, losing its comments.
pub fn migrate_config(text: &str, add_execution: bool) -> Result<String> {
    let mut out = text.to_string();
    for (section, key) in RETIRED_CONFIG {
        let edited = if key.is_empty() {
            yaml_text::remove_top_key(&out, section)
        } else {
            yaml_text::remove_nested_key(&out, section, key)
        };
        if let Some(e) = edited {
            out = e;
        }
    }
    let parsed: Value = serde_yaml::from_str(&out).context("the migrated config does not parse")?;
    if RETIRED_CONFIG.iter().any(|(s, k)| has_key(&parsed, s, k)) {
        out = reserialize_without_v1(&parsed)?;
    }
    if add_execution {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(crate::cli::init::config_file::EXECUTION_BLOCK);
    }
    let check: crate::config::Config =
        serde_yaml::from_str(&out).context("the migrated config does not load")?;
    drop(check);
    Ok(out)
}

fn reserialize_without_v1(v: &Value) -> Result<String> {
    let mut v = v.clone();
    for (section, key) in RETIRED_CONFIG {
        let Value::Mapping(top) = &mut v else { break };
        if key.is_empty() {
            top.remove(section);
        } else if let Some(Value::Mapping(sec)) = top.get_mut(section) {
            sec.remove(key);
        }
    }
    Ok(serde_yaml::to_string(&v)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1_CONFIG: &str = "# my project\nproject:\n  name: app # shown\n  language: rust\n  \
        test_command: \"cargo test\"\n  root_dir: \".\"\nopencode:\n  model: \"claude-sonnet-4-6\"\n  \
        context_files: []\nrunner:\n  default: \"claude\"\npaths:\n  tasks: \"./.zforge/tasks\"\n  \
        agents: \"./.zforge/agents\"\n  memory: \"./.zforge/memory\"\n  skills: \"./.zforge/skills\"\n\
        review:\n  auto_approve: false\n";

    #[test]
    fn a_v1_config_keeps_its_comments_and_gains_the_execution_block() {
        let out = migrate_config(V1_CONFIG, true).unwrap();
        assert!(
            out.starts_with("# my project\nproject:\n  name: app # shown\n"),
            "{out}"
        );
        for gone in [
            "root_dir",
            "opencode",
            "tasks:",
            "memory:",
            "\nreview:",
            "auto_approve",
        ] {
            assert!(!out.contains(gone), "{gone} left in {out}");
        }
        assert!(
            out.contains("paths:\n  agents: \"./.zforge/agents\"\n  skills:"),
            "{out}"
        );
        assert!(out.contains("execution:\n  # Code → verify"), "{out}");
        assert!(out.contains("# budget_usd: 5.0"));
    }

    #[test]
    fn flow_style_is_reserialized() {
        let out = migrate_config(
            "project: {name: app}\npaths: {tasks: ./t, agents: ./a}\n",
            false,
        )
        .unwrap();
        let v: Value = serde_yaml::from_str(&out).unwrap();
        assert!(v["paths"].get("tasks").is_none());
        assert_eq!(v["paths"]["agents"], "./a");
    }

    #[test]
    fn plan_finds_v1_files_config_and_models() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".zforge/tasks/T1")).unwrap();
        std::fs::create_dir_all(root.join(".claude/agents")).unwrap();
        std::fs::write(root.join(".claude/agents/spec-agent.md"), "x").unwrap();
        std::fs::write(root.join(".claude/settings.json"), "{}").unwrap();
        std::fs::write(root.join("CLAUDE.md"), "# my service").unwrap();
        std::fs::write(root.join(CONFIG), V1_CONFIG).unwrap();
        std::fs::write(root.join(MODELS), "claude:\n  plan: opus\n").unwrap();

        let p = plan(root).unwrap();
        assert_eq!(p.archive, [".zforge/tasks", ".claude/agents/spec-agent.md"]);
        assert_eq!(
            p.config_keys,
            [
                "opencode",
                "review",
                "project.root_dir",
                "paths.tasks",
                "paths.memory"
            ]
        );
        assert!(p.add_execution && p.models);
        assert_eq!(p.clients, ["claude"]);
        assert_eq!(
            p.backups,
            [CONFIG, MODELS, ".claude/settings.json"],
            "the project's own CLAUDE.md is not rewritten, so not backed up"
        );

        let dir = apply_files(root, &p).unwrap();
        assert!(dir.join(".zforge/tasks/T1").is_dir());
        assert!(!root.join(".zforge/tasks").exists());
        assert!(dir.join(".claude/settings.json").is_file());
        assert!(root.join("CLAUDE.md").is_file());
        assert!(!models_text::has_removed_phases(&root.join(MODELS)));
        assert!(
            plan(root).unwrap().is_empty(),
            "migrated once, nothing left"
        );
    }

    /// Local mode: the project's own agents and skills are rewritten by
    /// the refresh, so they are copied to the archive first.
    #[test]
    fn local_mode_backs_up_the_project_copies_of_agents_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::create_dir_all(root.join(".zforge/skills")).unwrap();
        std::fs::create_dir_all(root.join(".zforge/agents")).unwrap();
        std::fs::write(root.join(".zforge/agents/code-agent.md"), "mine").unwrap();
        std::fs::write(root.join(".zforge/skills/debug.md"), "mine").unwrap();
        std::fs::write(root.join(".zforge/agents/spec.tmpl"), "v1").unwrap();
        std::fs::write(root.join(CONFIG), V1_CONFIG).unwrap();

        let p = plan(root).unwrap();
        assert!(p
            .backups
            .contains(&".zforge/agents/code-agent.md".to_string()));
        assert!(p.backups.contains(&".zforge/skills/debug.md".to_string()));

        std::fs::write(
            root.join(CONFIG),
            V1_CONFIG.replace("./.zforge/agents", "/store/agents"),
        )
        .unwrap();
        let shared = plan(root).unwrap();
        assert!(!shared.backups.iter().any(|b| b.contains("code-agent")));
    }

    #[test]
    fn a_v15_project_needs_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zforge/intakes")).unwrap();
        assert!(!needs_migration(tmp.path()));
    }
}
