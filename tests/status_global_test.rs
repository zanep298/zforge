mod common;

use common::{make_project, TestHome};
use serial_test::serial;
use std::path::Path;
use zforge::cli::status;
use zforge::registry::auto::auto_register;
use zforge::state::{Flow, State, TaskState};

fn write_task(tasks_dir: &Path, id: &str, target: State) {
    let mut ts = TaskState::new_with_flow(id, Flow::Full);
    let path = [
        State::SpecDone,
        State::TestspecDone,
        State::TestspecReviewed,
        State::Planned,
        State::PlanReviewed,
        State::Coded,
        State::Verified,
        State::Reviewed,
    ];
    for step in path {
        if ts.state >= target {
            break;
        }
        ts.advance(step, "test").unwrap();
    }
    ts.save(tasks_dir).unwrap();
}

#[test]
#[serial]
fn empty_registry_returns_empty() {
    let _h = TestHome::new();
    let (rows, warnings) = status::collect_global(2000).unwrap();
    assert!(rows.is_empty());
    assert!(warnings.is_empty());
}

#[test]
#[serial]
fn aggregates_tasks_across_projects() {
    let _h = TestHome::new();

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    make_project(a.path());
    make_project(b.path());

    write_task(&a.path().join(".zforge/tasks"), "TASK-A", State::Planned);
    write_task(&b.path().join(".zforge/tasks"), "TASK-B", State::SpecDone);

    auto_register(a.path(), Some("alpha"), false).unwrap();
    auto_register(b.path(), Some("beta"), false).unwrap();

    let (rows, _warnings) = status::collect_global(2000).unwrap();
    assert_eq!(rows.len(), 2);
    let ids: Vec<&str> = rows.iter().map(|r| r.task_id.as_str()).collect();
    assert!(ids.contains(&"TASK-A"));
    assert!(ids.contains(&"TASK-B"));
}

#[test]
#[serial]
fn skips_malformed_state_yaml() {
    let _h = TestHome::new();

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    make_project(a.path());
    make_project(b.path());

    // a has a malformed state file
    let bad_dir = a.path().join(".zforge/tasks/BAD-1");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join(".state.yaml"), "not: valid: yaml: [").unwrap();

    write_task(&b.path().join(".zforge/tasks"), "TASK-B", State::Planned);

    auto_register(a.path(), Some("alpha"), false).unwrap();
    auto_register(b.path(), Some("beta"), false).unwrap();

    let (rows, _warnings) = status::collect_global(2000).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.task_id.as_str()).collect();
    assert!(!ids.contains(&"BAD-1"));
    assert!(ids.contains(&"TASK-B"));
}

#[test]
#[serial]
fn skips_missing_path() {
    let _h = TestHome::new();

    let a = tempfile::tempdir().unwrap();
    make_project(a.path());
    auto_register(a.path(), Some("alpha"), false).unwrap();

    // Drop the directory after registration.
    let _ = std::fs::remove_dir_all(a.path());

    let (rows, _warnings) = status::collect_global(2000).unwrap();
    assert!(rows.is_empty());
}

#[test]
#[serial]
fn terminal_tasks_excluded() {
    let _h = TestHome::new();

    let a = tempfile::tempdir().unwrap();
    make_project(a.path());

    write_task(
        &a.path().join(".zforge/tasks"),
        "TASK-DONE",
        State::Reviewed,
    );
    write_task(&a.path().join(".zforge/tasks"), "TASK-LIVE", State::Planned);

    auto_register(a.path(), Some("alpha"), false).unwrap();

    let (rows, _warnings) = status::collect_global(2000).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.task_id.as_str()).collect();
    assert!(ids.contains(&"TASK-LIVE"));
    assert!(!ids.contains(&"TASK-DONE"));
}
