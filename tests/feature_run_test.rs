#![cfg(unix)]
//! Mốc C acceptance (MOC-C TASK-008): the five success signals of
//! `01-outcome.md`, end to end through the real binary with a stub agent,
//! plus a task with two dependencies and an amendment asked for by the agent.
//!
//! Every scenario also checks that the user's checkout is left as it was:
//! all work happens on run branches in worktrees.

#[path = "support/run_project.rs"]
mod run_project;

use run_project::*;
use std::time::Duration;

/// The user's checkout: its status and branch.
fn checkout(p: &Project) -> (String, String) {
    (
        p.git(&["status", "--porcelain"]),
        p.git(&["branch", "--show-current"]),
    )
}

fn task_of(p: &Project, run: &str) -> String {
    status_json(p, run)["meta"]["task"]
        .as_str()
        .unwrap()
        .to_string()
}

fn output_of(p: &Project, run: &str) -> serde_json::Value {
    status_json(p, run)["state"]["output"].clone()
}

/// Signal 1: A ← B ← C in one command — B from A's output, C from B's —
/// the integration check passes, and knowledge says `integration_verified`.
#[test]
fn a_chain_runs_to_a_verified_feature() {
    let p = Project::with(
        3.0,
        3,
        &[
            ("TASK-001", ""),
            ("TASK-002", "TASK-001"),
            ("TASK-003", "TASK-002"),
        ],
        "```bash\nsh test.sh\ntest -f TASK-001.done\ntest -f TASK-002.done\ntest -f TASK-003.done\n```",
    );
    p.stub(&per_task(&p, ""));
    let before = checkout(&p);

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));

    assert_eq!(
        (1..=3)
            .map(|n| task_of(&p, &format!("RUN-{n:03}")))
            .collect::<Vec<_>>(),
        ["TASK-001", "TASK-002", "TASK-003"]
    );
    let start = |run: &str| status_json(&p, run)["meta"]["start"]["commit"].clone();
    assert_eq!(start("RUN-002"), output_of(&p, "RUN-001"));
    assert_eq!(start("RUN-003"), output_of(&p, "RUN-002"));
    assert_eq!(status_json(&p, "RUN-004")["meta"]["kind"], "integration");
    assert_eq!(p.last("RUN-004")["event"], "verified");
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature: verified (RUN-004)"));
    assert_eq!(
        entry(&p.knowledge(), "REQ-001")["implementation"],
        "integration_verified"
    );
    assert_eq!(checkout(&p), before);
}

/// A task with two dependencies starts from both outputs, merged.
#[test]
fn a_task_with_two_dependencies_starts_from_both() {
    let p = Project::with(
        3.0,
        3,
        &[
            ("TASK-001", ""),
            ("TASK-002", "TASK-001"),
            ("TASK-003", ""),
            ("TASK-004", "TASK-002, TASK-003"),
        ],
        "```bash\nsh test.sh\ntest -f TASK-004.done\n```",
    );
    // TASK-004's agent records what it found where it started.
    p.stub(&per_task(
        &p,
        &format!(
            "[ \"$task\" = TASK-004 ] && ls *.done > {}/found",
            p.marks.display()
        ),
    ));
    let before = checkout(&p);

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));

    // Manifest order: TASK-001, TASK-002, TASK-003, TASK-004.
    assert_eq!(task_of(&p, "RUN-004"), "TASK-004");
    let from: Vec<String> = status_json(&p, "RUN-004")["meta"]["start"]["from"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["task"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(from, ["TASK-002", "TASK-003"]);
    let found = std::fs::read_to_string(p.marks.join("found")).unwrap();
    assert_eq!(
        found.lines().collect::<Vec<_>>(),
        ["TASK-001.done", "TASK-002.done", "TASK-003.done"]
    );
    assert_eq!(checkout(&p), before);
}

/// The stub body for an agent that asks for its contract to change the
/// first time it works on `task`, and works normally once `marks/amended`
/// exists.
fn asks_to_amend(p: &Project, task: &str) -> String {
    let body: String = [
        "Hợp đồng đang áp dụng",
        "Bằng chứng",
        "Đề xuất",
        "Tác động",
        "Cần quyết định",
    ]
    .iter()
    .map(|t| format!("## {t}\\nnội dung\\n"))
    .collect();
    per_task(
        p,
        &format!(
            "if [ \"$task\" = {task} ] && [ ! -e {m}/amended ]; then\n  \
             cr=$(grep -o '[^ `]*CHANGE-RUN-[0-9]*\\.md' {m}/prompt | head -1)\n  \
             printf '{body}' > \"$cr\"; cat {FIXTURE}; exit 0\nfi",
            m = p.marks.display()
        ),
    )
}

/// Signal 2: A is blocked; B and C, which depend on it, never run; D,
/// which does not, is verified; there is no integration.
#[test]
fn a_blocked_task_stops_only_what_depends_on_it() {
    let p = feature_project(3);
    p.stub(&asks_to_amend(&p, "TASK-001"));
    let before = checkout(&p);

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());

    let blocked = p.last("RUN-001");
    assert_eq!(blocked["event"], "blocked");
    assert_eq!(blocked["reason"], "amendment: CHANGE-RUN-001");
    assert_eq!(task_of(&p, "RUN-002"), "TASK-004");
    assert_eq!(p.last("RUN-002")["event"], "verified");
    assert!(!p.run_dir("RUN-003").exists(), "nothing else ran");
    let text = String::from_utf8_lossy(&out.stdout);
    for line in [
        "TASK-001     blocked   RUN-001 — amendment: CHANGE-RUN-001",
        "TASK-002     blocked   by TASK-001",
        "TASK-003     blocked   by TASK-001",
        "TASK-004     verified  RUN-002",
        "integration  blocked   by TASK-001",
        "feature: not verified",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }
    assert_eq!(checkout(&p), before);
}

/// Signal 3: killed while B works, run again: A does not run again, B has
/// the interrupted run and a new one, and the feature completes.
#[test]
fn a_killed_handover_resumes_without_redoing_work() {
    let p = feature_project(3);
    let wake = p.marks.join("wake");
    // Until woken, TASK-002's agent leaves a suite that hangs, so the kill
    // lands while it is verified — after its agent call was paid for.
    p.stub(&per_task(
        &p,
        &format!(
            "[ \"$task\" = TASK-002 ] && [ ! -e {} ] && {{ printf '#!/bin/sh\\necho $$ > {m}/test.pid\\nsleep 30\\n' > test.sh; cat {FIXTURE}; exit 0; }}",
            wake.display(),
            m = p.marks.display()
        ),
    ));
    let before = checkout(&p);
    assert!(p
        .zforge(&["run", "HANDOVER-001", "--async"])
        .status
        .success());
    p.wait_for("RUN-002", "verify_started");
    let suite = p.pid_file("test.pid");
    p.wait_for_recorded_child("RUN-002");
    let loop_pid: i32 = std::fs::read_to_string(feature_dir(&p).join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: the test's own background worker.
    unsafe { libc::kill(loop_pid, libc::SIGKILL) };
    assert!(dies_within(loop_pid, Duration::from_secs(5)));
    std::fs::write(&wake, "").unwrap();

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(dies_within(suite, Duration::from_secs(5)));

    let status = p.zforge(&["run", "status", "RUN-002"]);
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("failed (interrupted"),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
    assert_eq!(task_of(&p, "RUN-003"), "TASK-002");
    assert_eq!(status_json(&p, "RUN-003")["meta"]["retry_of"], "RUN-002");
    let runs_of_a = (1..=6)
        .map(|n| format!("RUN-{n:03}"))
        .filter(|r| p.run_dir(r).exists() && task_of(&p, r) == "TASK-001")
        .count();
    assert_eq!(runs_of_a, 1, "TASK-001 did not run again");
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature: verified"));
    assert_eq!(checkout(&p), before);
}

/// Signal 4: the agent on C asks for an amendment; the user amends C and
/// hands over again; A and B are reused and only C runs.
#[test]
fn an_amendment_reruns_only_the_amended_task() {
    let p = feature_project(3);
    p.stub(&asks_to_amend(&p, "TASK-003"));
    let before = checkout(&p);

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    assert_eq!(task_of(&p, "RUN-003"), "TASK-003");
    assert_eq!(p.last("RUN-003")["reason"], "amendment: CHANGE-RUN-003");
    assert!(p
        .root
        .join(".zforge/intakes/F/changes/CHANGE-RUN-003.md")
        .is_file());

    // The user decides: amend TASK-003's contract, accept it, hand over.
    p.amend(
        "tasks/TASK-003.md",
        "Chỉ sửa lib.sh.",
        "Chỉ sửa lib.sh; theo CHANGE-RUN-003.",
    );
    p.hand_over_again();
    std::fs::write(p.marks.join("amended"), "").unwrap();
    let calls = p.agent_calls();

    let out = p.zforge(&["run", "HANDOVER-002"]);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(p.agent_calls(), calls + 1, "only TASK-003 called the agent");
    let f = p.zforge(&["run", "status", "HANDOVER-002", "--json"]);
    let f: serde_json::Value = serde_json::from_slice(&f.stdout).unwrap();
    let reused: Vec<&str> = f["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t.get("reused_from").is_some())
        .map(|t| t["task"].as_str().unwrap())
        .collect();
    assert_eq!(reused, ["TASK-001", "TASK-002", "TASK-004"]);
    assert_eq!(f["integration"]["state"], "verified");
    assert_eq!(checkout(&p), before);
}

/// Signal 5: every task passes, the integration check does not: the
/// feature is not verified, and knowledge stays at the tasks' level.
#[test]
fn a_failed_integration_leaves_the_feature_unverified() {
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", ""), ("TASK-002", "TASK-001")],
        "```bash\nsh test.sh\ntest -f never-made.txt\n```",
    );
    p.stub(&per_task(&p, ""));
    let before = checkout(&p);

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());

    assert_eq!(p.last("RUN-001")["event"], "verified");
    assert_eq!(p.last("RUN-002")["event"], "verified");
    let failed = p.last("RUN-003");
    assert_eq!(failed["event"], "failed");
    assert!(
        failed["reason"]
            .as_str()
            .unwrap()
            .contains("`test -f never-made.txt` failed"),
        "{failed}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature: not verified"));
    let req = entry(&p.knowledge(), "REQ-001");
    assert_eq!(req["implementation"], "verified");
    assert!(req.get("integration").is_none());
    assert_eq!(checkout(&p), before);
}
