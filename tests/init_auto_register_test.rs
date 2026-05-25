mod common;

use common::{make_project, TestHome};
use serial_test::serial;
use zforge::registry::auto::{auto_register, AutoResult};
use zforge::registry::io;

#[test]
#[serial]
fn init_in_fresh_dir_registers() {
    let h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    let res = auto_register(proj.path(), None, false).unwrap();
    match res {
        AutoResult::Registered { name } => {
            assert!(!name.is_empty());
        }
        other => panic!("expected Registered, got {other:?}"),
    }

    assert!(h.registry_path().exists());
    let r = io::load().unwrap();
    assert_eq!(r.projects.len(), 1);
}

#[test]
#[serial]
fn init_in_same_dir_is_noop() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    let first = auto_register(proj.path(), Some("myproj"), false).unwrap();
    assert!(matches!(first, AutoResult::Registered { .. }));

    let second = auto_register(proj.path(), Some("myproj"), false).unwrap();
    match second {
        AutoResult::AlreadyExists { name } => assert_eq!(name, "myproj"),
        other => panic!("expected AlreadyExists, got {other:?}"),
    }

    let r = io::load().unwrap();
    assert_eq!(r.projects.len(), 1);
}

#[test]
#[serial]
fn init_same_name_diff_path_suffixes() {
    let _h = TestHome::new();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    make_project(a.path());
    make_project(b.path());

    auto_register(a.path(), Some("foo"), false).unwrap();
    let res = auto_register(b.path(), Some("foo"), false).unwrap();
    match res {
        AutoResult::Suffixed {
            requested,
            final_name,
        } => {
            assert_eq!(requested, "foo");
            assert_eq!(final_name, "foo-2");
        }
        other => panic!("expected Suffixed, got {other:?}"),
    }
}

#[test]
#[serial]
fn init_same_path_diff_name_updates() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    auto_register(proj.path(), Some("a"), false).unwrap();
    let before = io::load().unwrap().projects[0].registered_at;

    let res = auto_register(proj.path(), Some("b"), false).unwrap();
    match res {
        AutoResult::Updated { old_name, new_name } => {
            assert_eq!(old_name, "a");
            assert_eq!(new_name, "b");
        }
        other => panic!("expected Updated, got {other:?}"),
    }
    let after = io::load().unwrap().projects[0].registered_at;
    assert_eq!(before, after, "registered_at should be preserved");
}

#[test]
#[serial]
fn switch_flag_sets_current_project() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    auto_register(proj.path(), Some("foo"), true).unwrap();
    let r = io::load().unwrap();
    assert_eq!(r.current_project.as_deref(), Some("foo"));
}

#[test]
#[serial]
fn first_registration_sets_current_project_even_without_switch() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    auto_register(proj.path(), Some("foo"), false).unwrap();
    let r = io::load().unwrap();
    assert_eq!(r.current_project.as_deref(), Some("foo"));
}

#[test]
#[serial]
fn second_registration_without_switch_keeps_current() {
    let _h = TestHome::new();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    make_project(a.path());
    make_project(b.path());

    auto_register(a.path(), Some("a"), false).unwrap();
    auto_register(b.path(), Some("b"), false).unwrap();
    let r = io::load().unwrap();
    assert_eq!(r.current_project.as_deref(), Some("a"));
}

#[test]
#[serial]
fn init_without_zforge_dir_errors() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    // No .zforge subdir created.

    let err = auto_register(proj.path(), Some("foo"), false).unwrap_err();
    assert!(err.to_string().contains(".zforge"));
}

#[test]
#[serial]
fn invalid_name_override_is_rejected() {
    let _h = TestHome::new();
    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());

    let err = auto_register(proj.path(), Some("bad name!"), false).unwrap_err();
    assert!(err.to_string().contains("invalid project name"));
}
