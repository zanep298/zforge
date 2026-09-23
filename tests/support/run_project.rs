//! The `zforge run` test project: a git repository with a bug (`add`
//! subtracts), an intake accepted and handed over through the library, and
//! the real binary. `claude` is a stub that works in its cwd — fixing
//! `lib.sh` or not — and replays a real Claude Code stream, whose `result`
//! reports $0.20. Shared by `run_test.rs` and `feature_run_test.rs`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/claude/stream_code_agent.jsonl"
);

pub const BUG: &str = "add() { echo $(( $1 - $2 )); }\n";

pub const FIX: &str = "add() { echo $(( $1 + $2 )); }\n";

pub fn task(id: &str, deps: &str) -> String {
    format!(
        "---\nid: {id}\nparent: F\nrequirements: [REQ-001]\ndepends_on: [{deps}]\n---\n\n# {id}\n\n\
         ## Mục tiêu\nadd trả về tổng.\n## Input\nlib.sh.\n## Output\nadd đúng.\n## Ràng buộc\nChỉ sửa lib.sh.\n\
         ## Tự chủ\nTự chọn cách sửa.\n## Acceptance và kiểm chứng\n- AC-01: sh test.sh pass\n## Bàn giao\nLocal.\n\
         ## Cần amendment khi\nPhải sửa test.\n## Câu hỏi còn mở\n"
    )
}

pub struct Project {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub marks: PathBuf,
}

impl Project {
    /// `budget` and `iterations` go into the handover's policy. TASK-001,
    /// and TASK-002 depending on it; the integration check is prose, so the
    /// test command.
    pub fn new(budget: f64, iterations: u32) -> Self {
        Self::with(
            budget,
            iterations,
            &[("TASK-001", ""), ("TASK-002", "TASK-001")],
            "sh test.sh",
        )
    }

    /// `tasks` as (id, depends_on); `integration` is the body of the
    /// breakdown's "Kiểm chứng tích hợp".
    pub fn with(budget: f64, iterations: u32, tasks: &[(&str, &str)], integration: &str) -> Self {
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
        p.hand_over(tasks, integration);
        p
    }

    pub fn git(&self, args: &[&str]) -> String {
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

    pub fn hand_over(&self, tasks: &[(&str, &str)], integration: &str) {
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
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| *id).collect();
        std::fs::write(
            d.join("04-breakdown.md"),
            format!(
                "# K\n\n## Task\n{}\n\n## Kiểm chứng tích hợp\n{integration}\n\n## Câu hỏi còn mở\n",
                ids.join(", ")
            ),
        )
        .unwrap();
        for (id, deps) in tasks {
            std::fs::write(d.join(format!("tasks/{id}.md")), task(id, deps)).unwrap();
        }
        for f in i.files() {
            review::review(&i, &f).unwrap();
            review::accept(&i, &f, None).unwrap();
        }
        let config = zforge::config::load_from(&self.root.join(".zforge/config.yaml")).unwrap();
        let r = readiness::check(&i, &self.root, &[], &config).unwrap();
        assert!(r.ready, "{:?}", r.checks);
        handover::create(&i, &self.root, &[], &config, &r.files, None).unwrap();
    }

    /// Edit `file` of intake F (replace `from` with `to`), then review and
    /// accept the new revision, as the user would.
    pub fn amend(&self, file: &str, from: &str, to: &str) {
        use zforge::intake::review;
        let i = zforge::intake::Intake::open(&self.root, "F").unwrap();
        let path = i.dir.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
        review::review(&i, file).unwrap();
        review::accept(&i, file, None).unwrap();
    }

    /// Hand every task over again: the next `HANDOVER-nnn`. Files that
    /// rest on something amended since they were accepted are confirmed
    /// again first, as the user would (readiness refuses otherwise).
    pub fn hand_over_again(&self) {
        use zforge::intake::{handover, readiness, review};
        let i = zforge::intake::Intake::open(&self.root, "F").unwrap();
        // Confirming one file can make those below it stale in turn.
        loop {
            let stale = readiness::stale(&i).unwrap();
            if stale.is_empty() {
                break;
            }
            for f in stale {
                review::review(&i, &f).unwrap();
                review::accept(&i, &f, None).unwrap();
            }
        }
        let config = zforge::config::load_from(&self.root.join(".zforge/config.yaml")).unwrap();
        let r = readiness::check(&i, &self.root, &[], &config).unwrap();
        assert!(r.ready, "{:?}", r.checks);
        handover::create(&i, &self.root, &[], &config, &r.files, None).unwrap();
    }

    pub fn agent_calls(&self) -> usize {
        std::fs::read_to_string(self.marks.join("cwd"))
            .map(|t| t.lines().count())
            .unwrap_or(0)
    }

    /// Register `claude` as a stub: records its cwd, then runs `body`.
    pub fn stub(&self, body: &str) {
        let script = self.home.join("claude-stub");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat > {marks}/prompt\npwd >> {marks}/cwd\necho \"$@\" >> {marks}/args\n{body}\n",
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

    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_zforge"));
        c.args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    pub fn zforge(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    pub fn run_dir(&self, id: &str) -> PathBuf {
        self.root.join(".zforge/runs").join(id)
    }

    pub fn events(&self, id: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.run_dir(id).join("events.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    pub fn count(&self, id: &str, event: &str) -> usize {
        self.events(id)
            .iter()
            .filter(|e| e["event"] == event)
            .count()
    }

    pub fn last(&self, id: &str) -> serde_json::Value {
        self.events(id).last().cloned().unwrap()
    }
}

pub fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub fn fix_and_report() -> String {
    format!(
        "printf '{}' > lib.sh\ncat {FIXTURE}",
        FIX.trim_end().replace('%', "%%") + "\\n"
    )
}

pub fn status_json(p: &Project, id: &str) -> serde_json::Value {
    let out = p.zforge(&["run", "status", id, "--json"]);
    assert!(out.status.success(), "{}", err(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

impl Project {
    pub fn wait_for(&self, id: &str, event: &str) {
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

    /// Wait until `run` has recorded the process group of the child it is
    /// running. A worker killed before it records one leaves that child
    /// behind — a window of a few milliseconds that a test must not hit by
    /// chance.
    pub fn wait_for_recorded_child(&self, run: &str) {
        let path = self.run_dir(run).join("child-pgids");
        let deadline = Instant::now() + Duration::from_secs(30);
        while std::fs::read_to_string(&path)
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            assert!(Instant::now() < deadline, "{run} never recorded a child");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn pid_file(&self, name: &str) -> i32 {
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

pub fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes.
    unsafe { libc::kill(pid, 0) == 0 }
}

pub fn dies_within(pid: i32, limit: Duration) -> bool {
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
pub fn slow_suite(marks: &Path) -> String {
    format!(
        "printf '#!/bin/sh\\necho $$ > {m}/test.pid\\nsleep 30\\n' > test.sh\ncat {FIXTURE}",
        m = marks.display()
    )
}

impl Project {
    pub fn knowledge(&self) -> serde_json::Value {
        serde_json::from_slice(&self.zforge(&["knowledge", "index", "--json"]).stdout).unwrap()
    }
}

pub fn entry(v: &serde_json::Value, id: &str) -> serde_json::Value {
    v.as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .cloned()
        .unwrap()
}

/// Run `task` to `verified` and return its sealed output.
pub fn verify_task(p: &Project, task: &str) -> String {
    let out = p.zforge(&["run", "HANDOVER-001", "--task", task]);
    assert!(out.status.success(), "{task}: {}", err(&out));
    let runs = p.zforge(&["run", "list", "--json"]);
    let runs: serde_json::Value = serde_json::from_slice(&runs.stdout).unwrap();
    runs.as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|r| r["meta"]["task"] == task)
        .and_then(|r| r["state"]["output"].as_str())
        .unwrap()
        .to_string()
}

/// TASK-001 ← TASK-002 ← TASK-003, and TASK-004 on its own. The integration
/// check wants both chains' marks in one tree.
pub fn feature_project(iterations: u32) -> Project {
    Project::with(
        3.0,
        iterations,
        &[
            ("TASK-001", ""),
            ("TASK-002", "TASK-001"),
            ("TASK-003", "TASK-002"),
            ("TASK-004", ""),
        ],
        "```bash\nsh test.sh\ntest -f TASK-001.done\ntest -f TASK-003.done\ntest -f TASK-004.done\n```",
    )
}

/// Every task fixes `lib.sh` (the same way) and leaves `<task>.done`;
/// `before` runs first with `$task` set.
pub fn per_task(p: &Project, before: &str) -> String {
    format!(
        "task=$(sed -n '1s/^# Leaf task \\([^ ]*\\).*/\\1/p' {marks}/prompt)\n{before}\n{}\ntouch \"$task.done\"",
        fix_and_report(),
        marks = p.marks.display()
    )
}

pub fn feature_dir(p: &Project) -> PathBuf {
    p.root.join(".zforge/runs/features/F/HANDOVER-001")
}

/// A handover run to the end, then HANDOVER-002 after `change`.
pub fn handed_over_twice(change: impl Fn(&Project)) -> Project {
    let p = feature_project(3);
    p.stub(&per_task(&p, ""));
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    change(&p);
    p.hand_over_again();
    p
}

pub fn states(p: &Project, handover: &str) -> Vec<(String, String)> {
    let out = p.zforge(&["run", "status", handover, "--json"]);
    let f: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    f["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let label = match (t["state"].as_str().unwrap(), t.get("reused_from")) {
                ("verified", Some(_)) => "reused",
                (s, _) => s,
            };
            (t["task"].as_str().unwrap().to_string(), label.to_string())
        })
        .collect()
}

pub fn expect(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

pub fn merge_into_main(p: &Project, args: &[&str]) {
    let mut all = vec!["-c", "user.email=t@t", "-c", "user.name=t", "merge", "-q"];
    all.extend_from_slice(args);
    p.git(&all);
}
