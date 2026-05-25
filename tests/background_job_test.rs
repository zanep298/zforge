//! End-to-end test of the `ship --async` background job runtime.
//!
//! Spawns the real `zforge` binary (via `CARGO_BIN_EXE_zforge` →
//! `ZFORGE_WORKER_BIN`) as a detached worker, then asserts the controller
//! returns immediately, the worker progresses through `running` → terminal,
//! and the job state file reflects the outcome.

mod common;

use common::{make_project, TestHome};
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zforge::job::lifecycle::reconcile_dead_worker;
use zforge::job::schema::JobStatus;
use zforge::job::store::{load_job, job_log_path};
use zforge::registry::{
    io,
    schema::{AgentSpec, FallbackPolicy, Registry},
};
use zforge::state::{Flow, State, TaskState};

struct CwdGuard {
    prev: PathBuf,
}

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self { prev }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev);
    }
}

fn fake_agent_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("examples")
        .join("fake_agent")
}

fn ensure_fake_agent_built() {
    if fake_agent_path().exists() {
        return;
    }
    let status = std::process::Command::new("cargo")
        .args(["build", "--example", "fake_agent"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("cargo build --example fake_agent");
    assert!(status.success());
}

fn set_worker_binary() {
    // CARGO_BIN_EXE_<name> is provided by cargo when running integration tests.
    let path = env!("CARGO_BIN_EXE_zforge");
    std::env::set_var("ZFORGE_WORKER_BIN", path);
}

fn write_fake_config(dir: &Path, name: &str, cfg: &serde_json::Value) -> PathBuf {
    let p = dir.join(format!("{name}.json"));
    std::fs::write(&p, serde_json::to_string(cfg).unwrap()).unwrap();
    p
}

fn write_config(project: &Path, test_command: &str) {
    let cfg = format!(
        r#"project:
  name: "test"
  language: "shell"
  test_command: "{cmd}"
  root_dir: "."
opencode:
  model: "claude-sonnet-4-6"
  context_files: []
paths:
  tasks: "./.zforge/tasks"
  agents: "./.zforge/agents"
  memory: "./.zforge/memory"
  skills: "./.zforge/skills"
review:
  auto_approve: false
"#,
        cmd = test_command.replace('"', "\\\""),
    );
    std::fs::write(project.join(".zforge/config.yaml"), cfg).unwrap();
}

fn write_min_agents_dir(project: &Path) {
    let dir = project.join(".zforge/agents");
    std::fs::create_dir_all(&dir).unwrap();
    let tmpl = "Implementing {{task_id}} via {{test_command}}\n";
    std::fs::write(dir.join("code.tmpl"), tmpl).unwrap();
}

fn seed_registry_with_fake_agent(extra_args: &[&str]) {
    let mut r = Registry {
        fallback_policy: FallbackPolicy {
            max_retries: 0,
            cooldown_seconds: 0,
            ..FallbackPolicy::default()
        },
        ..Default::default()
    };
    r.agents.insert(
        "primary".into(),
        AgentSpec {
            command: fake_agent_path().to_string_lossy().into_owned(),
            args: extra_args.iter().map(|s| s.to_string()).collect(),
        },
    );
    io::save_atomic(&r).unwrap();
}

fn make_task_ready_to_ship(project: &Path, task_id: &str) {
    let tasks_dir = project.join(".zforge/tasks");
    std::fs::create_dir_all(tasks_dir.join(task_id)).unwrap();
    for name in ["task.md", "spec.md", "testspec.md", "plan.md"] {
        std::fs::write(
            tasks_dir.join(task_id).join(name),
            format!("# {name}\nstub\n"),
        )
        .unwrap();
    }
    let mut ts = TaskState::new_with_flow(task_id, Flow::Full);
    ts.assigned_agent = Some("primary".into());
    ts.active_agent = Some("primary".into());
    for next in [
        State::SpecDone,
        State::TestspecDone,
        State::TestspecReviewed,
        State::Planned,
        State::PlanReviewed,
    ] {
        ts.advance(next, "test").unwrap();
    }
    ts.save(&tasks_dir).unwrap();
}

fn wait_terminal(project: &Path, job_id: &str, timeout: Duration) -> JobStatus {
    let config = zforge::config::load_from(&project.join(".zforge/config.yaml")).unwrap();
    let deadline = Instant::now() + timeout;
    loop {
        reconcile_dead_worker(&config, job_id).ok();
        let job = load_job(&config, job_id).unwrap();
        if job.status.is_terminal() {
            return job.status;
        }
        if Instant::now() >= deadline {
            panic!(
                "job {job_id} did not reach terminal in {:?}: {}",
                timeout,
                job.status.as_str()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
#[serial]
fn async_ship_spawns_worker_and_reaches_success() {
    ensure_fake_agent_built();
    set_worker_binary();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());
    write_config(proj.path(), "sh -c 'true'");

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    // Spawn job. Controller must return ~immediately.
    let started = Instant::now();
    zforge::cli::ship::run_async("T1", None, 60, 1).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "controller blocked: {:?}",
        started.elapsed()
    );

    // Find the job that was just created.
    let config = zforge::config::load_from(&proj.path().join(".zforge/config.yaml")).unwrap();
    let jobs = zforge::job::store::list_jobs(&config).unwrap();
    assert_eq!(jobs.len(), 1, "expected exactly one job");
    let job_id = jobs[0].job_id.clone();

    let final_status = wait_terminal(proj.path(), &job_id, Duration::from_secs(30));
    assert_eq!(final_status, JobStatus::Success);

    let job = load_job(&config, &job_id).unwrap();
    assert!(job.started_at.is_some());
    assert!(job.finished_at.is_some());
    assert_eq!(job.exit_code, Some(0));
}

#[test]
#[serial]
fn async_ship_marks_failed_when_tests_fail() {
    ensure_fake_agent_built();
    set_worker_binary();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());
    // single-shot ship returns Ok even on verify fail (state stays at Coded)
    // so use max_iterations=2 to force the verifier loop, which DOES bail
    // when budget exhausts.
    write_config(proj.path(), "sh -c 'false'");

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    zforge::cli::ship::run_async("T1", None, 60, 2).unwrap();

    let config = zforge::config::load_from(&proj.path().join(".zforge/config.yaml")).unwrap();
    let jobs = zforge::job::store::list_jobs(&config).unwrap();
    let job_id = jobs[0].job_id.clone();

    let final_status = wait_terminal(proj.path(), &job_id, Duration::from_secs(30));
    assert_eq!(final_status, JobStatus::Failed);

    let job = load_job(&config, &job_id).unwrap();
    assert!(
        job.error.as_deref().unwrap_or("").contains("budget exhausted"),
        "error: {:?}",
        job.error
    );
}

#[test]
#[serial]
fn list_jobs_returns_newest_first() {
    ensure_fake_agent_built();
    set_worker_binary();
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());
    write_config(proj.path(), "sh -c 'true'");

    let cfg_dir = tempfile::tempdir().unwrap();
    let agent_cfg = write_fake_config(cfg_dir.path(), "primary", &json!({"exit_code": 0}));
    seed_registry_with_fake_agent(&[agent_cfg.to_str().unwrap()]);

    let _g = CwdGuard::enter(proj.path());
    make_task_ready_to_ship(proj.path(), "T1");

    // Create job twice (second after small gap so created_at differs).
    let config = zforge::config::load_from(&proj.path().join(".zforge/config.yaml")).unwrap();
    let job1 = zforge::job::store::create_job(
        &config,
        "T1",
        zforge::job::schema::JobKind::Ship,
        None,
        60,
        1,
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    let job2 = zforge::job::store::create_job(
        &config,
        "T1",
        zforge::job::schema::JobKind::Ship,
        None,
        60,
        1,
    )
    .unwrap();

    let jobs = zforge::job::store::list_jobs(&config).unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].job_id, job2.job_id, "newest first");
    assert_eq!(jobs[1].job_id, job1.job_id);
}

#[test]
#[serial]
fn dead_worker_detected_and_flipped_to_failed() {
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());
    write_config(proj.path(), "sh -c 'true'");

    let config = zforge::config::load_from(&proj.path().join(".zforge/config.yaml")).unwrap();
    let mut job = zforge::job::store::create_job(
        &config,
        "T1",
        zforge::job::schema::JobKind::Ship,
        None,
        60,
        1,
    )
    .unwrap();

    // Forge a "running" state with a guaranteed-dead PID. PIDs > 2^31 never
    // exist on Unix; kill(pid, 0) returns ESRCH.
    job.status = JobStatus::Running;
    job.worker_pid = Some(0x7fff_fffe);
    zforge::job::store::save_atomic(&config, &job).unwrap();

    let status = reconcile_dead_worker(&config, &job.job_id).unwrap();
    assert_eq!(status, JobStatus::Failed);

    let reloaded = load_job(&config, &job.job_id).unwrap();
    assert_eq!(reloaded.status, JobStatus::Failed);
    assert!(reloaded
        .error
        .as_deref()
        .unwrap_or("")
        .contains("worker process died"));
}

#[test]
#[serial]
fn log_file_exists_after_job_created() {
    let _h = TestHome::new();

    let proj = tempfile::tempdir().unwrap();
    make_project(proj.path());
    write_min_agents_dir(proj.path());
    write_config(proj.path(), "sh -c 'true'");

    let config = zforge::config::load_from(&proj.path().join(".zforge/config.yaml")).unwrap();
    let job = zforge::job::store::create_job(
        &config,
        "T1",
        zforge::job::schema::JobKind::Ship,
        None,
        60,
        1,
    )
    .unwrap();

    let log = job_log_path(&config, &job.job_id);
    assert!(log.exists(), "log file should be pre-created");
}
