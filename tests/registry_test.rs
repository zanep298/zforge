mod common;

use common::TestHome;
use serial_test::serial;
use std::path::PathBuf;
use zforge::registry::{io, lock, schema::*, validate};

#[test]
#[serial]
fn roundtrip_default_registry() {
    let r = Registry::default();
    let y = serde_yaml::to_string(&r).unwrap();
    let back: Registry = serde_yaml::from_str(&y).unwrap();
    assert_eq!(back.projects.len(), 0);
    assert_eq!(
        back.fallback_policy.max_retries,
        r.fallback_policy.max_retries
    );
}

#[test]
#[serial]
fn roundtrip_full_registry() {
    let _h = TestHome::new();
    let mut r = Registry {
        current_project: Some("foo".into()),
        ..Default::default()
    };
    r.projects.push(ProjectEntry {
        name: "foo".into(),
        path: PathBuf::from("/tmp/foo"),
        registered_at: chrono::Utc::now(),
        registered_by: RegisteredBy::Init,
        agent_overrides: Default::default(),
    });
    r.agents.insert(
        "claude".into(),
        AgentSpec {
            command: "claude".into(),
            args: vec!["--print".into()],
        },
    );

    let y = serde_yaml::to_string(&r).unwrap();
    let back: Registry = serde_yaml::from_str(&y).unwrap();
    assert_eq!(back.current_project.as_deref(), Some("foo"));
    assert_eq!(back.projects.len(), 1);
    assert_eq!(back.projects[0].name, "foo");
    assert_eq!(back.agents.get("claude").unwrap().command, "claude");
}

#[test]
#[serial]
fn old_registry_without_fallback_policy_loads() {
    let y = "projects: []\nagents: {}\n";
    let r: Registry = serde_yaml::from_str(y).unwrap();
    assert_eq!(r.fallback_policy.max_retries, 2);
    assert_eq!(r.fallback_policy.cooldown_seconds, 30);
}

#[test]
#[serial]
fn save_atomic_writes_then_loads() {
    let _h = TestHome::new();
    let r = Registry {
        current_project: Some("alpha".into()),
        ..Default::default()
    };
    io::save_atomic(&r).unwrap();

    let loaded = io::load().unwrap();
    assert_eq!(loaded.current_project.as_deref(), Some("alpha"));
}

#[test]
#[serial]
fn load_returns_default_when_file_missing() {
    let _h = TestHome::new();
    let r = io::load().unwrap();
    assert!(r.projects.is_empty());
    assert!(r.current_project.is_none());
}

#[test]
#[serial]
fn lock_serializes_concurrent_writers() {
    let h = TestHome::new();
    let home_path = h.home.path().to_path_buf();

    // Two threads each append a distinct project under with_lock.
    let t1_home = home_path.clone();
    let t1 = std::thread::spawn(move || {
        std::env::set_var("ZFORGE_HOME", &t1_home);
        lock::with_lock(|| {
            let mut r = io::load()?;
            // Hold the lock briefly to force contention.
            std::thread::sleep(std::time::Duration::from_millis(50));
            r.projects.push(ProjectEntry {
                name: "one".into(),
                path: PathBuf::from("/tmp/one"),
                registered_at: chrono::Utc::now(),
                registered_by: RegisteredBy::Manual,
                agent_overrides: Default::default(),
            });
            io::save_atomic(&r)?;
            Ok(())
        })
        .unwrap();
    });

    let t2_home = home_path.clone();
    let t2 = std::thread::spawn(move || {
        std::env::set_var("ZFORGE_HOME", &t2_home);
        std::thread::sleep(std::time::Duration::from_millis(10));
        lock::with_lock(|| {
            let mut r = io::load()?;
            r.projects.push(ProjectEntry {
                name: "two".into(),
                path: PathBuf::from("/tmp/two"),
                registered_at: chrono::Utc::now(),
                registered_by: RegisteredBy::Manual,
                agent_overrides: Default::default(),
            });
            io::save_atomic(&r)?;
            Ok(())
        })
        .unwrap();
    });

    t1.join().unwrap();
    t2.join().unwrap();

    let r = io::load().unwrap();
    let names: Vec<&str> = r.projects.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"one"), "missing one: {:?}", names);
    assert!(names.contains(&"two"), "missing two: {:?}", names);
}

#[test]
#[serial]
fn name_validation_rejects_bad_inputs() {
    assert!(validate::validate_name("").is_err());
    assert!(validate::validate_name(&"a".repeat(65)).is_err());
    assert!(validate::validate_name("bad name").is_err());
    assert!(validate::validate_name("bad.name").is_err());
    assert!(validate::validate_name("bad/name").is_err());
    assert!(validate::validate_name("good_name-1").is_ok());
}

#[test]
#[serial]
fn next_available_name_finds_first_gap() {
    let mut r = Registry::default();
    r.projects.push(ProjectEntry {
        name: "foo".into(),
        path: PathBuf::from("/tmp/a"),
        registered_at: chrono::Utc::now(),
        registered_by: RegisteredBy::Manual,
        agent_overrides: Default::default(),
    });
    assert_eq!(validate::next_available_name(&r, "foo"), "foo-2");
    r.projects.push(ProjectEntry {
        name: "foo-2".into(),
        path: PathBuf::from("/tmp/b"),
        registered_at: chrono::Utc::now(),
        registered_by: RegisteredBy::Manual,
        agent_overrides: Default::default(),
    });
    assert_eq!(validate::next_available_name(&r, "foo"), "foo-3");
}
