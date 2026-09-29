//! ONBOARD (v2): the project's own knowledge, reviewed and accepted the
//! same way an intake file is — one implementation, shared through
//! [`docs::DocSet`] (REQ-005). Three files for now: `domain.md`,
//! `conventions.md`, `rules.md`, under `knowledge.dir` (default
//! `docs/knowledge/`), committed with the code (REQ-006).
//!
//! What the code cannot show yet — item IDs and evidence (TASK-002), the
//! probe and baseline (TASK-003), staleness (TASK-004), drafting
//! (TASK-008) — is out of scope here: this task only makes the three files
//! reviewable, acceptable and revisable.

pub mod docs;

use crate::config::Config;
use crate::intake::lint::{self, Issue, Severity};
use crate::intake::record;
use crate::intake::review as intake_review;
use crate::intake::status::{DocStatus, Rev};
use anyhow::{bail, Context, Result};
use docs::DocSet;
use std::path::{Path, PathBuf};

/// The three files the user reviews and accepts.
pub const FILES: [&str; 3] = ["domain.md", "conventions.md", "rules.md"];

/// The project's knowledge: `knowledge.dir` plus its `.records/`.
#[derive(Debug, Clone)]
pub struct Knowledge {
    pub dir: PathBuf,
}

impl Knowledge {
    pub fn open(config: &Config) -> Self {
        Self {
            dir: config.knowledge_dir(),
        }
    }
}

impl DocSet for Knowledge {
    fn id(&self) -> &str {
        "knowledge"
    }

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, rel: &str) -> Result<PathBuf> {
        validate_file(rel)?;
        Ok(self.dir.join(rel))
    }

    fn files(&self) -> Vec<String> {
        FILES.iter().map(|s| s.to_string()).collect()
    }

    fn lint(&self, rel: &str, text: &str) -> Vec<Issue> {
        lint_issues(rel, text)
    }
}

/// A file other than the three known ones is refused (Output, AC-06).
pub fn validate_file(rel: &str) -> Result<()> {
    if FILES.contains(&rel) {
        Ok(())
    } else {
        bail!(
            "{rel:?} is not a knowledge file: use one of {}",
            FILES.join(", ")
        );
    }
}

/// Structural checks shared with intake files, minus everything specific
/// to stages and tasks (requirement and task IDs, acceptance criteria):
/// content is not empty, and there is an "Open questions" section. Item
/// IDs and evidence are checked from ONBOARD TASK-002 on.
pub fn lint_issues(rel: &str, text: &str) -> Vec<Issue> {
    let mut issues = Vec::new();
    let (_, body) = lint::split_frontmatter(text);
    let clean = lint::strip_comments(body);
    if clean
        .lines()
        .all(|l| l.trim().is_empty() || l.starts_with('#'))
    {
        issues.push(Issue {
            file: rel.to_string(),
            severity: Severity::Error,
            message: "has no content yet (only headings)".into(),
        });
    }
    let secs = lint::sections(&clean);
    if lint::section(&secs, lint::OPEN_QUESTIONS).is_none() {
        issues.push(Issue {
            file: rel.to_string(),
            severity: Severity::Warning,
            message: format!("has no \"{}\" section", lint::OPEN_QUESTIONS),
        });
    }
    issues
}

pub fn statuses(k: &Knowledge) -> Result<Vec<DocStatus>> {
    intake_review::statuses(k)
}

pub fn file_status(k: &Knowledge, rel: &str) -> Result<DocStatus> {
    intake_review::file_status(k, rel)
}

pub fn pending_review(k: &Knowledge, rel: &str) -> Result<Rev> {
    intake_review::pending_review(k, rel)
}

/// Send `rel`'s current content for review (REQ-005). On the first
/// knowledge review, `.gitattributes` gains the union-merge line for the
/// decision log (Output).
pub fn review(project_root: &Path, k: &Knowledge, rel: &str) -> Result<intake_review::Reviewed> {
    ensure_gitattributes(project_root, &k.dir)?;
    intake_review::review(k, rel)
}

/// Accept the revision under review. Refused while it still has an
/// unchecked open question (Output, AC-03).
pub fn accept(k: &Knowledge, rel: &str, by: Option<String>) -> Result<Rev> {
    let pending = intake_review::pending_review(k, rel)?;
    let text = record::read_snapshot(k, rel, pending.revision)?;
    if let Some(q) = lint::open_questions(&text).into_iter().next() {
        bail!(
            "{rel} revision {} still has an open question: {q}",
            pending.revision
        );
    }
    intake_review::accept(k, rel, by)
}

pub fn revise(k: &Knowledge, rel: &str, by: Option<String>, note: &str) -> Result<Rev> {
    intake_review::revise(k, rel, by, note)
}

/// Add the union-merge line for `<dir>/.records/decisions.jsonl` to
/// `.gitattributes` at the repository root, once, leaving the rest of the
/// file as it was (Output).
fn ensure_gitattributes(project_root: &Path, knowledge_dir: &Path) -> Result<()> {
    let repo_root = crate::run::git::ok(project_root, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_root.to_path_buf());
    let rel = knowledge_dir
        .strip_prefix(&repo_root)
        .unwrap_or(knowledge_dir)
        .to_string_lossy()
        .replace('\\', "/");
    let line = format!("{rel}/.records/decisions.jsonl merge=union");
    let path = repo_root.join(".gitattributes");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    if text.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    let mut updated = text;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&line);
    updated.push('\n');
    crate::fs::write_atomic(&path, updated.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
    }

    const DOMAIN_TEXT: &str = "# Domain\n\nA thing. (src/x.rs:1)\n\n## Open questions\n";

    fn knowledge(root: &Path) -> Knowledge {
        Knowledge {
            dir: root.join("docs").join("knowledge"),
        }
    }

    #[test]
    fn review_writes_a_revision_named_after_the_full_file_name() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();
        std::fs::write(k.dir.join("domain.md"), DOMAIN_TEXT).unwrap();

        let r = review(tmp.path(), &k, "domain.md").unwrap();
        assert_eq!(r.revision, 1);
        let snap = k
            .dir
            .join(".records")
            .join("revisions")
            .join("domain.md")
            .join("1.md");
        assert!(snap.is_file(), "{}", snap.display());
        assert_eq!(std::fs::read_to_string(snap).unwrap(), DOMAIN_TEXT);
        assert_eq!(
            file_status(&k, "domain.md").unwrap().state,
            crate::intake::status::DocState::InReview
        );
    }

    #[test]
    fn a_file_other_than_the_three_is_refused() {
        assert!(validate_file("glossary.md").is_err());
        for f in FILES {
            assert!(validate_file(f).is_ok());
        }
    }

    #[test]
    fn accept_needs_a_terminal_confirmation_upstream_and_refuses_open_questions() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();
        let with_open_question =
            "# Domain\n\nA thing. (src/x.rs:1)\n\n## Open questions\n\n- [ ] is this right?\n";
        std::fs::write(k.dir.join("domain.md"), with_open_question).unwrap();
        review(tmp.path(), &k, "domain.md").unwrap();

        let err = accept(&k, "domain.md", None).unwrap_err().to_string();
        assert!(err.contains("open question"), "{err}");

        std::fs::write(k.dir.join("domain.md"), DOMAIN_TEXT).unwrap();
        review(tmp.path(), &k, "domain.md").unwrap();
        let rev = accept(&k, "domain.md", Some("zane".into())).unwrap();
        assert_eq!(rev.revision, 2);
    }

    #[test]
    fn gitattributes_gains_the_union_line_once_and_keeps_other_lines() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        std::fs::write(tmp.path().join(".gitattributes"), "*.png binary\n").unwrap();
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();
        std::fs::write(k.dir.join("domain.md"), DOMAIN_TEXT).unwrap();

        review(tmp.path(), &k, "domain.md").unwrap();
        let text = std::fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert!(text.contains("*.png binary"));
        assert!(text.contains("docs/knowledge/.records/decisions.jsonl merge=union"));
        let occurrences = text.matches("merge=union").count();

        std::fs::write(k.dir.join("domain.md"), format!("{DOMAIN_TEXT}more.\n")).unwrap();
        review(tmp.path(), &k, "domain.md").unwrap();
        let text_again = std::fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert_eq!(text_again.matches("merge=union").count(), occurrences);
    }

    #[test]
    fn config_dir_moves_files_and_records_under_it() {
        let yaml = "project:\n  name: p\nknowledge:\n  dir: kb\n";
        let mut cfg: crate::config::Config = serde_yaml::from_str(yaml).unwrap();
        cfg.config_file = std::path::PathBuf::from("/tmp/nonexistent/.zforge/config.yaml");
        let k = Knowledge::open(&cfg);
        assert!(k.dir.ends_with("kb"), "{}", k.dir.display());
        assert!(
            k.records_dir().ends_with("kb/.records"),
            "{}",
            k.records_dir().display()
        );
    }
}
