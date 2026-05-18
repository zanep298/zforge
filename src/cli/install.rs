//! `zforge install` — populate the global zforge store at `~/.zforge/` with
//! prompt templates, agent definitions, and skill bundles baked into the
//! binary. One-time per-machine setup so multiple projects can share a
//! single source of truth via `zforge init --shared`.
//!
//! Re-run after upgrading the binary to refresh global files; pass `--force`
//! to overwrite user edits.

use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::cli::init::{label, write_safe};
use crate::embedded;

pub fn run(force: bool) -> Result<()> {
    let root = embedded::global_store_dir()
        .ok_or_else(|| anyhow!("cannot determine home directory for ~/.zforge/"))?;
    install_into(&root, force, /* quiet */ false)?;
    Ok(())
}

/// Install the embedded template/agent/skill payload under `root`. Returns
/// the number of files written. `quiet` suppresses progress lines so other
/// commands (e.g. `init --shared`) can auto-trigger install without
/// flooding output.
pub fn install_into(root: &Path, force: bool, quiet: bool) -> Result<usize> {
    let agents_dir = root.join("agents");
    let skills_dir = root.join("skills");
    std::fs::create_dir_all(&agents_dir)?;
    std::fs::create_dir_all(&skills_dir)?;

    let mut created = 0usize;
    let mut skipped = 0usize;

    // Prompt templates (.tmpl) — read by Engine
    for (name, body) in embedded::PROMPT_TEMPLATES {
        if write_safe(&agents_dir.join(name), body, force)? {
            created += 1;
        } else {
            skipped += 1;
        }
    }

    // Agent definitions (.md) — discovered by Claude Code / OpenCode / Codex
    // when symlinked into the harness's project-local agents dir.
    for (name, body) in embedded::AGENTS {
        if write_safe(&agents_dir.join(name), body, force)? {
            created += 1;
        } else {
            skipped += 1;
        }
    }

    if !quiet {
        println!(
            "{} {} — {} prompt templates + {} agent definitions",
            label(created > 0),
            display_path(&agents_dir),
            embedded::PROMPT_TEMPLATES.len(),
            embedded::AGENTS.len(),
        );
    }

    // Generic skills (flat + backend/ + frontend/)
    let before_skills = created;
    for (name, body) in embedded::SKILLS {
        if write_safe(&skills_dir.join(name), body, force)? {
            created += 1;
        } else {
            skipped += 1;
        }
    }

    // All per-language skills — global store gets every language so any
    // project can opt in via its AGENTS.md without re-running install.
    let lang_skills = embedded::all_lang_skills();
    let lang_count = lang_skills.len();
    for (name, body) in &lang_skills {
        if write_safe(&skills_dir.join(name), body, force)? {
            created += 1;
        } else {
            skipped += 1;
        }
    }

    if !quiet {
        println!(
            "{} {} — {} base + {} language skills",
            label(created > before_skills),
            display_path(&skills_dir),
            embedded::SKILLS.len(),
            lang_count,
        );
        println!();
        println!(
            "  {} created, {} skipped (already existed). Use --force to overwrite.",
            created, skipped
        );
        println!();
        println!("Next:");
        println!("  zforge init --shared      # scaffold a project that uses this store");
    }

    Ok(created)
}

/// Render `~/.zforge/...` rather than the absolute home expansion when
/// possible so progress output stays readable.
fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}

/// Ensure `~/.zforge/agents/` is populated. If it's already there, do
/// nothing. Otherwise run a quiet install. Used by `init --shared` so users
/// don't need to remember a separate `zforge install` step on first run.
pub fn ensure_global_store() -> Result<PathBuf> {
    let root = embedded::global_store_dir()
        .ok_or_else(|| anyhow!("cannot determine home directory for ~/.zforge/"))?;
    let marker = root.join("agents").join("spec-agent.md");
    if !marker.exists() {
        eprintln!(
            "{} ~/.zforge/ not initialized — running first-time install…",
            "ℹ".cyan()
        );
        install_into(&root, /* force */ false, /* quiet */ true)?;
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn install_into_writes_all_embedded_files() {
        let tmp = TempDir::new().unwrap();
        let written = install_into(tmp.path(), /* force */ false, /* quiet */ true).unwrap();

        let expected = embedded::PROMPT_TEMPLATES.len()
            + embedded::AGENTS.len()
            + embedded::SKILLS.len()
            + embedded::all_lang_skills().len();
        assert_eq!(written, expected);

        // Spot-check key files
        assert!(tmp.path().join("agents/spec.tmpl").exists());
        assert!(tmp.path().join("agents/spec-agent.md").exists());
        assert!(tmp.path().join("skills/clarify-spec.md").exists());
        assert!(tmp.path().join("skills/rust-patterns.md").exists());
        assert!(tmp.path().join("skills/flutter-patterns.md").exists());
        assert!(tmp.path().join("skills/backend/api-contracts.md").exists());
    }

    #[test]
    fn install_into_skips_existing_without_force() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("agents")).unwrap();
        let preexisting = tmp.path().join("agents/spec.tmpl");
        std::fs::write(&preexisting, "USER EDIT").unwrap();

        install_into(tmp.path(), /* force */ false, /* quiet */ true).unwrap();

        let contents = std::fs::read_to_string(&preexisting).unwrap();
        assert_eq!(contents, "USER EDIT", "must not overwrite user edits");
    }

    #[test]
    fn install_into_overwrites_with_force() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("agents")).unwrap();
        let target = tmp.path().join("agents/spec.tmpl");
        std::fs::write(&target, "USER EDIT").unwrap();

        install_into(tmp.path(), /* force */ true, /* quiet */ true).unwrap();

        let contents = std::fs::read_to_string(&target).unwrap();
        assert_ne!(contents, "USER EDIT", "force must overwrite");
        assert!(!contents.trim().is_empty());
    }
}
