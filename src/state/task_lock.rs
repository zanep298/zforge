//! Per-task advisory lock.
//!
//! Prevents two concurrent zforge processes from operating on the same task
//! at the same time (e.g. two `ship --async` calls, or one foreground `code`
//! plus one async `ship` racing on the same `.state.yaml`). Built on `fs2`
//! `flock` — same primitive the registry uses.
//!
//! Semantics:
//!   - One holder per task. `try_acquire` non-blocking; returns `Busy` with
//!     the current owner PID when lock is held elsewhere.
//!   - Lock file at `<tasks_dir>/<TASK_ID>/.task.lock`. Content = owner PID
//!     (single line). The file persists across acquisitions — flock is the
//!     mutex, the file is just an inode to attach it to.
//!   - Held for the lifetime of the `TaskLockGuard`. Dropping the guard
//!     releases the lock. Process death also releases (kernel cleanup).

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Held while a process owns the lock. Drop releases.
#[derive(Debug)]
pub struct TaskLockGuard {
    file: File,
    #[allow(dead_code)]
    path: PathBuf,
}

impl Drop for TaskLockGuard {
    fn drop(&mut self) {
        // Best-effort unlock. The fd close on `File` drop also releases the
        // flock, so a failure here is harmless.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Why an acquire failed. `Busy` carries the owner PID when readable from
/// the lock file (newly created files race; treat unreadable as `None`).
#[derive(Debug)]
pub enum TaskLockError {
    Busy {
        task_id: String,
        owner_pid: Option<u32>,
    },
    Io(anyhow::Error),
}

impl std::fmt::Display for TaskLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskLockError::Busy {
                task_id,
                owner_pid: Some(pid),
            } => write!(
                f,
                "task {task_id} is locked by another zforge process (pid {pid}). \
                 If that process is gone, retry; flock auto-releases on exit."
            ),
            TaskLockError::Busy {
                task_id,
                owner_pid: None,
            } => write!(
                f,
                "task {task_id} is locked by another zforge process. \
                 If you're sure it crashed, retry — flock auto-releases on exit."
            ),
            TaskLockError::Io(e) => write!(f, "task-lock I/O error: {e}"),
        }
    }
}

impl std::error::Error for TaskLockError {}

/// Non-blocking acquire. Returns `Busy` immediately when another process
/// holds the lock — caller decides whether to retry or exit.
pub fn try_acquire(tasks_dir: &Path, task_id: &str) -> Result<TaskLockGuard, TaskLockError> {
    let dir = tasks_dir.join(task_id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(TaskLockError::Io(
            anyhow::Error::from(e).context(format!("create task dir {dir:?}")),
        ));
    }
    let path = dir.join(".task.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| TaskLockError::Io(anyhow::Error::from(e).context(format!("open {path:?}"))))?;

    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            // Lock held — write our PID for diagnostics. Failure here is
            // not fatal; the lock semantics come from flock, not the file
            // content.
            let _ = file.set_len(0);
            let _ = writeln!(&file, "{}", std::process::id());
            Ok(TaskLockGuard { file, path })
        }
        Err(_) => {
            let owner_pid = read_pid(&path);
            Err(TaskLockError::Busy {
                task_id: task_id.to_string(),
                owner_pid,
            })
        }
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
    fn acquire_succeeds_when_lock_free() {
        let tmp = TempDir::new().unwrap();
        let guard = try_acquire(tmp.path(), "T1").expect("first acquire");
        drop(guard);
    }

    #[test]
    fn second_acquire_returns_busy_while_first_held() {
        let tmp = TempDir::new().unwrap();
        let guard = try_acquire(tmp.path(), "T1").expect("first acquire");

        match try_acquire(tmp.path(), "T1") {
            Err(TaskLockError::Busy { task_id, owner_pid }) => {
                assert_eq!(task_id, "T1");
                assert_eq!(owner_pid, Some(std::process::id()));
            }
            other => panic!("expected Busy, got {other:?}"),
        }

        drop(guard);
        // After release, acquire succeeds again.
        let _g2 = try_acquire(tmp.path(), "T1").expect("re-acquire after drop");
    }

    #[test]
    fn different_tasks_dont_block_each_other() {
        let tmp = TempDir::new().unwrap();
        let _g1 = try_acquire(tmp.path(), "T1").expect("T1");
        let _g2 = try_acquire(tmp.path(), "T2").expect("T2 should not block on T1");
    }

    #[test]
    fn lock_file_persists_after_release() {
        let tmp = TempDir::new().unwrap();
        let guard = try_acquire(tmp.path(), "T1").expect("acquire");
        drop(guard);
        let lock_path = tmp.path().join("T1").join(".task.lock");
        assert!(
            lock_path.exists(),
            "lock file kept around — inode reused for next holder"
        );
    }

    #[test]
    fn busy_error_displays_helpful_message() {
        let tmp = TempDir::new().unwrap();
        let _guard = try_acquire(tmp.path(), "T1").unwrap();
        let err = try_acquire(tmp.path(), "T1").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("T1"));
        assert!(msg.contains("flock"));
    }
}
