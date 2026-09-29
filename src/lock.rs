//! Advisory lock on one intake, run or handover claim.
//!
//! Built on `fs2` `flock` — the same primitive the registry uses.
//!
//! Semantics:
//!   - One holder per id. `try_acquire` is non-blocking; it returns `Busy`
//!     with the current owner PID when the lock is held elsewhere.
//!   - Lock file at `<dir>/<ID>/.task.lock` (the name predates intakes and
//!     runs; it is kept so an older zforge and this one exclude each other).
//!     Content = owner PID (single line). The file persists across
//!     acquisitions — flock is the mutex, the file is just an inode.
//!   - Held for the lifetime of the `LockGuard`. Dropping the guard releases
//!     the lock. Process death also releases (kernel cleanup).
//!   - NOT re-entrant. `flock` is bound to the open file description, so a
//!     second acquire from the same process conflicts with the first exactly
//!     as one from another process would.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

const LOCK_FILE: &str = ".task.lock";

/// Held while a process owns the lock. Drop releases.
#[derive(Debug)]
pub struct LockGuard {
    file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Best-effort unlock. The fd close on `File` drop also releases the
        // flock, so a failure here is harmless.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Why an acquire failed. `Busy` carries the owner PID when readable from
/// the lock file (newly created files race; treat unreadable as `None`).
#[derive(Debug)]
pub enum LockError {
    Busy { id: String, owner_pid: Option<u32> },
    Io(anyhow::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Busy {
                id,
                owner_pid: Some(pid),
            } => write!(
                f,
                "{id} is locked by another zforge process (pid {pid}). \
                 If that process is gone, retry; flock auto-releases on exit."
            ),
            LockError::Busy {
                id,
                owner_pid: None,
            } => write!(
                f,
                "{id} is locked by another zforge process. \
                 If you're sure it crashed, retry — flock auto-releases on exit."
            ),
            LockError::Io(e) => write!(f, "lock I/O error: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

/// [`try_acquire`] with the error converted for `anyhow` callers.
pub fn lock(dir: &Path, id: &str) -> anyhow::Result<LockGuard> {
    try_acquire(dir, id).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Non-blocking acquire. Returns `Busy` immediately when another process
/// holds the lock — caller decides whether to retry or exit. Lock file at
/// `<dir>/<id>/.task.lock`.
pub fn try_acquire(dir: &Path, id: &str) -> Result<LockGuard, LockError> {
    try_acquire_at(&dir.join(id), id)
}

/// [`lock_at`] with the error converted for `anyhow` callers.
pub fn lock_at(dir: &Path, id: &str) -> anyhow::Result<LockGuard> {
    try_acquire_at(dir, id).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Like [`try_acquire`], but `dir` is already the exact directory to lock
/// — no further `<id>` join. `id` still names the holder in messages. For
/// a caller that picked its own lock location (the knowledge document set
/// locks under its own `.records/`, not `<parent>/<id>`).
pub fn try_acquire_at(dir: &Path, id: &str) -> Result<LockGuard, LockError> {
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Err(LockError::Io(
            anyhow::Error::from(e).context(format!("create lock dir {dir:?}")),
        ));
    }
    let path = dir.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| LockError::Io(anyhow::Error::from(e).context(format!("open {path:?}"))))?;

    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            // Lock held — write our PID for diagnostics. Failure here is
            // not fatal; the lock semantics come from flock, not the file
            // content.
            let _ = file.set_len(0);
            let _ = writeln!(&file, "{}", std::process::id());
            Ok(LockGuard { file })
        }
        Err(_) => Err(LockError::Busy {
            id: id.to_string(),
            owner_pid: read_pid(&path),
        }),
    }
}

/// Read the PID written by the current holder. Returns `None` when the
/// file is empty (race during creation) or unparseable.
fn read_pid(path: &Path) -> Option<u32> {
    let mut f = File::open(path).ok()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;
    buf.trim().parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn second_acquire_returns_busy_while_first_held() {
        let tmp = TempDir::new().unwrap();
        let guard = try_acquire(tmp.path(), "T1").expect("first acquire");

        match try_acquire(tmp.path(), "T1") {
            Err(LockError::Busy { id, owner_pid }) => {
                assert_eq!(id, "T1");
                assert_eq!(owner_pid, Some(std::process::id()));
            }
            other => panic!("expected Busy, got {other:?}"),
        }

        drop(guard);
        // After release, acquire succeeds again.
        let _g2 = try_acquire(tmp.path(), "T1").expect("re-acquire after drop");
    }

    #[test]
    fn different_ids_dont_block_each_other() {
        let tmp = TempDir::new().unwrap();
        let _g1 = try_acquire(tmp.path(), "T1").expect("T1");
        let _g2 = try_acquire(tmp.path(), "T2").expect("T2 should not block on T1");
    }

    #[test]
    fn lock_file_persists_after_release() {
        let tmp = TempDir::new().unwrap();
        drop(try_acquire(tmp.path(), "T1").expect("acquire"));
        assert!(
            tmp.path().join("T1").join(LOCK_FILE).exists(),
            "lock file kept around — inode reused for next holder"
        );
    }

    #[test]
    fn busy_error_displays_helpful_message() {
        let tmp = TempDir::new().unwrap();
        let _guard = try_acquire(tmp.path(), "T1").unwrap();
        let msg = try_acquire(tmp.path(), "T1").unwrap_err().to_string();
        assert!(msg.contains("T1"));
        assert!(msg.contains("flock"));
    }
}
