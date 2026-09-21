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
//!   - NOT re-entrant. `flock` is bound to the open file description, so a
//!     second acquire from the same process conflicts with the first exactly
//!     as one from another process would. Code that already owns the lock
//!     passes its guard down (`Some(&guard)`) instead of acquiring again —
//!     see [`with_task_lock`]. Ownership is proven by holding the guard, not
//!     inferred from an environment variable.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Held while a process owns the lock. Drop releases.
#[derive(Debug)]
pub struct TaskLockGuard {
    file: File,
    path: PathBuf,
    task_id: String,
}

impl TaskLockGuard {
    /// Task this guard owns.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// True when this guard is the lock for `task_id` under `tasks_dir`.
    fn covers(&self, tasks_dir: &Path, task_id: &str) -> bool {
        if self.task_id != task_id {
            return false;
        }
        let expected = tasks_dir.join(task_id).join(".task.lock");
        if self.path == expected {
            return true;
        }
        // Same file reached through different spellings (relative vs
        // absolute, symlinked tmp dirs on macOS).
        match (self.path.canonicalize(), expected.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
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
                 If that process is gone, retry; flock auto-releases on exit. {AGENT_HINT}"
            ),
            TaskLockError::Busy {
                task_id,
                owner_pid: None,
            } => write!(
                f,
                "task {task_id} is locked by another zforge process. \
                 If you're sure it crashed, retry — flock auto-releases on exit. {AGENT_HINT}"
            ),
            TaskLockError::Io(e) => write!(f, "task-lock I/O error: {e}"),
        }
    }
}

impl std::error::Error for TaskLockError {}

/// Appended to `Busy` so an agent spawned by `zforge ship` — whose prompt may
/// tell it to run `zforge verify` — understands why that call is refused
/// and what to do instead.
const AGENT_HINT: &str = "If you are an agent launched by that zforge process \
    (e.g. the code agent under `zforge ship`), it runs verification itself — \
    run the project's test command directly instead.";

/// [`try_acquire`] with the error converted for `anyhow` callers.
pub fn lock_task(tasks_dir: &Path, task_id: &str) -> anyhow::Result<TaskLockGuard> {
    try_acquire(tasks_dir, task_id).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Run `f` while owning the lock for `task_id`.
///
/// - `held = Some(guard)`: the caller already owns the lock (e.g. `ship`
///   calling into `verify`, or the orchestrator recording a fallback while
///   `ship` holds the task). The guard is checked to be for this task and
///   passed through; nothing is re-acquired, so there is no self-deadlock.
/// - `held = None`: acquire for the duration of `f`, returning `Busy`
///   without running `f` when another holder exists.
pub fn with_task_lock<T>(
    tasks_dir: &Path,
    task_id: &str,
    held: Option<&TaskLockGuard>,
    f: impl FnOnce(&TaskLockGuard) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    match held {
        Some(guard) => {
            if !guard.covers(tasks_dir, task_id) {
                anyhow::bail!(
                    "internal: lock held for task {} ({}) passed to an operation on task {} ({})",
                    guard.task_id,
                    guard.path.display(),
                    task_id,
                    tasks_dir.join(task_id).join(".task.lock").display(),
                );
            }
            f(guard)
        }
        None => {
            let guard = lock_task(tasks_dir, task_id)?;
            f(&guard)
        }
    }
}

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
            Ok(TaskLockGuard {
                file,
                path,
                task_id: task_id.to_string(),
            })
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
    fn with_task_lock_acquires_when_not_held() {
        let tmp = TempDir::new().unwrap();
        let ran = with_task_lock(tmp.path(), "T1", None, |g| {
            assert_eq!(g.task_id(), "T1");
            // Held while `f` runs.
            assert!(try_acquire(tmp.path(), "T1").is_err());
            Ok(true)
        })
        .unwrap();
        assert!(ran);
        // Released afterwards.
        let _again = try_acquire(tmp.path(), "T1").expect("released after f");
    }

    #[test]
    fn with_task_lock_reuses_a_held_guard_without_reacquiring() {
        let tmp = TempDir::new().unwrap();
        let outer = try_acquire(tmp.path(), "T1").unwrap();
        // A fresh acquire here would be Busy against `outer`; passing the
        // guard down must not attempt one.
        let v = with_task_lock(tmp.path(), "T1", Some(&outer), |_| Ok(7)).unwrap();
        assert_eq!(v, 7);
    }

    #[test]
    fn with_task_lock_does_not_run_f_when_busy() {
        let tmp = TempDir::new().unwrap();
        let _other = try_acquire(tmp.path(), "T1").unwrap();
        let mut ran = false;
        let err = with_task_lock(tmp.path(), "T1", None, |_| {
            ran = true;
            Ok(())
        })
        .unwrap_err();
        assert!(!ran, "f must not run without the lock");
        assert!(err.to_string().contains("locked"));
    }

    #[test]
    fn with_task_lock_rejects_a_guard_for_another_task() {
        let tmp = TempDir::new().unwrap();
        let g1 = try_acquire(tmp.path(), "T1").unwrap();
        let err = with_task_lock(tmp.path(), "T2", Some(&g1), |_| Ok(())).unwrap_err();
        assert!(err.to_string().contains("internal"));
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
