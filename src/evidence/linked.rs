//! Code the git tree hash does not cover (IMP-002).
//!
//! A tree records a submodule as its commit and a symlink as its target
//! *string*, so editing a file inside a checked-out submodule, or the file a
//! tracked symlink out of the project points at, left the fingerprint
//! unchanged. Those inputs are fingerprinted separately and folded into the
//! candidate:
//!
//! - a checked-out submodule: the fingerprint of its own working tree
//!   (recursively, so nested submodules are covered too);
//! - a tracked symlink resolving outside the project: the content of the
//!   file, or of every file under the directory, it points at. Symlinks
//!   inside the project need nothing — their target is fingerprinted as
//!   part of the tree — and a dangling one is just its target string.
//!
//! A project with neither keeps the plain tree hash, so reports written
//! before this still match.

use super::candidate::{git, git_with_input, stdout};
use std::path::{Path, PathBuf};

const MODE_SUBMODULE: &str = "160000";
const MODE_SYMLINK: &str = "120000";

/// `code_tree`, or — when the project has linked inputs — a hash of the tree
/// together with them. `fingerprint` fingerprints a submodule's work tree.
pub(super) fn with_linked_inputs(
    root: &Path,
    index: Option<&Path>,
    code_tree: String,
    fingerprint: fn(&Path) -> Result<String, String>,
) -> Result<String, String> {
    let inputs = linked_inputs(root, index, fingerprint)?;
    if inputs.is_empty() {
        return Ok(code_tree);
    }
    let manifest = format!("tree {code_tree}\n{}\n", inputs.join("\n"));
    hash_bytes(root, manifest.as_bytes())
}

/// One line per linked input, in index order (sorted by path).
fn linked_inputs(
    root: &Path,
    index: Option<&Path>,
    fingerprint: fn(&Path) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    // Paths are relative to `root` and limited to it, so a project in a
    // repository subdirectory sees only its own entries.
    let listing = git(root, index, &["ls-files", "-s", "-z"])?;
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("resolve {}: {e}", root.display()))?;

    let mut inputs = Vec::new();
    for entry in listing.stdout.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let full = root.join(path);
        match meta.split(' ').next() {
            Some(MODE_SUBMODULE) if full.join(".git").exists() => {
                let hash = fingerprint(&full).map_err(|e| format!("submodule {path}: {e}"))?;
                inputs.push(format!("submodule {path} {hash}"));
            }
            Some(MODE_SYMLINK) => {
                if let Some(hash) = outside_target_hash(root, &canonical_root, &full)? {
                    inputs.push(format!("symlink {path} {hash}"));
                }
            }
            _ => {}
        }
    }
    Ok(inputs)
}

/// Content hash of what `link` points at, when that is outside the project.
fn outside_target_hash(
    root: &Path,
    canonical_root: &Path,
    link: &Path,
) -> Result<Option<String>, String> {
    let Ok(target) = link.canonicalize() else {
        return Ok(None); // dangling: the target string is in the tree
    };
    if target.starts_with(canonical_root) {
        return Ok(None);
    }
    let files = if target.is_dir() {
        files_under(&target)
    } else {
        vec![target.clone()]
    };
    let paths: String = files.iter().map(|f| format!("{}\n", f.display())).collect();
    let hashes = stdout(git_with_input(
        root,
        &["hash-object", "--stdin-paths"],
        paths.as_bytes(),
    )?);
    let listing: String = files
        .iter()
        .zip(hashes.lines())
        .map(|(f, h)| {
            let rel = f.strip_prefix(&target).unwrap_or(f);
            format!("{h} {}\n", rel.display())
        })
        .collect();
    hash_bytes(root, listing.as_bytes()).map(Some)
}

/// Regular files under `dir`, sorted, without following nested symlinks
/// (which could loop).
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => pending.push(entry.path()),
                Ok(t) if t.is_file() => files.push(entry.path()),
                _ => {}
            }
        }
    }
    files.sort();
    files
}

fn hash_bytes(root: &Path, bytes: &[u8]) -> Result<String, String> {
    Ok(stdout(git_with_input(
        root,
        &["hash-object", "--stdin"],
        bytes,
    )?))
}
