//! End-to-end test of per-task locking.
//!
//! Verifies that two concurrent zforge processes cannot operate on the same
//! task simultaneously, regardless of whether they are foreground or async.

mod common;

use common::{make_project, TestHome};
use serial_test::serial;
use zforge::state::{try_acquire_task_lock, TaskLockError};

#[test]
#[serial]
fn second_acquire_within_same_process_returns_busy() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    let tasks_dir = proj.path().join(".zforge/tasks");

    let _g1 = try_acquire_task_lock(&tasks_dir, "T1").expect("first acquire");
    match try_acquire_task_lock(&tasks_dir, "T1") {
        Err(TaskLockError::Busy { task_id, owner_pid }) => {
            assert_eq!(task_id, "T1");
            assert_eq!(owner_pid, Some(std::process::id()));
        }
        other => panic!("expected Busy, got {other:?}"),
    }
}

#[test]
#[serial]
fn dropping_guard_releases_lock() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    let tasks_dir = proj.path().join(".zforge/tasks");

    {
        let _g = try_acquire_task_lock(&tasks_dir, "T1").expect("first");
    }
    let _g2 = try_acquire_task_lock(&tasks_dir, "T1").expect("re-acquire after drop");
}

#[test]
#[serial]
fn different_tasks_dont_block() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    let tasks_dir = proj.path().join(".zforge/tasks");

    let _a = try_acquire_task_lock(&tasks_dir, "T-A").expect("T-A");
    let _b = try_acquire_task_lock(&tasks_dir, "T-B").expect("T-B");
}

#[test]
#[serial]
fn lock_file_records_owner_pid() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    let tasks_dir = proj.path().join(".zforge/tasks");

    let _g = try_acquire_task_lock(&tasks_dir, "T1").expect("acquire");
    let lock_path = tasks_dir.join("T1").join(".task.lock");
    let content = std::fs::read_to_string(&lock_path).unwrap();
    let pid: u32 = content.trim().parse().unwrap();
    assert_eq!(pid, std::process::id());
}
