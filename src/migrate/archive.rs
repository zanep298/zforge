//! Where a migration puts what it takes away.
//!
//! Every file or directory a migration removes, and a copy of every file it
//! rewrites, lands under `<base>/v1-archive/<timestamp>/` at the same path
//! relative to `root` — so restoring one is a `mv` back. Nothing is ever
//! deleted outright.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const DIR: &str = "v1-archive";

pub struct Archive {
    root: PathBuf,
    dir: PathBuf,
}

impl Archive {
    /// An archive for paths under `root`, kept in `<base>/v1-archive/<ts>/`.
    /// Nothing is created until something is archived.
    pub fn new(root: &Path, base: &Path) -> Self {
        let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
        Self {
            root: root.to_path_buf(),
            dir: base.join(DIR).join(ts),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Move `rel` (a file or a directory) into the archive.
    pub fn take(&self, rel: &str) -> Result<()> {
        let from = self.root.join(rel);
        let to = self.target(rel)?;
        std::fs::rename(&from, &to)
            .with_context(|| format!("move {} to {}", from.display(), to.display()))
    }

    /// Keep a copy of the file `rel` in the archive, leaving it in place.
    pub fn keep_copy(&self, rel: &str) -> Result<()> {
        let from = self.root.join(rel);
        let to = self.target(rel)?;
        std::fs::copy(&from, &to)
            .map(|_| ())
            .with_context(|| format!("copy {} to {}", from.display(), to.display()))
    }

    fn target(&self, rel: &str) -> Result<PathBuf> {
        let to = self.dir.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_moves_and_keep_copy_copies_at_the_same_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/f.md"), "f").unwrap();
        std::fs::write(root.join("g.yaml"), "g").unwrap();

        let archive = Archive::new(root, &root.join("base"));
        assert!(!archive.dir().exists(), "nothing created up front");
        archive.take("a/b").unwrap();
        archive.keep_copy("g.yaml").unwrap();

        assert!(!root.join("a/b").exists());
        assert_eq!(
            std::fs::read_to_string(archive.dir().join("a/b/f.md")).unwrap(),
            "f"
        );
        assert!(root.join("g.yaml").exists());
        assert!(archive.dir().join("g.yaml").exists());
        assert!(archive.dir().starts_with(root.join("base").join(DIR)));
    }
}
