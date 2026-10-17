//! What zforge wrote, so an upgrade can tell its own files from the user's
//! edits.
//!
//! A manifest (`.shipped.json`) records, per file, the SHA-256 of the
//! content zforge last wrote there and the zforge version that wrote it.
//! [`sync`] then brings a set of files to this version:
//!
//! - missing → written;
//! - already this version's content → recorded, untouched;
//! - recorded and unchanged since → replaced by this version's (`updated`);
//! - recorded but changed since → the user edited it: kept and reported;
//! - not recorded (written before the manifest existed) → per file,
//!   [`Unrecorded::Replace`] (old copy archived first) or
//!   [`Unrecorded::Keep`].
//!
//! [`Mode::Force`] replaces every differing file, archiving the old copy;
//! [`Mode::Overwrite`] replaces without archiving (a re-render the user
//! asked for, e.g. after `zforge models set`). Archives go to
//! `<archive base>/archive/<ts>/` at the file's relative path.

use crate::migrate::archive::Archive;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// File name of the manifest, in the store root (`~/.zforge/.shipped.json`)
/// or a project's `.zforge/`.
pub const MANIFEST: &str = ".shipped.json";
/// Directory, beside the manifest, that old copies are archived under.
pub const ARCHIVE_DIR: &str = "archive";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// The zforge version that last wrote the files.
    #[serde(default)]
    pub version: String,
    /// Path relative to the synced root → SHA-256 (hex) of what was written.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

impl Manifest {
    /// `None` when there is no manifest or it cannot be read — treated as a
    /// store written before manifests existed.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save(&mut self, path: &Path) -> Result<()> {
        self.version = env!("CARGO_PKG_VERSION").to_string();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        crate::fs::write_atomic(path, &json)
    }
}

/// SHA-256 of `bytes`, lowercase hex.
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Update what zforge wrote and the user left alone; keep edits.
    Refresh,
    /// Replace every differing file, archiving the old copy.
    Force,
    /// Replace every differing file without archiving.
    Overwrite,
}

/// What to do with a file that differs from this version's and that the
/// manifest has no record of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unrecorded {
    /// Archive the old copy, then write this version's.
    Replace,
    /// Keep it and report it, like a user edit.
    Keep,
}

#[derive(Debug, Default)]
pub struct Report {
    /// Written where nothing was.
    pub created: Vec<String>,
    /// Replaced — zforge's own earlier content, unedited.
    pub updated: Vec<String>,
    /// Replaced after archiving the old copy.
    pub archived: Vec<String>,
    /// Differs from this version's and was kept (the user's edit).
    pub kept: Vec<String>,
    /// Already this version's content.
    pub current: usize,
    /// Where old copies went, when any did.
    pub archive_dir: Option<PathBuf>,
}

impl Report {
    /// Files written by this sync.
    pub fn written(&self) -> usize {
        self.created.len() + self.updated.len() + self.archived.len()
    }

    pub fn total(&self) -> usize {
        self.written() + self.kept.len() + self.current
    }

    /// Whether anything was written or kept — worth a line to the user.
    pub fn is_quiet(&self) -> bool {
        self.written() == 0 && self.kept.is_empty()
    }

    /// Counts, e.g. `2 created, 3 updated (1 old copy archived), 1 kept`.
    pub fn counts(&self) -> String {
        let mut parts = Vec::new();
        if !self.created.is_empty() {
            parts.push(format!("{} created", self.created.len()));
        }
        let replaced = self.updated.len() + self.archived.len();
        if replaced > 0 {
            let mut s = format!("{replaced} updated");
            if !self.archived.is_empty() {
                s.push_str(&format!(
                    " ({} old cop{} archived)",
                    self.archived.len(),
                    if self.archived.len() == 1 { "y" } else { "ies" }
                ));
            }
            parts.push(s);
        }
        if !self.kept.is_empty() {
            parts.push(format!("{} kept (your edits)", self.kept.len()));
        }
        if self.current > 0 {
            parts.push(format!("{} current", self.current));
        }
        if parts.is_empty() {
            "nothing to do".into()
        } else {
            parts.join(", ")
        }
    }
}

/// Bring `files` (path relative to `root` → this version's content) under
/// `root` to this version, recording what was written in `manifest`.
/// Entries of the manifest for other files are kept, so several syncs may
/// share one manifest. `unrecorded` decides, per path, a differing file the
/// manifest does not know.
pub fn sync(
    root: &Path,
    manifest: &Path,
    files: &[(String, String)],
    mode: Mode,
    unrecorded: &dyn Fn(&str) -> Unrecorded,
) -> Result<Report> {
    let base = manifest.parent().unwrap_or(root);
    let archive = Archive::named(root, base, ARCHIVE_DIR);
    let mut record = Manifest::load(manifest).unwrap_or_default();
    let mut report = Report::default();

    for (rel, content) in files {
        let path = root.join(rel);
        let new_hash = digest(content.as_bytes());
        let on_disk = std::fs::read(&path).ok();
        let Some(on_disk) = on_disk else {
            write(&path, content)?;
            record.files.insert(rel.clone(), new_hash);
            report.created.push(rel.clone());
            continue;
        };
        if on_disk == content.as_bytes() {
            record.files.insert(rel.clone(), new_hash);
            report.current += 1;
            continue;
        }
        let untouched = record.files.get(rel).map(|h| *h == digest(&on_disk));
        let archive_first = match (mode, untouched) {
            (Mode::Overwrite, _) | (Mode::Refresh, Some(true)) => false,
            (Mode::Force, _) => true,
            (Mode::Refresh, Some(false)) => {
                report.kept.push(rel.clone());
                continue;
            }
            (Mode::Refresh, None) => match unrecorded(rel) {
                Unrecorded::Replace => true,
                Unrecorded::Keep => {
                    report.kept.push(rel.clone());
                    continue;
                }
            },
        };
        if archive_first {
            archive.keep_copy(rel)?;
            report.archive_dir = Some(archive.dir().to_path_buf());
            report.archived.push(rel.clone());
        } else {
            report.updated.push(rel.clone());
        }
        write(&path, content)?;
        record.files.insert(rel.clone(), new_hash);
    }

    record.save(manifest)?;
    Ok(report)
}

fn write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    // A symlink (older zforge linked client files into the store) is
    // replaced by the file itself, never written through.
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        std::fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    }
    crate::fs::write_atomic(path, content.as_bytes())
}

/// The state of a synced root against this version, read-only (`doctor`).
#[derive(Debug)]
pub struct Status {
    /// The manifest's version; `None` without a manifest.
    pub version: Option<String>,
    /// Files missing.
    pub missing: Vec<String>,
    /// Files that differ from this version's and that the user changed
    /// since zforge wrote them (or the manifest does not know).
    pub edited: Vec<String>,
    /// Files that differ but are unedited zforge content — a refresh
    /// would update them.
    pub outdated: Vec<String>,
}

impl Status {
    /// Whether the manifest is this binary's version and every file is
    /// there and current or the user's own edit.
    pub fn is_current(&self) -> bool {
        self.version.as_deref() == Some(env!("CARGO_PKG_VERSION"))
            && self.missing.is_empty()
            && self.outdated.is_empty()
    }
}

pub fn status(root: &Path, manifest: &Path, files: &[(String, String)]) -> Status {
    let record = Manifest::load(manifest);
    let mut status = Status {
        version: record.as_ref().map(|m| m.version.clone()),
        missing: Vec::new(),
        edited: Vec::new(),
        outdated: Vec::new(),
    };
    for (rel, content) in files {
        let Ok(on_disk) = std::fs::read(root.join(rel)) else {
            status.missing.push(rel.clone());
            continue;
        };
        if on_disk == content.as_bytes() {
            continue;
        }
        let recorded = record.as_ref().and_then(|m| m.files.get(rel));
        match recorded {
            Some(h) if *h == digest(&on_disk) => status.outdated.push(rel.clone()),
            _ => status.edited.push(rel.clone()),
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(r, c)| (r.to_string(), c.to_string()))
            .collect()
    }

    #[test]
    fn refresh_updates_unedited_keeps_edited_and_replaces_unrecorded() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manifest = root.join(MANIFEST);
        let v1 = files(&[("a.md", "a1"), ("b.md", "b1")]);
        let r = sync(root, &manifest, &v1, Mode::Refresh, &|_| Unrecorded::Keep).unwrap();
        assert_eq!(r.created.len(), 2);

        std::fs::write(root.join("b.md"), "b-edited").unwrap();
        std::fs::write(root.join("c.md"), "c-old").unwrap();
        let v2 = files(&[("a.md", "a2"), ("b.md", "b2"), ("c.md", "c2")]);
        let r = sync(root, &manifest, &v2, Mode::Refresh, &|_| {
            Unrecorded::Replace
        })
        .unwrap();
        assert_eq!(r.updated, vec!["a.md"]);
        assert_eq!(r.kept, vec!["b.md"]);
        assert_eq!(r.archived, vec!["c.md"]);
        assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "a2");
        assert_eq!(
            std::fs::read_to_string(root.join("b.md")).unwrap(),
            "b-edited"
        );
        let dir = r.archive_dir.unwrap();
        assert!(dir.starts_with(root.join(ARCHIVE_DIR)));
        assert_eq!(std::fs::read_to_string(dir.join("c.md")).unwrap(), "c-old");

        // The edit stays an edit on the next run too.
        let r = sync(root, &manifest, &v2, Mode::Refresh, &|_| {
            Unrecorded::Replace
        })
        .unwrap();
        assert_eq!(r.kept, vec!["b.md"]);
        assert_eq!(r.current, 2);
        let s = status(root, &manifest, &v2);
        assert!(s.is_current());
        assert_eq!(s.edited, vec!["b.md"]);

        // Force replaces it, archiving the edit.
        let r = sync(root, &manifest, &v2, Mode::Force, &|_| Unrecorded::Keep).unwrap();
        assert_eq!(r.archived, vec!["b.md"]);
        assert_eq!(
            std::fs::read_to_string(r.archive_dir.unwrap().join("b.md")).unwrap(),
            "b-edited"
        );
        assert!(status(root, &manifest, &v2).edited.is_empty());
    }
}
