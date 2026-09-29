//! The instruction files at the project root: `CLAUDE.md`, `AGENTS.md`.
//!
//! They are often the project's own: a team writes CLAUDE.md for the
//! codebase long before zforge arrives. `--force` refreshes a file zforge
//! generated — recognised by [`MARKER`], which every zforge template
//! carries — and never replaces one the user wrote.

use anyhow::Result;
use std::path::Path;

/// A line every generated instruction file has, in every version.
pub(crate) const MARKER: &str = "**Workflow manager:** zforge";

/// Whether zforge generated `text`: [`MARKER`] as a line of its own. A file
/// that merely mentions it — documentation about zforge — is not one
/// (zforge's own CLAUDE.md was overwritten that way).
pub(crate) fn generated(text: &str) -> bool {
    text.lines().any(|l| l.trim() == MARKER)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Written {
    Created,
    Refreshed,
    /// Exists; left as is because `--force` was not given.
    Kept,
    /// Exists and zforge did not write it: never replaced.
    Theirs,
}

pub(crate) fn write(path: &Path, content: &str, force: bool) -> Result<Written> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::fs::write_atomic(path, content.as_bytes())?;
            return Ok(Written::Created);
        }
        Err(e) => return Err(e.into()),
    };
    if !generated(&existing) {
        return Ok(Written::Theirs);
    }
    if !force {
        return Ok(Written::Kept);
    }
    crate::fs::write_atomic(path, content.as_bytes())?;
    Ok(Written::Refreshed)
}

/// Write `name` under `root`, record it and say what happened.
pub(crate) fn write_and_report(
    root: &Path,
    name: &str,
    content: &str,
    force: bool,
) -> Result<bool> {
    use colored::Colorize;
    let written = write(&root.join(name), content, force)?;
    let changed = matches!(written, Written::Created | Written::Refreshed);
    super::print_file_status(changed, name);
    match written {
        Written::Kept => eprintln!(
            "  {} {name} already exists — not overwritten. Run with --force to replace.",
            "⚠".yellow()
        ),
        Written::Theirs => eprintln!(
            "  {} {name} is the project's own (not written by zforge) — kept. \
             zforge's workflow notes are in {}",
            "ℹ".cyan(),
            ".zforge/README.md".cyan()
        ),
        Written::Created | Written::Refreshed => {}
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_the_user_wrote_is_never_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        std::fs::write(&path, "# My service\n").unwrap();
        assert_eq!(write(&path, "new", true).unwrap(), Written::Theirs);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# My service\n");
    }

    #[test]
    fn a_generated_file_is_refreshed_only_with_force() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        assert_eq!(write(&path, "a", true).unwrap(), Written::Created);
        let old = format!("# app\n{MARKER}\nold\n");
        std::fs::write(&path, &old).unwrap();
        assert_eq!(write(&path, "new", false).unwrap(), Written::Kept);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        assert_eq!(write(&path, "new", true).unwrap(), Written::Refreshed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn a_file_that_only_mentions_the_marker_is_theirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        let doc = format!(
            "# CLAUDE.md\n\n| `instructions.rs` | refreshes a file carrying `{MARKER}` |\n"
        );
        std::fs::write(&path, &doc).unwrap();
        assert_eq!(write(&path, "new", true).unwrap(), Written::Theirs);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), doc);
    }

    #[test]
    fn every_template_carries_the_marker() {
        for t in [
            include_str!("../../../templates/CLAUDE.md"),
            include_str!("../../../templates/AGENTS.md"),
        ] {
            assert!(generated(t));
        }
    }
}
