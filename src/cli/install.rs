//! `zforge install` — populate the global zforge store at `~/.zforge/` with
//! prompt templates, agent definitions, and skill bundles baked into the
//! binary. One-time per-machine setup so multiple projects can share a
//! single source of truth via `zforge init` (shared mode, the default).
//!
//! Every run (and every `zforge init`, via [`ensure_global_store`]) brings
//! the store to this binary's version, keeping files the user edited
//! (`crate::shipped`); `--force` replaces those too, archiving a copy.

use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::cli::init::label;
use crate::{embedded, shipped};

pub fn run(force: bool) -> Result<()> {
    let root = embedded::global_store_dir()
        .ok_or_else(|| anyhow!("cannot determine home directory for ~/.zforge/"))?;
    migrate_store(&root, false)?;
    install_into(&root, force, /* quiet */ false)?;
    Ok(())
}

/// Bring a store left by the task pipeline to this version
/// (`crate::migrate::global`); silent when there is nothing to do.
fn migrate_store(root: &Path, to_stderr: bool) -> Result<()> {
    let report = crate::migrate::global::migrate(root)?;
    if report.is_empty() {
        return Ok(());
    }
    let mut lines = vec![format!(
        "{} migrated {} from the task pipeline to v1.5:",
        "↻".cyan(),
        display_path(root)
    )];
    lines.extend(
        crate::migrate::global::describe(&report)
            .into_iter()
            .map(|l| format!("  {l}")),
    );
    for l in lines {
        if to_stderr {
            eprintln!("{l}");
        } else {
            println!("{l}");
        }
    }
    Ok(())
}

/// Bring the store at `root` to this version's agents and skills
/// (`crate::shipped`): missing files are written, files zforge wrote and
/// the user left alone are updated, the user's edits are kept and reported
/// — unless `force`, which replaces them after archiving a copy. A store
/// from before the manifest has no record of what zforge wrote: a file
/// there that differs is archived, then updated. `quiet` (used by `init`)
/// prints one line to stderr when something changed or was kept, nothing
/// when all is current.
pub fn install_into(root: &Path, force: bool, quiet: bool) -> Result<shipped::Report> {
    let files: Vec<(String, String)> = embedded::store_files()
        .into_iter()
        .map(|(rel, body)| (rel, body.to_string()))
        .collect();
    let mode = if force {
        shipped::Mode::Force
    } else {
        shipped::Mode::Refresh
    };
    let report = shipped::sync(root, &root.join(shipped::MANIFEST), &files, mode, &|_| {
        shipped::Unrecorded::Replace
    })?;

    if quiet {
        if !report.is_quiet() {
            eprintln!("{} {}", "↻".cyan(), summary_line(root, &report));
        }
        return Ok(report);
    }

    println!(
        "{} {} — {} agent definitions, {} base + {} language skills",
        label(report.written() > 0),
        display_path(root),
        embedded::AGENTS.len(),
        embedded::SKILLS.len(),
        embedded::all_lang_skills().len(),
    );
    println!("  {}", report.counts());
    for rel in &report.kept {
        println!("  {}", kept_line(root, rel));
    }
    if let Some(dir) = &report.archive_dir {
        println!("  old copies archived in {}", display_path(dir));
    }
    println!();
    println!("Next:");
    println!("  zforge init               # scaffold a project that uses this store");
    Ok(report)
}

/// One line for `init`: what changed in the store, and the edits kept.
fn summary_line(root: &Path, report: &shipped::Report) -> String {
    let mut line = format!(
        "{} brought to zforge {}: {}",
        display_path(root),
        env!("CARGO_PKG_VERSION"),
        report.counts()
    );
    if let Some(dir) = &report.archive_dir {
        line.push_str(&format!("; old copies in {}", display_path(dir)));
    }
    if !report.kept.is_empty() {
        line.push_str(&format!(
            "; kept your edit: {} (zforge ships a newer version; `zforge install --force` \
             replaces it, keeping a copy in {}/)",
            report.kept.join(", "),
            display_path(&root.join(shipped::ARCHIVE_DIR).join("<ts>"))
        ));
    }
    line
}

fn kept_line(root: &Path, rel: &str) -> String {
    format!(
        "{} kept your edit: {rel} (zforge ships a newer version; `zforge install --force` \
         replaces it, keeping a copy in {}/)",
        "!".yellow(),
        display_path(&root.join(shipped::ARCHIVE_DIR).join("<ts>"))
    )
}

/// The store against this binary, read-only (`zforge doctor`).
pub fn store_status(root: &Path) -> shipped::Status {
    let files: Vec<(String, String)> = embedded::store_files()
        .into_iter()
        .map(|(rel, body)| (rel, body.to_string()))
        .collect();
    shipped::status(root, &root.join(shipped::MANIFEST), &files)
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

/// Ensure the global store is usable by this version: a quiet install when
/// it is empty; otherwise migrate what the task pipeline left
/// (`crate::migrate::global`) and bring every shipped file to this version,
/// keeping the user's edits ([`install_into`]). Used by `init` (shared
/// mode) so no separate `zforge install` step is needed on first run or
/// after an upgrade.
pub fn ensure_global_store() -> Result<PathBuf> {
    let root = embedded::global_store_dir()
        .ok_or_else(|| anyhow!("cannot determine home directory for ~/.zforge/"))?;
    let marker = root.join("agents").join("code-agent.md");
    if !marker.exists() {
        eprintln!(
            "{} ~/.zforge/ not initialized — running first-time install…",
            "ℹ".cyan()
        );
        install_into(&root, /* force */ false, /* quiet */ true)?;
    } else {
        migrate_store(&root, true)?;
        install_into(&root, /* force */ false, /* quiet */ true)?;
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn read(root: &Path, rel: &str) -> String {
        std::fs::read_to_string(root.join(rel)).unwrap()
    }

    fn manifest(root: &Path) -> shipped::Manifest {
        shipped::Manifest::load(&root.join(shipped::MANIFEST)).expect("manifest written")
    }

    /// The store as an older zforge left it: `rel` holds `old`, and (when
    /// `recorded`) the manifest says zforge wrote exactly that.
    fn older_store(root: &Path, rel: &str, old: &str, recorded: bool) {
        install_into(root, false, true).unwrap();
        std::fs::write(root.join(rel), old).unwrap();
        let path = root.join(shipped::MANIFEST);
        if recorded {
            let mut m = manifest(root);
            m.version = "0.0.1".into();
            m.files.insert(rel.into(), shipped::digest(old.as_bytes()));
            std::fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
    }

    #[test]
    fn install_into_writes_all_embedded_files_and_the_manifest() {
        let tmp = TempDir::new().unwrap();
        let report = install_into(tmp.path(), /* force */ false, /* quiet */ true).unwrap();

        let expected = embedded::store_files().len();
        assert_eq!(report.created.len(), expected);

        // Spot-check key files
        assert!(tmp.path().join("agents/code-agent.md").exists());
        assert!(tmp.path().join("skills/review-patch.md").exists());
        assert!(tmp.path().join("skills/rust-patterns.md").exists());
        assert!(tmp.path().join("skills/flutter-patterns.md").exists());
        assert!(tmp.path().join("skills/backend/api-contracts.md").exists());

        let m = manifest(tmp.path());
        assert_eq!(m.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(m.files.len(), expected);
        assert_eq!(
            m.files["skills/intake.md"],
            shipped::digest(read(tmp.path(), "skills/intake.md").as_bytes())
        );
        assert!(store_status(tmp.path()).is_current());

        // A second run has nothing to do.
        let again = install_into(tmp.path(), false, true).unwrap();
        assert!(again.is_quiet());
        assert_eq!(again.current, expected);
    }

    #[test]
    fn upgrade_refreshes_files_zforge_wrote_and_the_user_left_alone() {
        let tmp = TempDir::new().unwrap();
        older_store(tmp.path(), "skills/intake.md", "OLD INTAKE", true);
        let status = store_status(tmp.path());
        assert!(!status.is_current());
        assert_eq!(status.outdated, vec!["skills/intake.md"]);

        let report = install_into(tmp.path(), false, true).unwrap();
        assert_eq!(report.updated, vec!["skills/intake.md"]);
        assert!(
            report.archive_dir.is_none(),
            "zforge's own copy needs no archive"
        );
        assert_eq!(
            read(tmp.path(), "skills/intake.md"),
            embedded::store_files()
                .into_iter()
                .find(|(r, _)| r == "skills/intake.md")
                .unwrap()
                .1
        );
        assert_eq!(manifest(tmp.path()).version, env!("CARGO_PKG_VERSION"));
        assert!(store_status(tmp.path()).is_current());
    }

    #[test]
    fn upgrade_keeps_and_reports_a_file_the_user_edited() {
        let tmp = TempDir::new().unwrap();
        older_store(tmp.path(), "agents/code-agent.md", "OLD AGENT", true);
        std::fs::write(tmp.path().join("agents/code-agent.md"), "USER EDIT").unwrap();

        let report = install_into(tmp.path(), false, true).unwrap();
        assert_eq!(report.kept, vec!["agents/code-agent.md"]);
        assert_eq!(read(tmp.path(), "agents/code-agent.md"), "USER EDIT");
        assert!(summary_line(tmp.path(), &report).contains("kept your edit: agents/code-agent.md"));

        let status = store_status(tmp.path());
        assert!(status.is_current(), "an edit kept is not an outdated store");
        assert_eq!(status.edited, vec!["agents/code-agent.md"]);
    }

    #[test]
    fn a_store_from_before_the_manifest_is_archived_then_refreshed() {
        let tmp = TempDir::new().unwrap();
        older_store(tmp.path(), "skills/intake.md", "LEGACY", false);
        assert!(store_status(tmp.path()).version.is_none());

        let report = install_into(tmp.path(), false, true).unwrap();
        assert_eq!(report.archived, vec!["skills/intake.md"]);
        let dir = report.archive_dir.expect("old copy archived");
        assert!(dir.starts_with(tmp.path().join(shipped::ARCHIVE_DIR)));
        assert_eq!(
            std::fs::read_to_string(dir.join("skills/intake.md")).unwrap(),
            "LEGACY"
        );
        assert_ne!(read(tmp.path(), "skills/intake.md"), "LEGACY");
        assert!(store_status(tmp.path()).is_current());
    }

    #[test]
    fn force_replaces_edits_after_archiving_them() {
        let tmp = TempDir::new().unwrap();
        install_into(tmp.path(), false, true).unwrap();
        let target = tmp.path().join("agents/code-agent.md");
        std::fs::write(&target, "USER EDIT").unwrap();

        let report = install_into(tmp.path(), /* force */ true, /* quiet */ true).unwrap();

        assert_eq!(report.archived, vec!["agents/code-agent.md"]);
        let contents = std::fs::read_to_string(&target).unwrap();
        assert_ne!(contents, "USER EDIT", "force must overwrite");
        assert!(!contents.trim().is_empty());
        assert_eq!(
            std::fs::read_to_string(report.archive_dir.unwrap().join("agents/code-agent.md"))
                .unwrap(),
            "USER EDIT"
        );
    }
}
