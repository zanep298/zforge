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
