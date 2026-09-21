#![cfg(unix)]
//! FIX-006 end to end: timeouts, normal exits, cancel and Ctrl-C must each
//! bound zforge's wait *and* stop every process the child started.
//!
//! Each case plants a grandchild that writes its pid to a file, then asserts
//! both the wall time and that the grandchild is gone afterwards. Driven
//! through the real binary: the bug is about process trees and signals,
//! which only a real process boundary exercises.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn zforge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_zforge")
}

fn pid_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes for existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Orphans are reparented to init/launchd and reaped asynchronously.
fn assert_dies_within(pid: i32, limit: Duration) {
    let deadline = Instant::now() + limit;
    while pid_alive(pid) {
        if Instant::now() >= deadline {
            // Do not leak it into the rest of the run.
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("grandchild {pid} still running after {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn read_pid(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn wait_for_pid(path: &Path) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(pid) = read_pid(path) {
            return pid;
        }
        assert!(Instant::now() < deadline, "{path:?} never written");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A shell script that backgrounds a long sleep holding our pipes, records
/// its pid, and waits. `ignore_term` makes the grandchild immune to SIGTERM
/// (SIG_IGN survives fork+exec), so only SIGKILL stops it.
fn grandchild_script(pidfile: &Path, ignore_term: bool) -> String {
    let trap = if ignore_term { "trap '' TERM; " } else { "" };
    format!("{trap}sleep 60 & echo $! > {}; wait", pidfile.display())
}

fn write_script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("run.sh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

struct Project {
    root: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Project {
    fn new(test_command: &str, state: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let r = root.path();
        std::fs::create_dir_all(r.join(".zforge/tasks/T1")).unwrap();
        std::fs::create_dir_all(r.join(".zforge/agents")).unwrap();
        std::fs::write(
            r.join(".zforge/agents/code.tmpl"),
            "Implement {{task_id}}\n",
        )
        .unwrap();
        std::fs::write(
            r.join(".zforge/agents/verify-analysis.tmpl"),
            "Analyze {{task_id}}\n",
        )
        .unwrap();
        std::fs::write(
            r.join(".zforge/config.yaml"),
            format!(
                r#"project:
  name: "test"
  language: "shell"
  test_command: "{test_command}"
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
"#
            ),
        )
        .unwrap();
        for name in ["task.md", "spec.md", "testspec.md", "plan.md"] {
            std::fs::write(r.join(".zforge/tasks/T1").join(name), "# stub\n").unwrap();
        }
        std::fs::write(
            r.join(".zforge/tasks/T1/.state.yaml"),
            format!(
                "task_id: T1\nflow: Full\nstate: {state}\nupdated_at: \"2026-01-01T00:00:00+00:00\"\n\
                 history:\n  - state: {state}\n    at: \"2026-01-01T00:00:00+00:00\"\n    note: fixture\n\
                 assigned_agent: slow\nactive_agent: slow\n"
            ),
        )
        .unwrap();
        Self { root, home }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(zforge_bin());
        c.args(args)
            .current_dir(self.root.path())
            .env("ZFORGE_HOME", self.home.path())
            .env("ZFORGE_WORKER_BIN", zforge_bin())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    /// Register an agent named `slow` that runs `script`.
    fn register_agent(&self, script: &Path) {
        std::fs::write(
            self.home.path().join("registry.yaml"),
            format!(
                "agents:\n  slow:\n    command: {}\n    args: []\nfallback_policy:\n  max_retries: 0\n  cooldown_seconds: 0\n",
                script.display()
            ),
        )
        .unwrap();
    }

    fn job_id(&self) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        let jobs = self.root.path().join(".zforge/jobs");
        loop {
            if let Some(id) = std::fs::read_dir(&jobs)
                .ok()
                .and_then(|mut d| d.next())
                .and_then(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
            {
                return id;
            }
            assert!(Instant::now() < deadline, "no job created");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn wait_with_limit(mut child: Child, limit: Duration) -> std::process::Output {
    let deadline = Instant::now() + limit;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("zforge did not return within {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}

const EXIT_TIMEOUT: i32 = 124;

// ─── test runner ─────────────────────────────────────────────────────────────

/// The backlog repro, on `verify`: timeout 1s, a grandchild holding the
/// pipes for 60s. Used to wait for the grandchild and leave it running.
#[test]
fn verify_timeout_is_bounded_and_kills_the_test_process_tree() {
    let scratch = tempfile::tempdir().unwrap();
    let pidfile = scratch.path().join("gc.pid");
    let script = write_script(scratch.path(), &grandchild_script(&pidfile, false));
    let p = Project::new(&script.display().to_string(), "Coded");

    // On a loaded runner the timeout can fire before the script has even
    // recorded its grandchild. Such a run proves nothing about the tree
    // kill, so it is retried with a longer budget instead of failing — the
    // pidfile is read only after zforge exits, never raced against it.
    for timeout_secs in [3u64, 6, 12] {
        let _ = std::fs::remove_file(&pidfile);
        let started = Instant::now();
        let child = p
            .cmd(&["verify", "T1", "--timeout", &timeout_secs.to_string()])
            .spawn()
            .unwrap();
        let out = wait_with_limit(child, Duration::from_secs(30 + timeout_secs));
        let elapsed = started.elapsed();

        assert_eq!(
            out.status.code(),
            Some(EXIT_TIMEOUT),
            "a timed-out suite is a Timeout, not a Failed run: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // timeout + KILL_GRACE (2s) + 2 x DRAIN_GRACE (1s) worst case,
        // plus 2s slack for process start-up.
        let bound = Duration::from_secs(timeout_secs + 6);
        assert!(elapsed < bound, "waited {elapsed:?} (bound {bound:?})");

        let Some(grandchild) = read_pid(&pidfile) else {
            continue;
        };
        assert_dies_within(grandchild, Duration::from_secs(3));
        return;
    }
    panic!("the test script never recorded its grandchild, even with a 12s budget");
}

/// Normal exit that leaves a background process holding the pipes.
#[test]
fn verify_does_not_wait_for_a_leftover_background_process() {
    let scratch = tempfile::tempdir().unwrap();
    let pidfile = scratch.path().join("gc.pid");
    let script = write_script(
        scratch.path(),
        &format!("sleep 60 & echo $! > {}; exit 0", pidfile.display()),
    );
    let p = Project::new(&script.display().to_string(), "Coded");

    let started = Instant::now();
    let child = p.cmd(&["verify", "T1", "--timeout", "60"]).spawn().unwrap();
    let grandchild = wait_for_pid(&pidfile);
    let out = wait_with_limit(child, Duration::from_secs(30));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "waited {:?}",
        started.elapsed()
    );
    assert_dies_within(grandchild, Duration::from_secs(3));
}

// ─── signals ─────────────────────────────────────────────────────────────────

/// Children now run in their own process group, so a terminal Ctrl-C (sent
/// to zforge's group) no longer reaches them directly. zforge must forward
/// it and still die of the signal itself.
#[test]
fn sigint_to_zforge_reaches_the_test_process_tree() {
    let scratch = tempfile::tempdir().unwrap();
    let pidfile = scratch.path().join("gc.pid");
    let script = write_script(scratch.path(), &grandchild_script(&pidfile, false));
    let p = Project::new(&script.display().to_string(), "Coded");

    let child = p
        .cmd(&["verify", "T1", "--timeout", "600"])
        .spawn()
        .unwrap();
    let grandchild = wait_for_pid(&pidfile);

    // SAFETY: signalling a child process we spawned.
    unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
    let out = wait_with_limit(child, Duration::from_secs(10));

    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        out.status.signal(),
        Some(libc::SIGINT),
        "zforge should still terminate by SIGINT as it did before"
    );
    assert_dies_within(grandchild, Duration::from_secs(3));
}

// ─── cancel ──────────────────────────────────────────────────────────────────

fn start_async_ship_with_agent(ignore_term: bool) -> (Project, tempfile::TempDir, i32, String) {
    let scratch = tempfile::tempdir().unwrap();
    let pidfile = scratch.path().join("gc.pid");
    let agent = write_script(scratch.path(), &grandchild_script(&pidfile, ignore_term));
    let p = Project::new("sh -c 'exit 0'", "PlanReviewed");
    p.register_agent(&agent);

    let out = p.cmd(&["ship", "T1", "--async"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let job = p.job_id();
    let grandchild = wait_for_pid(&pidfile);
    (p, scratch, grandchild, job)
}

fn job_status(p: &Project, job: &str) -> String {
    let yaml = std::fs::read_to_string(
        p.root
            .path()
            .join(".zforge/jobs")
            .join(job)
            .join("job.yaml"),
    )
    .unwrap();
    yaml.lines()
        .find_map(|l| l.strip_prefix("status:").map(|s| s.trim().to_string()))
        .unwrap_or_default()
}

/// `job cancel` signals the worker's group; the agent's tree lives in its
/// own group and must be stopped too.
#[test]
fn job_cancel_stops_the_agent_process_tree() {
    let (p, _scratch, grandchild, job) = start_async_ship_with_agent(false);

    let out = wait_with_limit(
        p.cmd(&["job", "cancel", &job]).spawn().unwrap(),
        Duration::from_secs(15),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(job_status(&p, &job), "cancelled");
    assert_dies_within(grandchild, Duration::from_secs(5));
}

/// A grandchild that ignores SIGTERM survives the worker's forwarding; cancel
/// has to escalate to SIGKILL on the recorded child groups.
#[test]
fn job_cancel_escalates_to_sigkill_on_child_groups() {
    let (p, _scratch, grandchild, job) = start_async_ship_with_agent(true);

    let out = wait_with_limit(
        p.cmd(&["job", "cancel", &job]).spawn().unwrap(),
        Duration::from_secs(20),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_dies_within(grandchild, Duration::from_secs(5));
}
