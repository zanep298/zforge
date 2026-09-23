#![cfg(unix)]
//! Mốc B `zforge run` end to end (MOC-B TASK-004).
//!
//! A real git project with a bug (`add` subtracts), an intake accepted and
//! handed over through the library, and the real binary. `claude` is a stub
//! that works in its cwd — fixing `lib.sh` or not — and replays a real Claude
//! Code stream, whose `result` reports $0.20.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/claude/stream_code_agent.jsonl"
);
const BUG: &str = "add() { echo $(( $1 - $2 )); }\n";
const FIX: &str = "add() { echo $(( $1 + $2 )); }\n";

fn task(id: &str, deps: &str) -> String {
    format!(
        "---\nid: {id}\nparent: F\nrequirements: [REQ-001]\ndepends_on: [{deps}]\n---\n\n# {id}\n\n\
         ## Mục tiêu\nadd trả về tổng.\n## Input\nlib.sh.\n## Output\nadd đúng.\n## Ràng buộc\nChỉ sửa lib.sh.\n\
         ## Tự chủ\nTự chọn cách sửa.\n## Acceptance và kiểm chứng\n- AC-01: sh test.sh pass\n## Bàn giao\nLocal.\n\
         ## Cần amendment khi\nPhải sửa test.\n## Câu hỏi còn mở\n"
    )
}

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    marks: PathBuf,
}

impl Project {
    /// `budget` and `iterations` go into the handover's policy.
    fn new(budget: f64, iterations: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("proj");
        let home = dir.path().canonicalize().unwrap().join("zf");
        let marks = dir.path().canonicalize().unwrap().join("marks");
        for d in [&root, &home, &marks] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(root.join("lib.sh"), BUG).unwrap();
        std::fs::write(
            root.join("test.sh"),
            "#!/bin/sh\n. ./lib.sh\n[ \"$(add 2 3)\" = 5 ] || { echo 'FAIL add_small'; exit 1; }\necho ok\n",
        )
        .unwrap();
        std::fs::write(root.join(".gitignore"), ".zforge/\n").unwrap();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            format!(
                "project:\n  name: t\n  language: shell\n  test_command: \"sh test.sh\"\n\
                 execution:\n  budget_usd: {budget}\n  max_iterations: {iterations}\n"
            ),
        )
        .unwrap();
        let p = Self {
            _dir: dir,
            root,
            home,
            marks,
        };
        p.git(&["init", "-q", "-b", "main", "."]);
        p.git(&["add", "-A"]);
        p.git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ]);
        p.hand_over();
        p
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn hand_over(&self) {
        use zforge::intake::{handover, readiness, review};
        let i = review::create(&self.root, "F").unwrap();
        let d = &i.dir;
        std::fs::write(
            d.join("01-outcome.md"),
            "# F\n\n## Yêu cầu\n\n- REQ-001: add trả về tổng\n\n## Câu hỏi còn mở\n",
        )
        .unwrap();
        std::fs::write(
            d.join("02-behavior.md"),
            "# B\n\n## Tình huống\nREQ-001: add 2 3 = 5.\n\n## Câu hỏi còn mở\n",
        )
        .unwrap();
        std::fs::write(
            d.join("03-solution.md"),
            "# S\n\n## Luồng\nSửa phép tính.\n\n## Câu hỏi còn mở\n",
        )
        .unwrap();
        std::fs::write(d.join("04-breakdown.md"), "# K\n\n## Task\nTASK-001, TASK-002\n\n## Kiểm chứng tích hợp\nsh test.sh\n\n## Câu hỏi còn mở\n").unwrap();
        std::fs::write(d.join("tasks/TASK-001.md"), task("TASK-001", "")).unwrap();
        std::fs::write(d.join("tasks/TASK-002.md"), task("TASK-002", "TASK-001")).unwrap();
        for f in i.files() {
            review::review(&i, &f).unwrap();
            review::accept(&i, &f, None).unwrap();
        }
        let config = zforge::config::load_from(&self.root.join(".zforge/config.yaml")).unwrap();
        let r = readiness::check(&i, &self.root, &[], &config.execution).unwrap();
        assert!(r.ready, "{:?}", r.checks);
        handover::create(&i, &self.root, &[], &config, &r.files, None).unwrap();
    }

    /// Register `claude` as a stub: records its cwd, then runs `body`.
    fn stub(&self, body: &str) {
        let script = self.home.join("claude-stub");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat > /dev/null\npwd >> {marks}/cwd\necho \"$@\" >> {marks}/args\n{body}\n",
                marks = self.marks.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            self.home.join("registry.yaml"),
            format!(
                "agents:\n  claude:\n    command: {}\n    args: [\"-p\", \"--output-format\", \"stream-json\", \"--verbose\"]\n\
                 fallback_policy:\n  max_retries: 0\n  cooldown_seconds: 0\n",
                script.display()
            ),
        )
        .unwrap();
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_zforge"));
        c.args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    fn zforge(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn run_dir(&self, id: &str) -> PathBuf {
        self.root.join(".zforge/runs").join(id)
    }

    fn events(&self, id: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.run_dir(id).join("events.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn count(&self, id: &str, event: &str) -> usize {
        self.events(id)
            .iter()
            .filter(|e| e["event"] == event)
            .count()
    }

    fn last(&self, id: &str) -> serde_json::Value {
        self.events(id).last().cloned().unwrap()
    }
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn fix_and_report() -> String {
    format!(
        "printf '{}' > lib.sh\ncat {FIXTURE}",
        FIX.trim_end().replace('%', "%%") + "\\n"
    )
}

/// AC-01 and AC-04.
#[test]
fn a_run_fixes_the_task_in_its_worktree_and_is_verified() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());
    let status_before = p.git(&["status", "--porcelain"]);

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("RUN-001 verified"));

    let worktree = p.root.join(".zforge/worktrees/RUN-001");
    let last = p.last("RUN-001");
    assert_eq!(last["event"], "verified");
    let candidate = zforge::evidence::fingerprint(&worktree);
    assert_eq!(
        last["candidate"].as_str(),
        candidate.hash(),
        "evidence is the worktree's tree"
    );
    assert_eq!(p.count("RUN-001", "attempt"), 1);
    let attempt = p
        .events("RUN-001")
        .into_iter()
        .find(|e| e["event"] == "attempt")
        .unwrap();
    assert!((attempt["cost_usd"].as_f64().unwrap() - 0.2003).abs() < 0.001);

    // The work happened in the worktree, not in the user's checkout.
    assert_eq!(
        std::fs::read_to_string(worktree.join("lib.sh")).unwrap(),
        FIX
    );
    assert_eq!(std::fs::read_to_string(p.root.join("lib.sh")).unwrap(), BUG);
    assert_eq!(
        std::fs::read_to_string(p.marks.join("cwd")).unwrap().trim(),
        worktree.display().to_string()
    );
    assert_eq!(
        p.git(&["status", "--porcelain"]),
        status_before,
        "checkout unchanged"
    );
    assert_eq!(p.git(&["branch", "--show-current"]).trim(), "main");
    assert!(p
        .git(&["branch", "--list", "zforge/TASK-001/RUN-001"])
        .contains("RUN-001"));

    // AC-04: the agent got the budget, headless, in the worktree.
    let args = std::fs::read_to_string(p.marks.join("args")).unwrap();
    assert!(args.contains("--max-budget-usd 3.00"), "{args}");
    assert!(args.contains("--dangerously-skip-permissions"), "{args}");
    let trace: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(p.run_dir("RUN-001").join("trace.jsonl"))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert!(trace["command"].to_string().contains("--max-budget-usd"));
    let state =
        std::fs::read_to_string(p.run_dir("RUN-001").join("task/RUN-001/.state.yaml")).unwrap();
    assert!(
        state.contains("flow: Contract") && state.contains("state: Verified"),
        "{state}"
    );
}

/// AC-02.
#[test]
fn a_run_that_never_passes_fails_after_its_verifications() {
    let p = Project::new(5.0, 2);
    p.stub(&format!("cat {FIXTURE}"));

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(!out.status.success());
    let last = p.last("RUN-001");
    assert_eq!(last["event"], "failed");
    assert!(
        last["reason"]
            .as_str()
            .unwrap()
            .contains("verifier budget exhausted"),
        "{last}"
    );
    assert_eq!(p.count("RUN-001", "verify_started"), 2);
    assert_eq!(p.count("RUN-001", "verify_failed"), 2);
    assert_eq!(p.count("RUN-001", "attempt"), 2);
    assert!(
        p.root.join(".zforge/worktrees/RUN-001").is_dir(),
        "kept for inspection"
    );
}

/// AC-03 and AC-06: the budget is shared by every run of the task in the
/// handover.
#[test]
fn a_run_stops_at_its_budget_and_the_next_one_is_refused() {
    let p = Project::new(0.3, 3);
    p.stub(&format!("cat {FIXTURE}"));

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(!out.status.success());
    let last = p.last("RUN-001");
    assert_eq!(last["event"], "blocked", "{:?}", p.events("RUN-001"));
    assert!(last["reason"].as_str().unwrap().starts_with("budget:"));
    assert_eq!(
        p.count("RUN-001", "attempt"),
        2,
        "no agent call after the budget ran out"
    );
    let args = std::fs::read_to_string(p.marks.join("args")).unwrap();
    assert!(
        args.lines()
            .nth(1)
            .unwrap()
            .contains("--max-budget-usd 0.10"),
        "second call gets what is left: {args}"
    );

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("budget of TASK-001 in HANDOVER-001 is used up"),
        "{}",
        err(&out)
    );
    assert!(!p.run_dir("RUN-002").exists());
}

/// Foreground Ctrl-C: the agent is stopped and the run recorded cancelled.
#[test]
fn an_interrupted_foreground_run_is_recorded_as_cancelled() {
    let p = Project::new(3.0, 3);
    p.stub("sleep 30");
    let child = p
        .cmd(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !p.marks.join("cwd").exists() {
        assert!(Instant::now() < deadline, "agent never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    // SAFETY: signalling a child we spawned.
    unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
    let out = child.wait_with_output().unwrap();

    assert_eq!(out.status.code(), Some(130), "{}", err(&out));
    let last = p.last("RUN-001");
    assert_eq!(last["event"], "cancelled");
    assert!(last["reason"].as_str().unwrap().contains("signal 2"));
}

/// Refusals before anything is created: dependent task, unknown runner.
#[test]
fn refusals_create_no_run() {
    let p = Project::new(3.0, 3);
    p.stub(&format!("cat {FIXTURE}"));
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-002"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("running dependent tasks comes with Mốc C"),
        "{}",
        err(&out)
    );
    assert!(!p.root.join(".zforge/runs").join("RUN-001").exists());

    std::fs::write(p.home.join("registry.yaml"), "agents: {}\n").unwrap();
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(!out.status.success());
    let last = p.last("RUN-001");
    assert_eq!(last["event"], "failed");
    assert!(
        last["reason"]
            .as_str()
            .unwrap()
            .contains("leaf tasks run on Claude"),
        "{last}"
    );
    assert!(!Path::new(&p.root.join(".zforge/worktrees/RUN-001")).exists());
}

// ─── TASK-005: status, list, background, cancel, retry, clean ───────────────

impl Project {
    fn wait_for(&self, id: &str, event: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.count(id, event) == 0 {
            assert!(
                Instant::now() < deadline,
                "{id} never reached `{event}`: {:?}",
                self.events(id)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn pid_file(&self, name: &str) -> i32 {
        let path = self.marks.join(name);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(pid) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "{name} never written");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn dies_within(pid: i32, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while alive(pid) {
        if Instant::now() > deadline {
            // SAFETY: do not leak it into the rest of the suite.
            unsafe { libc::kill(pid, libc::SIGKILL) };
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

/// Agent that replaces the suite with a slow one recording its pid, so a
/// test can act while the run is verifying.
fn slow_suite(marks: &Path) -> String {
    format!(
        "printf '#!/bin/sh\\necho $$ > {m}/test.pid\\nsleep 30\\n' > test.sh\ncat {FIXTURE}",
        m = marks.display()
    )
}

/// AC-01: what `run status` and `run list` show.
#[test]
fn status_and_list_show_the_run() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .status
        .success());

    let out = p.zforge(&["run", "status", "RUN-001"]);
    assert!(out.status.success(), "{}", err(&out));
    let text = String::from_utf8_lossy(&out.stdout);
    for want in [
        "RUN-001 — TASK-001 of F/HANDOVER-001: verified",
        "branch zforge/TASK-001/RUN-001",
        "budget $3.00, spent $0.20; 1 attempt(s), 1 verification(s)",
        "attempt 1 started (up to $3.00)",
        "attempt 1 finished, exit 0, $0.20",
        "verified — candidate",
        "agent calls",
        "code · attempt 1 · claude",
    ] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }
    let v: serde_json::Value =
        serde_json::from_slice(&p.zforge(&["run", "status", "RUN-001", "--json"]).stdout).unwrap();
    assert_eq!(v["state"]["status"], "verified");
    assert_eq!(v["traces"].as_array().unwrap().len(), 1);
    assert!(p.run_dir("RUN-001").join("result.md").is_file());
    assert!(p.run_dir("RUN-001").join("progress.md").is_file());

    let list = String::from_utf8_lossy(
        &p.zforge(&["run", "list", "--handover", "HANDOVER-001"])
            .stdout,
    )
    .into_owned();
    assert!(
        list.contains("RUN-001") && list.contains("verified"),
        "{list}"
    );
    let none = String::from_utf8_lossy(
        &p.zforge(&["run", "list", "--handover", "HANDOVER-009"])
            .stdout,
    )
    .into_owned();
    assert!(none.contains("no runs yet"), "{none}");
}

/// AC-02: a background run whose worker dies is interrupted; retry makes a
/// new run with what is left of the budget.
#[test]
fn a_dead_background_worker_is_interrupted_and_retry_starts_over() {
    let p = Project::new(3.0, 3);
    p.stub(&slow_suite(&p.marks));
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--async"]);
    assert!(out.status.success(), "{}", err(&out));
    p.wait_for("RUN-001", "verify_started");
    let test_pid = p.pid_file("test.pid");

    let worker: i32 = std::fs::read_to_string(p.run_dir("RUN-001").join("launch.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: killing the worker we started; SIGKILL cannot be caught.
    unsafe { libc::kill(worker, libc::SIGKILL) };
    assert!(dies_within(worker, Duration::from_secs(5)));

    let v: serde_json::Value =
        serde_json::from_slice(&p.zforge(&["run", "status", "RUN-001", "--json"]).stdout).unwrap();
    assert_eq!(v["state"]["status"], "failed");
    assert!(v["state"]["reason"]
        .as_str()
        .unwrap()
        .starts_with("interrupted"));
    // Recording the interruption also stops what the dead worker left
    // running — its test suite here, an agent still spending money there.
    assert!(
        dies_within(test_pid, Duration::from_secs(10)),
        "the dead worker's test process was left running"
    );
    assert!(
        p.zforge(&["run", "cancel", "RUN-001"]).status.code() != Some(0),
        "already final"
    );

    let out = p.zforge(&["run", "retry", "RUN-001", "--async"]);
    assert!(out.status.success(), "{}", err(&out));
    let meta: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(p.run_dir("RUN-002").join("run.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["retry_of"], "RUN-001");
    let budget = meta["budget_usd"].as_f64().unwrap();
    assert!((budget - (3.0 - 0.2003)).abs() < 0.001, "{budget}");
    p.wait_for("RUN-002", "started");
    assert!(p.zforge(&["run", "cancel", "RUN-002"]).status.success());
}

/// AC-03: cancelling a background run stops its test process and records it.
#[test]
fn cancel_stops_a_background_run_while_it_verifies() {
    let p = Project::new(3.0, 3);
    p.stub(&slow_suite(&p.marks));
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--async"])
        .status
        .success());
    p.wait_for("RUN-001", "verify_started");
    let test_pid = p.pid_file("test.pid");

    let out = p.zforge(&["run", "cancel", "RUN-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        dies_within(test_pid, Duration::from_secs(10)),
        "test process survived the cancel"
    );
    assert_eq!(p.last("RUN-001")["event"], "cancelled");
}

/// AC-03: cancelling while the agent runs stops the agent.
#[test]
fn cancel_stops_a_background_run_while_the_agent_works() {
    let p = Project::new(3.0, 3);
    p.stub(&format!(
        "echo $$ > {}/agent.pid\nsleep 30",
        p.marks.display()
    ));
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--async"])
        .status
        .success());
    let agent = p.pid_file("agent.pid");

    let out = p.zforge(&["run", "cancel", "RUN-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        dies_within(agent, Duration::from_secs(10)),
        "agent survived the cancel"
    );
    let state: serde_json::Value =
        serde_json::from_slice(&p.zforge(&["run", "status", "RUN-001", "--json"]).stdout).unwrap();
    assert_eq!(state["state"]["status"], "cancelled");
    assert_eq!(
        state["state"]["cost_usd"], 3.0,
        "the call in flight counts its allotment"
    );
}

/// AC-04: clean refuses a live run, and keeps the branch — with the
/// worktree's uncommitted work committed to it.
#[test]
fn clean_removes_a_finished_worktree_and_keeps_the_work() {
    let p = Project::new(3.0, 3);
    p.stub(&slow_suite(&p.marks));
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--async"])
        .status
        .success());
    p.wait_for("RUN-001", "verify_started");

    let out = p.zforge(&["run", "clean", "RUN-001"]);
    assert!(!out.status.success());
    assert!(err(&out).contains("still verifying"), "{}", err(&out));

    assert!(p.zforge(&["run", "cancel", "RUN-001"]).status.success());
    let out = p.zforge(&["run", "clean", "RUN-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("uncommitted work saved"));
    assert!(!p.root.join(".zforge/worktrees/RUN-001").exists());
    let log = p.git(&["log", "--format=%s", "-1", "zforge/TASK-001/RUN-001"]);
    assert!(log.contains("zforge: uncommitted work of RUN-001"), "{log}");
    let saved = p.git(&["show", "zforge/TASK-001/RUN-001:test.sh"]);
    assert!(
        saved.contains("sleep 30"),
        "the agent's change is on the branch"
    );

    let out = p.zforge(&["run", "clean", "RUN-001"]);
    assert!(err(&out).contains("already cleaned"), "{}", err(&out));
}

// ─── TASK-006: the contract must change ─────────────────────────────────────

/// AC-01, AC-02 and AC-03: an agent that asks for an amendment stops the
/// run, the request is judged against §8, and the contract it was given is
/// untouched.
#[test]
fn a_change_request_blocks_the_run_and_leaves_the_contract_alone() {
    let p = Project::new(3.0, 3);
    let sections = [
        "Hợp đồng đang áp dụng",
        "Bằng chứng",
        "Đề xuất",
        "Tác động",
        "Cần quyết định",
    ];
    let body: String = sections
        .iter()
        .map(|t| format!("## {t}\\nnội dung\\n"))
        .collect();
    let cr = p.root.join(".zforge/intakes/F/changes/CHANGE-RUN-001.md");
    p.stub(&format!(
        "printf '{body}' > {}\ncat {FIXTURE}",
        cr.display()
    ));
    let snapshot = p
        .root
        .join(".zforge/intakes/F/.records/revisions/tasks__TASK-001/1.md");
    let before = std::fs::read_to_string(&snapshot).unwrap();

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(!out.status.success());
    let last = p.last("RUN-001");
    assert_eq!(last["event"], "blocked");
    assert_eq!(last["reason"], "amendment: CHANGE-RUN-001");
    assert_eq!(p.count("RUN-001", "attempt"), 1, "no second agent call");
    assert_eq!(
        p.count("RUN-001", "verify_started"),
        0,
        "nothing was verified"
    );

    // AC-03: the contract the run used is exactly what the manifest pins.
    assert_eq!(std::fs::read_to_string(&snapshot).unwrap(), before);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            p.root
                .join(".zforge/intakes/F/.records/handovers/HANDOVER-001.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pinned = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == "tasks/TASK-001.md")
        .unwrap()["sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let digest = zforge::intake::hash::sha256(&before);
    assert_eq!(digest, pinned, "snapshot still matches the handover");

    // The user sees the request where the intake is reviewed.
    let status = String::from_utf8_lossy(&p.zforge(&["intake", "status", "F"]).stdout).into_owned();
    assert!(status.contains("changes/CHANGE-RUN-001.md"), "{status}");
}

/// An incomplete request still stops the run, and says what is missing.
#[test]
fn an_incomplete_change_request_says_what_it_lacks() {
    let p = Project::new(3.0, 3);
    let cr = p.root.join(".zforge/intakes/F/changes/CHANGE-RUN-001.md");
    p.stub(&format!(
        "printf '## Đề xuất\\nđổi output\\n' > {}\ncat {FIXTURE}",
        cr.display()
    ));

    assert!(!p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .status
        .success());
    let reason = p.last("RUN-001")["reason"].as_str().unwrap().to_string();
    assert!(
        reason.starts_with("amendment: CHANGE-RUN-001 (incomplete"),
        "{reason}"
    );
    assert!(reason.contains("Bằng chứng"), "{reason}");
}
