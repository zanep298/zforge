//! The shared review machinery (ONBOARD REQ-005): a directory of
//! reviewable Markdown files plus its `.records/` — numbered revision
//! snapshots and an append-only decision log, exactly as an intake keeps
//! them (see `intake` module docs). `intake::Intake` and
//! [`super::Knowledge`] both implement [`DocSet`]; `intake::review`,
//! `intake::record` and `intake::status` are written once against it
//! (03-solution "Binding decisions": no second implementation).

use crate::intake::lint::Issue;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// `.records/` beside a document set's directory, by default.
pub const RECORDS_DIR: &str = ".records";

/// Which kind of document set [`open`] opens. The one constructor for
/// both, so nothing opens either kind another way (04-breakdown "Shared
/// interfaces").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Intake,
    Knowledge,
}

/// Open a document set of `kind`, rooted at `project_root`. `id` names the
/// intake (`.zforge/intakes/<id>/`) and is ignored for [`Kind::Knowledge`]
/// — there is only one project knowledge, at `config.knowledge_dir()`.
pub fn open(project_root: &Path, kind: Kind, id: &str) -> Result<Box<dyn DocSet>> {
    match kind {
        Kind::Intake => Ok(Box::new(crate::intake::Intake::open(project_root, id)?)),
        Kind::Knowledge => {
            let config_path = project_root.join(".zforge").join("config.yaml");
            let config = crate::config::load_from(&config_path)?;
            Ok(Box::new(crate::knowledge::Knowledge::open(&config)))
        }
    }
}

/// One reviewable document set — an intake, or the project's knowledge.
pub trait DocSet {
    /// Short identifier used in messages: an intake id, or `"knowledge"`.
    fn id(&self) -> &str;

    /// The directory the reviewable files live in.
    fn dir(&self) -> &Path;

    /// `.records/` beside [`Self::dir`]. Default: `dir().join(".records")`.
    fn records_dir(&self) -> PathBuf {
        self.dir().join(RECORDS_DIR)
    }

    /// The directory the review lock (`.task.lock`) lives in. Default: the
    /// document set's own directory — an intake locks itself, as it always
    /// has. Knowledge overrides this to lock under its own `.records/`, so
    /// nothing but the three reviewed files and `.records/` ever appears
    /// directly under `knowledge.dir` (Output, AC-03).
    fn lock_dir(&self) -> PathBuf {
        self.dir().to_path_buf()
    }

    /// Absolute path of `rel`, after checking it names a reviewable file.
    fn file(&self, rel: &str) -> Result<PathBuf>;

    /// Every reviewable file this set has, in its own order.
    fn files(&self) -> Vec<String>;

    /// Structural issues in `rel`'s current text, checked before it is sent
    /// for review (errors refuse the review; warnings do not).
    fn lint(&self, rel: &str, text: &str) -> Vec<Issue>;

    /// Whether `rel`'s accepted revision may be sent for review again
    /// unchanged, because something it depends on was accepted since (D6).
    /// Document sets with no such dependency chain never do.
    fn reconfirm_needed(&self, rel: &str) -> Result<bool> {
        let _ = rel;
        Ok(false)
    }

    /// Called under the document set's lock, on the exact revision about
    /// to be accepted, right before the acceptance is recorded. The
    /// default accepts unconditionally. Knowledge overrides this to refuse
    /// a revision that still has an unchecked open question (ONBOARD
    /// business rule 2), checked on that same revision so a concurrent
    /// review cannot slip a different one past the check (Output, AC-06).
    fn check_acceptable(&self, rel: &str, revision: u32, text: &str) -> Result<()> {
        let _ = (rel, revision, text);
        Ok(())
    }

    /// Key of `rel`'s revision folder under `.records/revisions/`. Default:
    /// `rel` itself with `/` turned into `__` (so `domain.md`'s revisions
    /// live in `revisions/domain.md/`).
    fn snapshot_key(&self, rel: &str) -> String {
        rel.replace('/', "__")
    }

    /// CLI hint shown when a decision is attempted before `rel` was sent
    /// for review. Kind-specific because the command differs.
    fn review_hint(&self, rel: &str) -> String {
        format!("zforge onboard review {rel}")
    }
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

    /// AC-07: `open` reaches both kinds through the one constructor.
    #[test]
    fn open_opens_both_kinds() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        std::fs::create_dir_all(tmp.path().join(".zforge")).unwrap();
        std::fs::write(
            tmp.path().join(".zforge/config.yaml"),
            "project:\n  name: t\n",
        )
        .unwrap();
        crate::intake::review::create(tmp.path(), "F").unwrap();

        let intake = open(tmp.path(), Kind::Intake, "F").unwrap();
        assert_eq!(intake.id(), "F");
        assert!(intake.dir().ends_with("intakes/F"));

        let knowledge = open(tmp.path(), Kind::Knowledge, "unused").unwrap();
        assert_eq!(knowledge.id(), "knowledge");
        assert!(knowledge.dir().ends_with("docs/knowledge"));
    }
}
