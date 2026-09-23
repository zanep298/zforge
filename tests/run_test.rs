#![cfg(unix)]
//! Mốc B `zforge run` end to end (MOC-B TASK-004).
//!
//! The project, the stub agent and the helpers are in
//! `support/run_project.rs`, shared with `feature_run_test.rs`.

#[path = "support/run_project.rs"]
mod run_project;

use run_project::*;
use std::path::Path;
use std::time::{Duration, Instant};

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

/// MOC-C TASK-001 AC-01 and AC-03: the tested tree is sealed as the run's
/// output — a commit on its branch — and a later commit to the branch does
/// not change what the run verified.
#[test]
fn a_verified_run_seals_the_tested_tree_as_its_output() {
    let p = Project::new(3.0, 3);
    // The stub leaves its fix uncommitted.
    p.stub(&fix_and_report());

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(out.status.success(), "{}", err(&out));

    let worktree = p.root.join(".zforge/worktrees/RUN-001");
    let verified = p.last("RUN-001");
    assert_eq!(verified["event"], "verified");
    let commit = verified["commit"]
        .as_str()
        .expect("verified carries its output");
    let branch = "zforge/TASK-001/RUN-001";
    assert_eq!(p.git(&["rev-parse", branch]).trim(), commit);
    assert_eq!(p.git(&["show", &format!("{commit}:lib.sh")]), FIX);
    assert_eq!(
        p.git(&["log", "-1", "--format=%an|%s", commit]).trim(),
        "zforge|zforge: output of RUN-001 (TASK-001)"
    );
    // The output is the tree the tests ran on.
    assert!(p
        .git(&[
            "-C",
            &worktree.display().to_string(),
            "status",
            "--porcelain"
        ])
        .is_empty());
    assert_eq!(
        zforge::evidence::fingerprint(&worktree).hash(),
        verified["candidate"].as_str()
    );

    let status = p.zforge(&["run", "status", "RUN-001", "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(state["state"]["output"], commit);

    // Whatever lands on the branch afterwards is not the output.
    std::fs::write(worktree.join("later.txt"), "after the run\n").unwrap();
    let wt = worktree.display().to_string();
    p.git(&["-C", &wt, "add", "-A"]);
    p.git(&[
        "-C",
        &wt,
        "-c",
        "user.email=u@u",
        "-c",
        "user.name=u",
        "commit",
        "-qm",
        "later",
    ]);
    assert_ne!(p.git(&["rev-parse", branch]).trim(), commit);
    let status = p.zforge(&["run", "status", "RUN-001", "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(state["state"]["output"], commit);
}

/// MOC-C TASK-002 AC-01 and AC-05: a dependent task starts from its
/// dependency's sealed output. TASK-002's agent changes nothing, so its
/// tests pass only because TASK-001's fix is where it starts.
#[test]
fn a_dependent_task_starts_from_its_dependencys_output() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(out.status.success(), "{}", err(&out));
    let output = status_json(&p, "RUN-001")["state"]["output"]
        .as_str()
        .unwrap()
        .to_string();

    p.stub(&format!("cat {FIXTURE}"));
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-002"]);
    assert!(out.status.success(), "{}", err(&out));

    let meta = &status_json(&p, "RUN-002")["meta"];
    assert_eq!(meta["start"]["commit"], output.as_str());
    assert_eq!(
        meta["start"]["from"],
        serde_json::json!([{"task": "TASK-001", "run": "RUN-001", "commit": output}])
    );
    assert_eq!(p.last("RUN-002")["event"], "verified");
    let worktree = p.root.join(".zforge/worktrees/RUN-002");
    assert_eq!(
        std::fs::read_to_string(worktree.join("lib.sh")).unwrap(),
        FIX
    );
    // Its branch continues from TASK-001's output.
    p.git(&[
        "merge-base",
        "--is-ancestor",
        &output,
        "zforge/TASK-002/RUN-002",
    ]);

    // AC-05: a task without dependencies records no start.
    assert!(status_json(&p, "RUN-001")["meta"].get("start").is_none());
    let yaml = std::fs::read_to_string(p.run_dir("RUN-001").join("run.yaml")).unwrap();
    assert!(!yaml.contains("start"), "{yaml}");
}

/// MOC-C TASK-002 AC-02: a dependency that ran but was not verified.
#[test]
fn a_dependency_that_failed_refuses_the_task() {
    let p = Project::new(3.0, 1);
    p.stub(&format!("cat {FIXTURE}"));
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(!out.status.success());

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-002"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains(
            "TASK-002 depends on TASK-001, which has no verified run in HANDOVER-001 (latest: RUN-001 failed)"
        ),
        "{}",
        err(&out)
    );
    assert!(!p.run_dir("RUN-002").exists());
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
    // MOC-C TASK-002 AC-02: a dependency without output refuses the task.
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-002"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("TASK-002 depends on TASK-001, which has not run in HANDOVER-001"),
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

// ─── TASK-007: knowledge and the run's result ───────────────────────────────

/// AC-01 and AC-03.
#[test]
fn a_verified_run_makes_its_requirement_verified() {
    let p = Project::new(3.0, 3);

    // Before any run: handed over, not built.
    let before = entry(&p.knowledge(), "REQ-001");
    assert_eq!(before["implementation"], "handed_over");

    // A failed run changes nothing.
    p.stub(&format!("cat {FIXTURE}"));
    assert!(!p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .status
        .success());
    assert_eq!(
        entry(&p.knowledge(), "REQ-001")["implementation"],
        "handed_over"
    );

    p.stub(&fix_and_report());
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .status
        .success());
    let e = entry(&p.knowledge(), "REQ-001");
    assert_eq!(e["implementation"], "verified");
    assert_eq!(e["verified_by"], "RUN-002");
    let candidate = e["candidate"].as_str().unwrap();
    assert_eq!(
        Some(candidate),
        zforge::evidence::fingerprint(&p.root.join(".zforge/worktrees/RUN-002")).hash(),
        "the candidate is the tree that passed"
    );
    let md = std::fs::read_to_string(p.root.join(".zforge/knowledge/index.md")).unwrap();
    assert!(md.contains("verified (RUN-002"), "{md}");

    // The run's own result reads back what it used and proved.
    let result = std::fs::read_to_string(p.run_dir("RUN-002").join("result.md")).unwrap();
    for want in [
        "RUN-002 — TASK-001 of F/HANDOVER-001: verified",
        "branch zforge/TASK-001/RUN-002",
        "## Contract",
        "tasks/TASK-001.md rev 1",
        "## Verifications",
        "1. passed — candidate",
    ] {
        assert!(result.contains(want), "missing {want:?} in:\n{result}");
    }

    // AC-03: the views are generated; editing them changes nothing.
    std::fs::write(p.run_dir("RUN-002").join("result.md"), "RUN-002 failed\n").unwrap();
    let after = entry(&p.knowledge(), "REQ-001");
    assert_eq!(after["implementation"], "verified");
    let status: serde_json::Value =
        serde_json::from_slice(&p.zforge(&["run", "status", "RUN-002", "--json"]).stdout).unwrap();
    assert_eq!(status["state"]["status"], "verified");
}

/// AC-02: a requirement dropped from the accepted outcome is never reported
/// as built, whatever its tasks' runs did.
#[test]
fn a_superseded_requirement_is_never_verified() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .status
        .success());
    assert_eq!(
        entry(&p.knowledge(), "REQ-001")["implementation"],
        "verified"
    );

    // The user accepts an outcome without REQ-001.
    let intake = zforge::intake::Intake::open(&p.root, "F").unwrap();
    std::fs::write(
        intake.dir.join("01-outcome.md"),
        "# F\n\n## Yêu cầu\n\n- REQ-002: cộng đúng\n\n## Câu hỏi còn mở\n",
    )
    .unwrap();
    zforge::intake::review::review(&intake, "01-outcome.md").unwrap();
    zforge::intake::review::accept(&intake, "01-outcome.md", None).unwrap();

    let e = entry(&p.knowledge(), "REQ-001");
    assert_eq!(e["decision"], "superseded");
    assert_eq!(e["implementation"], "not_implemented");
    assert!(e["verified_by"].is_null());
}

// ─── following a background run (outside the MOC-B contracts) ───────────────

/// `run log --follow` prints the worker's output and returns when the run
/// ends; `run wait` reports the verdict.
#[test]
fn log_and_wait_follow_a_background_run_to_its_end() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--async"])
        .status
        .success());

    let out = p.zforge(&["run", "wait", "RUN-001", "--timeout", "60"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("RUN-001 verified"));

    let out = p.zforge(&["run", "log", "RUN-001", "--follow"]);
    assert!(out.status.success(), "{}", err(&out));
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(
        log.contains("attempt 1") && log.contains("verifying"),
        "{log}"
    );

    let out = p.zforge(&["run", "log", "RUN-001", "--tail", "1"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), 1);

    // A foreground run keeps its output on the terminal, so it has no log.
    let p2 = Project::new(3.0, 3);
    p2.stub(&fix_and_report());
    assert!(p2
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001"])
        .status
        .success());
    let out = p2.zforge(&["run", "log", "RUN-001"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("only a run started with --async"),
        "{}",
        err(&out)
    );
}

/// A run that fails makes `wait` exit non-zero.
#[test]
fn wait_reports_a_failed_run() {
    let p = Project::new(5.0, 1);
    p.stub(&format!("cat {FIXTURE}"));
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--async"])
        .status
        .success());
    let out = p.zforge(&["run", "wait", "RUN-001", "--timeout", "60"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("failed"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// ─── MOC-C TASK-003: integration runs ───────────────────────────────────────

/// AC-01 and AC-04: one leaf, so the check runs on its output as is; the
/// breakdown's check is prose, so the project's test command is used. No
/// agent, no budget.
#[test]
fn an_integration_run_checks_the_leaf_output() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());
    verify_task(&p, "TASK-001");
    p.stub(&format!("cat {FIXTURE}"));
    let leaf = verify_task(&p, "TASK-002");
    let calls = std::fs::read_to_string(p.marks.join("cwd"))
        .unwrap()
        .lines()
        .count();

    let out = p.zforge(&["run", "HANDOVER-001", "--integration"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(err(&out)
        .contains("integration of HANDOVER-001: 1 check(s) from the project's test command"));

    let v = status_json(&p, "RUN-003");
    let meta = &v["meta"];
    assert_eq!(meta["kind"], "integration");
    assert_eq!(meta["task"], "integration");
    assert_eq!(meta["branch"], "zforge/F/integration/RUN-003");
    assert_eq!(meta["budget_usd"], 0.0);
    assert_eq!(
        meta["checks"],
        serde_json::json!({"from": "config", "commands": ["sh test.sh"]})
    );
    assert_eq!(meta["start"]["commit"], leaf.as_str());
    assert_eq!(meta["start"]["from"][0]["task"], "TASK-002");

    let state = &v["state"];
    assert_eq!(state["status"], "verified");
    assert_eq!(state["attempts"], 0);
    assert_eq!(state["cost_usd"], 0.0);
    let commit = state["output"].as_str().unwrap();
    // Nothing to seal on top of the leaf's output: it is the output.
    assert_eq!(commit, leaf);
    assert_eq!(
        p.git(&["rev-parse", "zforge/F/integration/RUN-003"]).trim(),
        commit
    );
    let worktree = p.root.join(".zforge/worktrees/RUN-003");
    assert_eq!(
        std::fs::read_to_string(worktree.join("lib.sh")).unwrap(),
        FIX
    );
    let log = std::fs::read_to_string(p.run_dir("RUN-003").join("checks.log")).unwrap();
    assert!(log.contains("$ sh test.sh") && log.contains("ok"), "{log}");
    // No agent was called.
    assert_eq!(
        std::fs::read_to_string(p.marks.join("cwd"))
            .unwrap()
            .lines()
            .count(),
        calls
    );
    // Its run is not an output a task could depend on, nor task spending.
    assert!(!p.root.join(".zforge/runs/RUN-003/trace.jsonl").exists());
}

/// AC-02: two leaves meet in the integration worktree, and the checks come
/// from the breakdown's code block.
#[test]
fn an_integration_run_merges_every_leaf() {
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", ""), ("TASK-002", "")],
        "```bash\nsh test.sh\ntest -f extra.txt\n```",
    );
    // Both tasks fix lib.sh the same way; TASK-002 also adds extra.txt.
    p.stub(&format!(
        "{}\n[ \"$(sed -n '1s/^# Leaf task \\([^ ]*\\).*/\\1/p' {marks}/prompt)\" = TASK-002 ] && echo extra > extra.txt\ntrue",
        fix_and_report(),
        marks = p.marks.display()
    ));
    let a = verify_task(&p, "TASK-001");
    let b = verify_task(&p, "TASK-002");

    let out = p.zforge(&["run", "HANDOVER-001", "--integration"]);
    assert!(out.status.success(), "{}", err(&out));

    let meta = &status_json(&p, "RUN-003")["meta"];
    assert_eq!(meta["checks"]["from"], "breakdown");
    assert_eq!(
        meta["checks"]["commands"],
        serde_json::json!(["sh test.sh", "test -f extra.txt"])
    );
    let from: Vec<&str> = meta["start"]["from"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["commit"].as_str().unwrap())
        .collect();
    assert_eq!(from, [a.as_str(), b.as_str()]);
    let branch = "zforge/F/integration/RUN-003";
    for c in [&a, &b] {
        p.git(&["merge-base", "--is-ancestor", c, branch]);
    }
    assert_eq!(p.last("RUN-003")["event"], "verified");
}

/// AC-03: the first failing command fails the run and stops the rest.
#[test]
fn a_failing_integration_command_fails_the_run() {
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", "")],
        "```bash\nsh test.sh\necho boom; exit 3\ntouch third-ran\n```",
    );
    p.stub(&fix_and_report());
    verify_task(&p, "TASK-001");

    let out = p.zforge(&["run", "HANDOVER-001", "--integration"]);
    assert!(!out.status.success());

    let events = p.events("RUN-002");
    let failed = events
        .iter()
        .find(|e| e["event"] == "verify_failed")
        .unwrap();
    assert_eq!(
        failed["failed_tests"],
        serde_json::json!(["echo boom; exit 3"])
    );
    let last = p.last("RUN-002");
    assert_eq!(last["event"], "failed");
    let reason = last["reason"].as_str().unwrap();
    assert!(
        reason.contains("`echo boom; exit 3` failed (exit 3)"),
        "{reason}"
    );
    assert!(reason.contains("boom"), "{reason}");
    assert!(!p.root.join(".zforge/worktrees/RUN-002/third-ran").exists());
    assert!(status_json(&p, "RUN-002")["state"].get("output").is_none());
}

/// AC-05: not every task is verified yet.
#[test]
fn an_integration_run_waits_for_every_task() {
    let p = Project::new(3.0, 3);
    let out = p.zforge(&["run", "HANDOVER-001", "--integration"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains(
            "integration needs every task verified: TASK-001, which has not run in HANDOVER-001"
        ),
        "{}",
        err(&out)
    );
    assert!(!p.run_dir("RUN-001").exists());

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001", "--integration"]);
    assert!(
        !out.status.success(),
        "--task and --integration exclude each other"
    );
}

// ─── MOC-C TASK-004: the state of a handover ────────────────────────────────

/// AC-03: `run status <HANDOVER>` shows what the records say, as the
/// library derives it; a stopped task blocks the integration.
#[test]
fn status_of_a_handover_is_derived_from_its_runs() {
    let p = Project::new(3.0, 1);
    let feature = |id: &str| -> serde_json::Value {
        let out = p.zforge(&["run", "status", id, "--json"]);
        assert!(out.status.success(), "{}", err(&out));
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let f = feature("HANDOVER-001");
    assert_eq!(
        f["tasks"][0],
        serde_json::json!({"task": "TASK-001", "state": "waiting", "on": []})
    );
    assert_eq!(f["tasks"][1]["on"], serde_json::json!(["TASK-001"]));
    assert_eq!(f["integration"]["state"], "waiting");

    p.stub(&fix_and_report());
    let a = verify_task(&p, "TASK-001");
    let f = feature("F/HANDOVER-001");
    assert_eq!(f["tasks"][0]["state"], "verified");
    assert_eq!(f["tasks"][0]["run"], "RUN-001");
    assert_eq!(f["tasks"][0]["commit"], a.as_str());
    assert_eq!(
        f["tasks"][1],
        serde_json::json!({"task": "TASK-002", "state": "waiting", "on": []})
    );
    assert_eq!(f["integration"]["on"], serde_json::json!(["TASK-002"]));

    // TASK-002 undoes the fix: its only verification fails.
    p.stub(&format!(
        "printf '{}' > lib.sh\ncat {FIXTURE}",
        BUG.trim_end().replace('%', "%%") + "\\n"
    ));
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-002"]);
    assert!(!out.status.success());
    let f = feature("HANDOVER-001");
    assert_eq!(f["tasks"][1]["state"], "stopped");
    assert_eq!(f["tasks"][1]["status"], "failed");
    assert_eq!(f["tasks"][1]["run"], "RUN-002");
    assert_eq!(
        f["integration"],
        serde_json::json!({"state": "blocked", "by": ["TASK-002"]})
    );
    // The CLI shows exactly what the library derives from the records.
    let derived = zforge::run::feature::load(&p.root, "HANDOVER-001").unwrap();
    assert_eq!(f, serde_json::to_value(&derived).unwrap());

    let out = p.zforge(&["run", "status", "HANDOVER-001"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("HANDOVER-001 of F"), "{text}");
    assert!(
        text.contains("TASK-001     verified  RUN-001  output"),
        "{text}"
    );
    assert!(text.contains("TASK-002     failed    RUN-002 — "), "{text}");
    assert!(
        text.contains("integration  blocked   by TASK-002"),
        "{text}"
    );
    assert!(text.contains("feature: not verified"), "{text}");

    // Nothing was written for the handover: its state is only derived.
    assert!(!p.root.join(".zforge/runs/features").exists());
}

// ─── MOC-C TASK-005: running a whole handover ───────────────────────────────

/// AC-01: one command, four task runs in order — each dependent one from
/// its dependency's output — then the integration.
#[test]
fn a_handover_runs_end_to_end() {
    let p = feature_project(3);
    p.stub(&per_task(&p, ""));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("feature: verified (RUN-005)"), "{text}");

    let meta = |id: &str| status_json(&p, id)["meta"].clone();
    let output = |id: &str| status_json(&p, id)["state"]["output"].clone();
    for (run, task) in [
        ("RUN-001", "TASK-001"),
        ("RUN-002", "TASK-002"),
        ("RUN-003", "TASK-003"),
        ("RUN-004", "TASK-004"),
    ] {
        assert_eq!(meta(run)["task"], task);
        assert_eq!(p.last(run)["event"], "verified", "{run}");
    }
    assert_eq!(meta("RUN-002")["start"]["commit"], output("RUN-001"));
    assert_eq!(meta("RUN-003")["start"]["commit"], output("RUN-002"));
    assert!(meta("RUN-004").get("start").is_none());
    let integration = meta("RUN-005");
    assert_eq!(integration["kind"], "integration");
    let from: Vec<&str> = integration["start"]["from"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["task"].as_str().unwrap())
        .collect();
    assert_eq!(from, ["TASK-003", "TASK-004"]);
    assert_eq!(p.last("RUN-005")["event"], "verified");

    // Nothing left to do: running it again changes nothing.
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(!p.run_dir("RUN-006").exists());
    // The loop is gone and says so.
    assert!(!feature_dir(&p).join("pid").exists());
}

/// AC-02: a task that fails stops its chain; the independent task still
/// runs; no integration; exit 1 with the picture.
#[test]
fn a_stopped_task_stops_only_its_chain() {
    let p = feature_project(1);
    // TASK-001 changes nothing, so its only verification fails.
    p.stub(&per_task(
        &p,
        &format!("[ \"$task\" = TASK-001 ] && {{ cat {FIXTURE}; exit 0; }}"),
    ));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("TASK-001     failed"), "{text}");
    assert!(
        text.contains("TASK-002     blocked   by TASK-001"),
        "{text}"
    );
    assert!(
        text.contains("TASK-003     blocked   by TASK-001"),
        "{text}"
    );
    assert!(text.contains("TASK-004     verified  RUN-002"), "{text}");
    assert!(
        text.contains("integration  blocked   by TASK-001"),
        "{text}"
    );
    assert!(
        err(&out).contains("HANDOVER-001 is not verified"),
        "{}",
        err(&out)
    );
    assert!(!p.run_dir("RUN-003").exists(), "nothing else ran");

    // A failure is a verdict: running again does not retry it.
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    assert!(!p.run_dir("RUN-003").exists());
}

/// AC-03 and AC-04: a killed loop is resumed where it stopped; while a loop
/// runs, a second loop or a run of the same task is refused. The kill lands
/// while TASK-002 verifies, after its agent call was paid for, so its
/// budget allows a new run.
#[test]
fn an_interrupted_handover_resumes_and_runs_one_at_a_time() {
    let p = feature_project(3);
    let wake = p.marks.join("wake");
    // Until woken, TASK-002's agent swaps in a suite that hangs.
    p.stub(&per_task(
        &p,
        &format!(
            "[ \"$task\" = TASK-002 ] && [ ! -e {} ] && {{ printf '#!/bin/sh\\necho $$ > {m}/test.pid\\nsleep 30\\n' > test.sh; cat {FIXTURE}; exit 0; }}",
            wake.display(),
            m = p.marks.display()
        ),
    ));
    let out = p.zforge(&["run", "HANDOVER-001", "--async"]);
    assert!(out.status.success(), "{}", err(&out));
    p.wait_for("RUN-002", "verify_started");
    let suite = p.pid_file("test.pid");

    // AC-04: one loop, one run of a task at a time.
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("HANDOVER-001 is already being run by pid"),
        "{}",
        err(&out)
    );
    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-002"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("TASK-002 already has RUN-002 verifying in HANDOVER-001"),
        "{}",
        err(&out)
    );
    let status = p.zforge(&["run", "status", "HANDOVER-001", "--json"]);
    let f: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(f["tasks"][1]["state"], "running");

    // Kill the loop outright: nothing gets to record anything.
    let loop_pid: i32 = std::fs::read_to_string(feature_dir(&p).join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: the test's own background worker.
    unsafe { libc::kill(loop_pid, libc::SIGKILL) };
    assert!(dies_within(loop_pid, Duration::from_secs(5)));
    std::fs::write(&wake, "").unwrap();

    // AC-03: run it again; the dead run is found interrupted (and its suite
    // stopped), TASK-001 is not run again, TASK-002 gets a new run.
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        dies_within(suite, Duration::from_secs(5)),
        "the orphaned suite was stopped"
    );
    let failed = p.last("RUN-002");
    assert_eq!(failed["event"], "failed");
    assert!(
        failed["reason"]
            .as_str()
            .unwrap()
            .starts_with("interrupted"),
        "{failed}"
    );
    assert_eq!(status_json(&p, "RUN-003")["meta"]["task"], "TASK-002");
    assert_eq!(status_json(&p, "RUN-003")["meta"]["retry_of"], "RUN-002");
    let task_001_runs = (1..=6)
        .filter(|n| {
            p.run_dir(&format!("RUN-{n:03}")).exists()
                && status_json(&p, &format!("RUN-{n:03}"))["meta"]["task"] == "TASK-001"
        })
        .count();
    assert_eq!(task_001_runs, 1);
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature: verified"));
}

/// AC-04: cancelling a handover stops its loop and the agent at work. That
/// agent call's allotment — all TASK-001 had — counts as spent (Mốc B), so
/// running again refuses TASK-001 for its budget, says so, and still runs
/// the task that does not depend on it.
#[test]
fn cancel_stops_a_background_handover() {
    let p = feature_project(3);
    let wake = p.marks.join("wake");
    p.stub(&per_task(
        &p,
        &format!(
            "[ \"$task\" = TASK-001 ] && [ ! -e {} ] && {{ echo $$ > {}/agent.pid; sleep 30; }}",
            wake.display(),
            p.marks.display()
        ),
    ));
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--async"])
        .status
        .success());
    p.wait_for("RUN-001", "attempt_started");
    let agent = p.pid_file("agent.pid");
    let loop_pid: i32 = std::fs::read_to_string(feature_dir(&p).join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    let out = p.zforge(&["run", "cancel", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        dies_within(loop_pid, Duration::from_secs(10)),
        "the loop stopped"
    );
    assert!(
        dies_within(agent, Duration::from_secs(10)),
        "its agent stopped"
    );
    assert_eq!(p.last("RUN-001")["event"], "cancelled");
    assert!(
        !p.run_dir("RUN-002").exists(),
        "nothing started after the cancel"
    );
    let out = p.zforge(&["run", "cancel", "HANDOVER-001"]);
    assert!(
        err(&out).contains("nothing of HANDOVER-001 is running"),
        "{}",
        err(&out)
    );

    // The log of the background loop is kept.
    let out = p.zforge(&["run", "log", "HANDOVER-001"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("RUN-001 — TASK-001"));

    std::fs::write(&wake, "").unwrap();
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    assert!(
        err(&out).contains("✗ TASK-001: budget of TASK-001 in HANDOVER-001 is used up"),
        "{}",
        err(&out)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("TASK-001     cancelled"), "{text}");
    assert!(text.contains("TASK-004     verified  RUN-002"), "{text}");
    assert!(!p.run_dir("RUN-003").exists());
}

/// `run wait <HANDOVER>` follows a background handover to its end.
#[test]
fn wait_follows_a_background_handover() {
    let p = feature_project(3);
    p.stub(&per_task(&p, ""));
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--async"])
        .status
        .success());
    let out = p.zforge(&["run", "wait", "HANDOVER-001", "--timeout", "120"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature: verified (RUN-005)"));
}

// ─── MOC-C TASK-006: reuse across handovers ─────────────────────────────────

/// AC-01: only the amended task runs again; what it depends on and what is
/// independent of it are reused, and the integration checks the result.
#[test]
fn an_amendment_reruns_only_what_it_touches() {
    let p = handed_over_twice(|p| {
        p.amend(
            "tasks/TASK-003.md",
            "Chỉ sửa lib.sh.",
            "Chỉ sửa lib.sh, cẩn thận.",
        )
    });
    let before = p.agent_calls();

    let out = p.zforge(&["run", "HANDOVER-002"]);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(
        p.agent_calls(),
        before + 1,
        "only TASK-003 called the agent"
    );
    assert_eq!(
        states(&p, "HANDOVER-002"),
        expect(&[
            ("TASK-001", "reused"),
            ("TASK-002", "reused"),
            ("TASK-003", "verified"),
            ("TASK-004", "reused"),
        ])
    );
    let reused = p.last("RUN-006");
    assert_eq!(reused["event"], "reused");
    assert_eq!(reused["from_run"], "RUN-001");
    assert_eq!(reused["from_handover"], "HANDOVER-001");
    assert_eq!(p.events("RUN-006").len(), 1, "no agent, no verification");
    // TASK-003 starts from the output TASK-002 reuses.
    let task2_output = status_json(&p, "RUN-002")["state"]["output"].clone();
    assert_eq!(status_json(&p, "RUN-007")["state"]["output"], task2_output);
    assert_eq!(
        status_json(&p, "RUN-008")["meta"]["start"]["commit"],
        task2_output
    );
    assert_eq!(p.last("RUN-010")["event"], "verified", "integration ran");
    let text =
        String::from_utf8_lossy(&p.zforge(&["run", "status", "HANDOVER-002"]).stdout).into_owned();
    assert!(
        text.contains("TASK-001     reused    RUN-006  reuses HANDOVER-001/RUN-001"),
        "{text}"
    );
}

/// AC-04: amending a task reruns it and everything depending on it.
#[test]
fn an_amended_dependency_reruns_its_chain() {
    let p = handed_over_twice(|p| {
        p.amend(
            "tasks/TASK-001.md",
            "Chỉ sửa lib.sh.",
            "Chỉ sửa lib.sh, cẩn thận.",
        )
    });
    let before = p.agent_calls();
    assert!(p.zforge(&["run", "HANDOVER-002"]).status.success());
    assert_eq!(p.agent_calls(), before + 3);
    assert_eq!(
        states(&p, "HANDOVER-002"),
        expect(&[
            ("TASK-001", "verified"),
            ("TASK-002", "verified"),
            ("TASK-003", "verified"),
            ("TASK-004", "reused"),
        ])
    );
}

/// AC-02: a stage is part of every task's contract.
#[test]
fn an_amended_stage_reruns_everything() {
    let p = handed_over_twice(|p| {
        p.amend(
            "03-solution.md",
            "Sửa phép tính.",
            "Sửa phép tính cẩn thận.",
        )
    });
    let before = p.agent_calls();
    assert!(p.zforge(&["run", "HANDOVER-002"]).status.success());
    assert_eq!(p.agent_calls(), before + 4);
    assert!(states(&p, "HANDOVER-002")
        .iter()
        .all(|(_, s)| s == "verified"));
}

/// AC-03: a new baseline is new code under every task.
#[test]
fn a_new_baseline_reruns_everything() {
    let p = handed_over_twice(|p| {
        std::fs::write(p.root.join("README"), "moved on\n").unwrap();
        p.git(&["add", "README"]);
        p.git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "later",
        ]);
    });
    let before = p.agent_calls();
    assert!(p.zforge(&["run", "HANDOVER-002"]).status.success());
    assert_eq!(p.agent_calls(), before + 4);
    assert!(states(&p, "HANDOVER-002")
        .iter()
        .all(|(_, s)| s == "verified"));
}

// ─── MOC-C TASK-007: three levels of implementation ─────────────────────────

/// AC-01 and AC-02: a passed integration raises its requirements; merging
/// its output into the baseline branch makes them integrated.
#[test]
fn knowledge_follows_the_integration_into_the_baseline() {
    let p = feature_project(3);
    p.stub(&per_task(&p, ""));
    assert!(p.zforge(&["run", "HANDOVER-001"]).status.success());

    let req = entry(&p.knowledge(), "REQ-001");
    assert_eq!(req["implementation"], "integration_verified");
    assert_eq!(req["integration"]["run"], "RUN-005");
    assert_eq!(req["integration"]["handover"], "HANDOVER-001");
    assert_eq!(req["integration"]["branch"], "main");
    let commit = req["integration"]["commit"].as_str().unwrap().to_string();
    assert_eq!(
        status_json(&p, "RUN-005")["state"]["output"]
            .as_str()
            .unwrap(),
        commit
    );

    merge_into_main(&p, &["--no-edit", "zforge/F/integration/RUN-005"]);
    let req = entry(&p.knowledge(), "REQ-001");
    assert_eq!(req["implementation"], "integrated");
    let out = p.zforge(&["knowledge", "index"]);
    assert!(out.status.success());
    let md = std::fs::read_to_string(p.root.join(".zforge/knowledge/index.md")).unwrap();
    assert!(
        md.contains(&format!(
            "integrated (RUN-005 of HANDOVER-001, {} in main)",
            &commit[..12]
        )),
        "{md}"
    );
}

/// AC-03: a squash merge carries the content but not the commit; knowledge
/// does not guess.
#[test]
fn a_squash_merge_is_not_taken_for_integrated() {
    let p = feature_project(3);
    p.stub(&per_task(&p, ""));
    assert!(p.zforge(&["run", "HANDOVER-001"]).status.success());
    merge_into_main(&p, &["--squash", "zforge/F/integration/RUN-005"]);
    p.git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "squashed",
    ]);
    assert_eq!(
        entry(&p.knowledge(), "REQ-001")["implementation"],
        "integration_verified"
    );
}

/// AC-04: a failed integration leaves the requirement verified by its task.
#[test]
fn a_failed_integration_leaves_the_task_level() {
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", "")],
        "```bash\nsh test.sh\nexit 1\n```",
    );
    p.stub(&fix_and_report());
    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    assert_eq!(p.last("RUN-002")["event"], "failed");
    let req = entry(&p.knowledge(), "REQ-001");
    assert_eq!(req["implementation"], "verified");
    assert_eq!(req["verified_by"], "RUN-001");
    assert!(req.get("integration").is_none());
}
