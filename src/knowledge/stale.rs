//! Stale knowledge citations (ONBOARD REQ-009, TASK-004): whether the code
//! an accepted item cites still says what the item does, has only moved, or
//! is gone.
//!
//! Only *accepted* revisions are checked (business rule 4: nothing but
//! review changes what a run or an intake reads) — a file with an unsent
//! draft does not affect this. For each cite, the text of its cited lines
//! at the file's `pinned` commit is compared with the same lines at HEAD:
//! equal is fresh; the same block found elsewhere in the HEAD file is
//! moved (its new range is reported, business rule 5); otherwise, or the
//! cited file gone at HEAD, it is stale. This is a plain text comparison
//! through git — no model, no network (Constraints) — and, since it reads
//! only `pinned` and `HEAD`, never the working tree, uncommitted edits
//! change nothing (AC-04).

use super::items::{self, Cite};
use super::Knowledge;
use crate::intake::record;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

/// Why a cite no longer matches the code it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The text at the cited lines differs at HEAD, and no block of the
    /// same text was found elsewhere in the file.
    Changed,
    /// The cited file no longer exists at HEAD.
    Gone,
}

/// One item's citation whose text no longer matches at HEAD (Output).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleItem {
    pub id: String,
    pub file: String,
    pub cite: Cite,
    pub reason: Reason,
}

/// A cite whose text is unchanged but sits at a different range at HEAD —
/// not stale (business rule 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Moved {
    pub id: String,
    pub file: String,
    pub cite: Cite,
    pub new_start: u32,
    pub new_end: u32,
}

/// Every stale and moved citation among the accepted knowledge files
/// (ONBOARD TASK-004 shared interface `stale::check`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub stale: Vec<StaleItem>,
    pub moved: Vec<Moved>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.stale.is_empty() && self.moved.is_empty()
    }
}

/// Check every accepted knowledge file's items against HEAD. A file with
/// no accepted revision, or whose accepted snapshot has no readable
/// `pinned` commit, contributes nothing — `lint` already refuses an item
/// with unreadable evidence before a file can be accepted at all.
pub fn check(k: &Knowledge) -> Result<Report> {
    let mut report = Report::default();
    for file in super::FILES {
        let status = super::file_status(k, file)?;
        let Some(accepted) = &status.accepted else {
            continue;
        };
        let Ok(text) = record::read_snapshot(k, file, accepted.revision) else {
            continue;
        };
        let Some(pinned) = items::pinned(&text) else {
            continue;
        };
        for item in items::items(&text) {
            for cite in item.evidence {
                match check_cite(&k.dir, &pinned, &cite) {
                    CiteState::Fresh => {}
                    CiteState::Moved { start, end } => report.moved.push(Moved {
                        id: item.id.clone(),
                        file: file.to_string(),
                        cite,
                        new_start: start,
                        new_end: end,
                    }),
                    CiteState::Stale(reason) => report.stale.push(StaleItem {
                        id: item.id.clone(),
                        file: file.to_string(),
                        cite,
                        reason,
                    }),
                }
            }
        }
    }
    Ok(report)
}

enum CiteState {
    Fresh,
    Moved { start: u32, end: u32 },
    Stale(Reason),
}

fn check_cite(dir: &Path, pinned: &str, cite: &Cite) -> CiteState {
    // The evidence was checked to exist at `pinned` when the file was
    // linted for review; if it cannot be read now, `pinned` itself (or its
    // blob) is unreadable — report it rather than silently skip it.
    let Some(pinned_text) = show_at(dir, pinned, &cite.path) else {
        return CiteState::Stale(Reason::Changed);
    };
    let pinned_lines: Vec<&str> = pinned_text.lines().collect();
    let (start, end) = (cite.start as usize, cite.end as usize);
    if start == 0 || start > end || end > pinned_lines.len() {
        return CiteState::Stale(Reason::Changed);
    }
    let block = pinned_lines[start - 1..end].join("\n");

    let Some(head_text) = show_at(dir, "HEAD", &cite.path) else {
        return CiteState::Stale(Reason::Gone);
    };
    let head_lines: Vec<&str> = head_text.lines().collect();
    if end <= head_lines.len() && head_lines[start - 1..end].join("\n") == block {
        return CiteState::Fresh;
    }
    match find_block(&head_lines, &block) {
        Some(new_start) => CiteState::Moved {
            start: new_start as u32,
            end: (new_start + (end - start)) as u32,
        },
        None => CiteState::Stale(Reason::Changed),
    }
}

/// The 1-based starting line of `block` (its lines joined by `\n`) inside
/// `lines`, by a plain substring search — no diff library needed
/// (03-solution, implementation suggestions).
fn find_block(lines: &[&str], block: &str) -> Option<usize> {
    let block_lines: Vec<&str> = block.split('\n').collect();
    if block_lines.is_empty() || block_lines.len() > lines.len() {
        return None;
    }
    lines
        .windows(block_lines.len())
        .position(|w| w == block_lines)
        .map(|i| i + 1)
}

/// `git show <rev>:<path>`'s raw output, or `None` when the path does not
/// exist at that revision.
fn show_at(dir: &Path, rev: &str, path: &str) -> Option<String> {
    let out = crate::run::git::run(dir, &["show", &format!("{rev}:{path}")]).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "user.name", "t"]);
    }

    fn write_and_commit(root: &Path, rel: &str, content: &str) -> String {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "c"]);
        head(root)
    }

    fn head(root: &Path) -> String {
        String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    fn knowledge(root: &Path) -> Knowledge {
        let dir = root.join("docs/knowledge");
        std::fs::create_dir_all(&dir).unwrap();
        Knowledge { dir }
    }

    fn numbered_lines(n: usize) -> String {
        (1..=n).map(|i| format!("line{i}\n")).collect()
    }

    /// Accept `domain.md` with one `DOM-001` item citing `src/x.rs:<start>-<end>`
    /// at `pinned`, then commit `after` on top of it (the state at HEAD).
    fn accept_domain(root: &Path, k: &Knowledge, cite: &str, pinned: &str) {
        let text = format!(
            "---\npinned: {pinned}\n---\n## m\n- DOM-001: a thing happens. ({cite})\n\n## Open questions\n"
        );
        std::fs::write(k.dir.join("domain.md"), &text).unwrap();
        crate::intake::review::review(k, "domain.md").unwrap();
        crate::intake::review::accept(k, "domain.md", None).unwrap();
        let _ = root;
    }

    /// AC-01: a commit changing the cited lines makes the item stale
    /// (`changed`).
    #[test]
    fn a_commit_changing_the_cited_lines_is_stale_changed() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let pinned = write_and_commit(tmp.path(), "src/x.rs", &numbered_lines(10));
        let k = knowledge(tmp.path());
        accept_domain(tmp.path(), &k, "src/x.rs:3", &pinned);

        // Change line 3 only.
        let mut lines: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
        lines[2] = "changed".to_string();
        write_and_commit(tmp.path(), "src/x.rs", &format!("{}\n", lines.join("\n")));

        let report = check(&k).unwrap();
        assert!(report.moved.is_empty(), "{report:?}");
        assert_eq!(report.stale.len(), 1, "{report:?}");
        assert_eq!(report.stale[0].id, "DOM-001");
        assert_eq!(report.stale[0].file, "domain.md");
        assert_eq!(report.stale[0].reason, Reason::Changed);
    }

    /// AC-02: a commit inserting lines above the cited ones leaves the item
    /// fresh and reports the cite's new range.
    #[test]
    fn lines_inserted_above_the_cite_are_moved_not_stale() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let pinned = write_and_commit(tmp.path(), "src/x.rs", &numbered_lines(10));
        let k = knowledge(tmp.path());
        accept_domain(tmp.path(), &k, "src/x.rs:3", &pinned);

        // Insert 2 lines above everything; line3's text now sits at line 5.
        let mut lines = vec!["new1".to_string(), "new2".to_string()];
        lines.extend((1..=10).map(|i| format!("line{i}")));
        write_and_commit(tmp.path(), "src/x.rs", &format!("{}\n", lines.join("\n")));

        let report = check(&k).unwrap();
        assert!(report.stale.is_empty(), "{report:?}");
        assert_eq!(report.moved.len(), 1, "{report:?}");
        let m = &report.moved[0];
        assert_eq!(m.id, "DOM-001");
        assert_eq!(m.new_start, 5);
        assert_eq!(m.new_end, 5);
    }

    /// AC-03: deleting the cited file makes the item stale (`gone`).
    #[test]
    fn a_deleted_cited_file_is_stale_gone() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let pinned = write_and_commit(tmp.path(), "src/x.rs", &numbered_lines(10));
        let k = knowledge(tmp.path());
        accept_domain(tmp.path(), &k, "src/x.rs:3", &pinned);

        std::fs::remove_file(tmp.path().join("src/x.rs")).unwrap();
        git(tmp.path(), &["add", "-A"]);
        git(tmp.path(), &["commit", "-q", "-m", "remove"]);

        let report = check(&k).unwrap();
        assert_eq!(report.stale.len(), 1, "{report:?}");
        assert_eq!(report.stale[0].reason, Reason::Gone);
    }

    /// AC-04: uncommitted edits to the cited file never change the result —
    /// only HEAD counts.
    #[test]
    fn uncommitted_edits_do_not_affect_the_result() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let pinned = write_and_commit(tmp.path(), "src/x.rs", &numbered_lines(10));
        let k = knowledge(tmp.path());
        accept_domain(tmp.path(), &k, "src/x.rs:3", &pinned);

        // Dirty the working tree without committing.
        std::fs::write(tmp.path().join("src/x.rs"), "utterly different\n").unwrap();

        let report = check(&k).unwrap();
        assert!(report.is_empty(), "{report:?}");
    }

    /// Unchanged text at the exact same range is fresh, and produces
    /// neither a stale nor a moved entry.
    #[test]
    fn unchanged_text_at_the_same_range_is_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let pinned = write_and_commit(tmp.path(), "src/x.rs", &numbered_lines(10));
        let k = knowledge(tmp.path());
        accept_domain(tmp.path(), &k, "src/x.rs:3", &pinned);

        let report = check(&k).unwrap();
        assert!(report.is_empty(), "{report:?}");
    }

    /// A knowledge file that was never accepted (still a draft) is not
    /// checked at all.
    #[test]
    fn a_file_with_no_accepted_revision_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let k = knowledge(tmp.path());
        std::fs::write(
            k.dir.join("domain.md"),
            "# Domain\n\nA thing happens. (src/x.rs:1)\n\n## Open questions\n",
        )
        .unwrap();

        let report = check(&k).unwrap();
        assert!(report.is_empty(), "{report:?}");
    }

    /// A multi-line cite (`start-end`) that moves keeps its full length at
    /// the new range.
    #[test]
    fn a_multiline_cite_moves_as_a_whole_block() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let pinned = write_and_commit(tmp.path(), "src/x.rs", &numbered_lines(10));
        let k = knowledge(tmp.path());
        accept_domain(tmp.path(), &k, "src/x.rs:3-5", &pinned);

        let mut lines = vec!["new1".to_string()];
        lines.extend((1..=10).map(|i| format!("line{i}")));
        write_and_commit(tmp.path(), "src/x.rs", &format!("{}\n", lines.join("\n")));

        let report = check(&k).unwrap();
        assert!(report.stale.is_empty(), "{report:?}");
        assert_eq!(report.moved.len(), 1, "{report:?}");
        assert_eq!(report.moved[0].new_start, 4);
        assert_eq!(report.moved[0].new_end, 6);
    }
}
