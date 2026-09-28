//! Migrate the global store (`~/.zforge`, or `$ZFORGE_HOME`) from the task
//! pipeline to v1.5. Run by `zforge install` and whenever `init` uses the
//! store, so upgrading needs no extra step; a store with nothing of v1 left
//! is untouched.
//!
//! - v1 prompt templates, the spec/testspec/plan agents and the three v1
//!   skills are moved to `v1-archive/<ts>/`;
//! - agents and skills zforge still ships are brought to this version, the
//!   old copy archived first (they described the task pipeline);
//! - `registry.yaml` drops the fallback policy's retry settings (kept:
//!   `spawn_timeout_secs`), and `models.yaml` drops the removed phases —
//!   both archived first.

use super::archive::Archive;
use super::models_text;
use crate::embedded;
use anyhow::Result;
use std::path::Path;

/// Store files only the task pipeline used, relative to the store root.
pub const RETIRED: &[&str] = &[
    "agents/spec.tmpl",
    "agents/testspec.tmpl",
    "agents/plan.tmpl",
    "agents/code.tmpl",
    "agents/review.tmpl",
    "agents/verify-analysis.tmpl",
    "agents/spec-agent.md",
    "agents/testspec-agent.md",
    "agents/plan-agent.md",
    "skills/clarify-spec.md",
    "skills/derive-test-cases.md",
    "skills/implementation-planning.md",
];

/// Registry keys of the removed fallback loop.
const RETIRED_REGISTRY_KEYS: [&str; 4] = [
    "max_retries:",
    "cooldown_seconds:",
    "retryable_exit_codes:",
    "retryable_stderr_patterns:",
];

#[derive(Debug, Default)]
pub struct Report {
    pub archived: Vec<String>,
    pub refreshed: Vec<String>,
    pub registry: bool,
    pub models: bool,
    pub archive_dir: Option<std::path::PathBuf>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.archived.is_empty() && self.refreshed.is_empty() && !self.registry && !self.models
    }
}

/// Whether the store still holds anything of the task pipeline.
pub fn needs_migration(store: &Path) -> bool {
    RETIRED.iter().any(|r| Archive::present(store, r))
        || registry_is_v1(store)
        || models_text::has_removed_phases(&store.join("models.yaml"))
}

fn registry_is_v1(store: &Path) -> bool {
    std::fs::read_to_string(store.join("registry.yaml")).is_ok_and(|t| {
        t.lines().any(|l| {
            RETIRED_REGISTRY_KEYS
                .iter()
                .any(|k| l.trim().starts_with(k))
        })
    })
}

/// Migrate `store`. A no-op (empty report) when nothing of v1 is left.
pub fn migrate(store: &Path) -> Result<Report> {
    let mut report = Report::default();
    if !needs_migration(store) {
        return Ok(report);
    }
    let archive = Archive::new(store, store);
    for rel in RETIRED {
        if Archive::present(store, rel) {
            archive.take(rel)?;
            report.archived.push((*rel).to_string());
        }
    }
    for (rel, body) in shipped() {
        let path = store.join(&rel);
        let Ok(current) = std::fs::read_to_string(&path) else {
            continue; // `install` writes missing files
        };
        if current != body {
            archive.keep_copy(&rel)?;
            crate::fs::write_atomic(&path, body.as_bytes())?;
            report.refreshed.push(rel);
        }
    }
    if registry_is_v1(store) {
        archive.keep_copy("registry.yaml")?;
        crate::registry::lock::with_lock(|| {
            let r = crate::registry::io::load()?;
            crate::registry::io::save_atomic(&r)
        })?;
        report.registry = true;
    }
    let models = store.join("models.yaml");
    if models_text::has_removed_phases(&models) {
        archive.keep_copy("models.yaml")?;
        models_text::drop_removed_phases(&models)?;
        report.models = true;
    }
    report.archive_dir = Some(archive.dir().to_path_buf());
    Ok(report)
}

/// Every agent and skill the binary ships, by path under the store.
fn shipped() -> Vec<(String, &'static str)> {
    embedded::AGENTS
        .iter()
        .map(|(n, b)| (format!("agents/{n}"), *b))
        .chain(
            embedded::SKILLS
                .iter()
                .map(|(n, b)| (format!("skills/{n}"), *b)),
        )
        .chain(
            embedded::all_lang_skills()
                .into_iter()
                .map(|(n, b)| (format!("skills/{n}"), b)),
        )
        .collect()
}

/// One line per change, for the user.
pub fn describe(report: &Report) -> Vec<String> {
    let mut out = Vec::new();
    if !report.archived.is_empty() {
        out.push(format!(
            "archived {} task-pipeline file(s): {}",
            report.archived.len(),
            report.archived.join(", ")
        ));
    }
    if !report.refreshed.is_empty() {
        out.push(format!(
            "updated {} agent/skill file(s) to this version (old copies archived): {}",
            report.refreshed.len(),
            report.refreshed.join(", ")
        ));
    }
    if report.registry {
        out.push("registry.yaml: dropped the fallback retry settings".into());
    }
    if report.models {
        out.push("models.yaml: dropped the spec/testspec/plan phases".into());
    }
    if let Some(dir) = &report.archive_dir {
        out.push(format!("archive: {}", dir.display()));
    }
    out
}
