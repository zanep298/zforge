//! v1.5 intake (Mốc A): top-down files the user reviews and accepts before
//! any work is handed to an agent. See `docs/v1.5/workflow.md` and the
//! decisions in `docs/v1.5/decisions.md` (D1, D2, D4).
//!
//! An intake is a directory of Markdown files — outcome, behavior, solution,
//! breakdown, leaf tasks — that anyone may edit. What was reviewed and
//! accepted is not stored in those files (an agent could write it) but in
//! `.records/`, which only the runtime writes: a byte-exact snapshot of each
//! reviewed revision and an append-only decision log. A file's status is
//! derived from the log and the file's current hash ([`status`]).

pub mod graph;
pub mod handover;
pub mod hash;
pub mod lint;
pub mod readability;
pub mod readiness;

pub mod record;
pub mod review;
pub mod status;
pub mod templates;

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Stage files, in the order the user works through them.
pub const STAGES: [&str; 4] = [
    "01-outcome.md",
    "02-behavior.md",
    "03-solution.md",
    "04-breakdown.md",
];
pub const TASKS_DIR: &str = "tasks";
/// The intake's one-page reading view, written by the agent in the user's
/// language. Not a contract file: never reviewed, linted or pinned.
pub const BRIEF_FILE: &str = "brief.md";
pub const CHANGES_DIR: &str = "changes";
pub const RECORDS_DIR: &str = ".records";

/// One intake directory: `<project>/.zforge/intakes/<ID>/`.
#[derive(Debug, Clone)]
pub struct Intake {
    pub id: String,
    pub dir: PathBuf,
}

/// `<project>/.zforge/intakes`.
pub fn intakes_dir(project_root: &Path) -> PathBuf {
    project_root.join(".zforge").join("intakes")
}

impl Intake {
    pub fn open(project_root: &Path, id: &str) -> Result<Self> {
        validate_id(id)?;
        let dir = intakes_dir(project_root).join(id);
        if !dir.is_dir() {
            bail!("intake {id} not found; create it with `zforge intake new {id}`");
        }
        Ok(Self {
            id: id.to_string(),
            dir,
        })
    }

    pub fn records_dir(&self) -> PathBuf {
        self.dir.join(RECORDS_DIR)
    }

    /// Absolute path of an intake file given as `01-outcome.md` or
    /// `tasks/TASK-001.md`, after checking it names a reviewable file.
    pub fn file(&self, rel: &str) -> Result<PathBuf> {
        validate_file(rel)?;
        Ok(self.dir.join(rel))
    }

    /// Every reviewable file present: stages in order, then tasks, then
    /// change requests, each sorted by name.
    pub fn files(&self) -> Vec<String> {
        let mut files: Vec<String> = STAGES
            .iter()
            .filter(|s| self.dir.join(s).is_file())
            .map(|s| (*s).to_string())
            .collect();
        for sub in [TASKS_DIR, CHANGES_DIR] {
            let mut names: Vec<String> = std::fs::read_dir(self.dir.join(sub))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".md"))
                .map(|n| format!("{sub}/{n}"))
                .collect();
            names.sort();
            files.extend(names);
        }
        files
    }
}

impl crate::knowledge::docs::DocSet for Intake {
    fn id(&self) -> &str {
        &self.id
    }

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, rel: &str) -> Result<PathBuf> {
        Intake::file(self, rel)
    }

    fn files(&self) -> Vec<String> {
        Intake::files(self)
    }

    fn lint(&self, rel: &str, text: &str) -> Vec<lint::Issue> {
        let mut issues = lint::lint(rel, text, &self.id, &review::known(self));
        issues.extend(readability::check(rel, text));
        issues
    }

    fn reconfirm_needed(&self, rel: &str) -> Result<bool> {
        Ok(readiness::stale(self)?.contains(rel))
    }

    fn snapshot_key(&self, rel: &str) -> String {
        rel.trim_end_matches(".md").replace('/', "__")
    }

    fn review_hint(&self, rel: &str) -> String {
        format!("zforge intake review {} {rel}", self.id)
    }
}

/// Intake IDs become directory names: letters, digits, `.`, `_`, `-`,
/// starting with a letter or digit.
pub fn validate_id(id: &str) -> Result<()> {
    let ok = id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        bail!("invalid intake id {id:?}: use letters, digits, '.', '_' or '-'");
    }
    Ok(())
}

/// A reviewable file: a stage file, or `tasks/<name>.md` / `changes/<name>.md`.
pub fn validate_file(rel: &str) -> Result<()> {
    if STAGES.contains(&rel) {
        return Ok(());
    }
    let valid = rel.split_once('/').is_some_and(|(sub, name)| {
        [TASKS_DIR, CHANGES_DIR].contains(&sub)
            && name.ends_with(".md")
            && name.len() > 3
            && !name.contains('/')
            && validate_id(name.trim_end_matches(".md")).is_ok()
    });
    if !valid {
        bail!(
            "{rel:?} is not an intake file: use one of {}, tasks/<ID>.md or changes/<ID>.md",
            STAGES.join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_files_cannot_leave_the_intake() {
        assert!(validate_id("FEATURE-001").is_ok());
        for bad in ["", "../x", "a/b", ".hidden", "x y"] {
            assert!(validate_id(bad).is_err(), "{bad:?}");
        }
        assert!(validate_file("01-outcome.md").is_ok());
        assert!(validate_file("tasks/TASK-001.md").is_ok());
        for bad in [
            "05-other.md",
            "tasks/../01-outcome.md",
            "tasks/a/b.md",
            "tasks/.md",
            ".records/decisions.jsonl",
            "/etc/passwd",
        ] {
            assert!(validate_file(bad).is_err(), "{bad:?}");
        }
    }
}
