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
    // `p.knowledge()` above writes `docs/knowledge/commitments.md`
    // (CHANGE-RUN-009): the checkout otherwise stays as it was.
    let after = checkout(&p);
    assert_eq!(after.1, before.1);
    let untracked: Vec<&str> = after
        .0
        .lines()
        .filter(|l| l.trim_start_matches("?? ").starts_with("docs/"))
        .collect();
    assert_eq!(untracked, vec!["?? docs/"], "{}", after.0);
    let rest: Vec<&str> = after
        .0
        .lines()
        .filter(|l| !l.trim_start_matches("?? ").starts_with("docs/"))
        .collect();
    assert_eq!(rest, before.0.lines().collect::<Vec<_>>());
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
    // `p.knowledge()` above writes `docs/knowledge/commitments.md`
    // (CHANGE-RUN-009): the checkout otherwise stays as it was.
    let after = checkout(&p);
    assert_eq!(after.1, before.1);
    let untracked: Vec<&str> = after
        .0
        .lines()
        .filter(|l| l.trim_start_matches("?? ").starts_with("docs/"))
        .collect();
    assert_eq!(untracked, vec!["?? docs/"], "{}", after.0);
    let rest: Vec<&str> = after
        .0
        .lines()
        .filter(|l| !l.trim_start_matches("?? ").starts_with("docs/"))
        .collect();
    assert_eq!(rest, before.0.lines().collect::<Vec<_>>());
}

/// `zforge status` follows a feature: before the run it says to run the
/// handover, after it that the handover is verified.
#[test]
fn status_follows_the_feature() {
    let p = Project::with(3.0, 3, &[("TASK-001", "")], "sh test.sh");
    p.stub(&per_task(&p, ""));
    let status = || -> serde_json::Value {
        let out = p.zforge(&["status", "--json"]);
        assert!(out.status.success(), "{}", err(&out));
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let s = status();
    let i = &s["intakes"][0];
    assert!(
        i["files"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == "accepted"),
        "{i}"
    );
    assert_eq!(i["handovers"][0]["tasks"]["TASK-001"], "ready");
    assert!(i["next"].as_str().unwrap().contains("zforge run"), "{i}");

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));

    let s = status();
    let h = &s["intakes"][0]["handovers"][0];
    assert_eq!(h["tasks"]["TASK-001"], "verified");
    assert_eq!(h["integration"], "verified");
    assert!(
        s["intakes"][0]["next"]
            .as_str()
            .unwrap()
            .contains("is verified"),
        "{s}"
    );
    let text = String::from_utf8_lossy(&p.zforge(&["status"]).stdout).into_owned();
    assert!(text.contains("TASK-001 verified"), "{text}");
}

/// Two tasks that do not depend on each other change the same file: their
/// outputs would not merge. The loop runs the later one again on top of
/// the earlier one's output, telling its agent why, and the integration
/// then merges and passes.
#[test]
fn conflicting_siblings_are_stacked_before_the_integration() {
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", ""), ("TASK-002", "")],
        "```bash\nsh test.sh\ngrep -qx TASK-001 shared.txt\ngrep -qx TASK-002 shared.txt\n```",
    );
    // Each task writes its name into shared.txt; told it starts on top of
    // another output, it adds its line instead.
    p.stub(&per_task(
        &p,
        &format!(
            "if grep -q 'does not merge with the output of' {m}/prompt; then\n  \
             echo \"$task\" >> shared.txt\nelse\n  echo \"$task\" > shared.txt\nfi",
            m = p.marks.display()
        ),
    ));
    let before = checkout(&p);

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        err(&out).contains("TASK-002's output does not merge with TASK-001's (shared.txt)"),
        "{}",
        err(&out)
    );

    assert_eq!(task_of(&p, "RUN-001"), "TASK-001");
    assert_eq!(task_of(&p, "RUN-002"), "TASK-002");
    assert_eq!(task_of(&p, "RUN-003"), "TASK-002");
    let start = &status_json(&p, "RUN-003")["meta"]["start"];
    assert_eq!(start["rebased"]["onto"], serde_json::json!(["TASK-001"]));
    assert_eq!(start["rebased"]["files"], serde_json::json!(["shared.txt"]));
    assert_eq!(start["rebased"]["replaces"]["run"], "RUN-002");
    let prompt = std::fs::read_to_string(p.marks.join("prompt")).unwrap();
    assert!(prompt.contains("RUN-002"), "{prompt}");
    assert_eq!(p.last("RUN-003")["event"], "verified");

    assert_eq!(task_of(&p, "RUN-004"), "integration");
    assert_eq!(p.last("RUN-004")["event"], "verified");
    assert!(!p.run_dir("RUN-005").exists(), "nothing more");
    assert_eq!(checkout(&p), before);
}

/// A task depending on two conflicting outputs (as RUN-037 in the field):
/// the later dependency is stacked on the earlier first, then the task
/// starts from both.
#[test]
fn a_task_on_conflicting_dependencies_waits_for_them_to_be_stacked() {
    let p = Project::with(
        3.0,
        3,
        &[
            ("TASK-001", ""),
            ("TASK-002", ""),
            ("TASK-003", "TASK-001, TASK-002"),
        ],
        "sh test.sh",
    );
    p.stub(&per_task(
        &p,
        &format!(
            "if grep -q 'does not merge with the output of' {m}/prompt; then\n  \
             echo \"$task\" >> shared.txt\nelif [ \"$task\" != TASK-003 ]; then\n  \
             echo \"$task\" > shared.txt\nfi",
            m = p.marks.display()
        ),
    ));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));

    let tasks: Vec<String> = (1..=5)
        .map(|n| task_of(&p, &format!("RUN-00{n}")))
        .collect();
    assert_eq!(
        tasks,
        [
            "TASK-001",
            "TASK-002",
            "TASK-002",
            "TASK-003",
            "integration"
        ]
    );
    let from: Vec<String> = status_json(&p, "RUN-004")["meta"]["start"]["from"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["run"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(from, ["RUN-001", "RUN-003"]);
    assert_eq!(p.last("RUN-005")["event"], "verified");
}

/// An integration check failing on the environment — its output names
/// Docker — is run once more and, passing, verifies the feature; the retry
/// is in `checks.log`.
#[test]
fn an_integration_check_failing_on_the_environment_is_retried_once() {
    let mark = tempfile::tempdir().unwrap();
    let flag = mark.path().join("docker-up");
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", "")],
        &format!(
            "```bash\nsh test.sh\ntest -f {f} || {{ touch {f}; echo 'Cannot connect to the Docker daemon at unix:///var/run/docker.sock' >&2; exit 1; }}\n```",
            f = flag.display()
        ),
    );
    p.stub(&per_task(&p, ""));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(p.last("RUN-001")["event"], "verified");
    assert_eq!(p.last("RUN-002")["event"], "verified");
    assert_eq!(p.count("RUN-002", "verify_failed"), 0);
    let log = std::fs::read_to_string(p.run_dir("RUN-002").join("checks.log")).unwrap();
    assert!(
        log.contains("matches infrastructure pattern \"Cannot connect to the Docker daemon\""),
        "{log}"
    );
    assert!(log.contains("passed on retry"), "{log}");
}

/// Failing on the environment twice ends the integration with an `infra: `
/// reason; it is not a verdict, so running the handover again retries it.
#[test]
fn an_integration_failing_on_the_environment_twice_is_retried_by_the_next_run() {
    let mark = tempfile::tempdir().unwrap();
    let n = mark.path().join("attempts");
    let p = Project::with(
        3.0,
        3,
        &[("TASK-001", "")],
        &format!(
            "```bash\nn=$(cat {n} 2>/dev/null || echo 0); n=$((n+1)); echo $n > {n}; [ $n -ge 3 ] || {{ echo 'dial tcp 127.0.0.1:5432: connect: connection refused' >&2; exit 1; }}\n```",
            n = n.display()
        ),
    );
    p.stub(&per_task(&p, ""));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(!out.status.success());
    let failed = p.last("RUN-002");
    assert_eq!(failed["event"], "failed");
    let reason = failed["reason"].as_str().unwrap();
    assert!(reason.starts_with("infra: "), "{reason}");
    assert!(reason.contains("connection refused"), "{reason}");
    assert_eq!(std::fs::read_to_string(&n).unwrap().trim(), "2");

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(p.agent_calls(), 1, "the task is not run again");
    assert_eq!(status_json(&p, "RUN-003")["meta"]["retry_of"], "RUN-002");
    assert_eq!(p.last("RUN-003")["event"], "verified");
}

/// Without a block under "Integration verification", the integration runs
/// `project.integration_command` before falling back to the test command.
#[test]
fn the_project_integration_command_stands_in_for_a_missing_block() {
    let mark = tempfile::tempdir().unwrap();
    let ran = mark.path().join("ran");
    let p = Project::bare(3.0, 3);
    let config = p.root.join(".zforge/config.yaml");
    let text = std::fs::read_to_string(&config).unwrap().replace(
        "  test_command: \"sh test.sh\"\n",
        &format!(
            "  test_command: \"sh test.sh\"\n  integration_command: \"sh test.sh && touch {}\"\n",
            ran.display()
        ),
    );
    std::fs::write(&config, text).unwrap();
    p.hand_over(&[("TASK-001", "")], "Chạy toàn bộ.");
    p.stub(&per_task(&p, ""));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    let meta = &status_json(&p, "RUN-002")["meta"];
    assert_eq!(meta["checks"]["from"], "integration_command");
    assert!(ran.exists());
}

/// A tiny intake (`intake new --tiny`) runs like any other: its standard
/// 04-breakdown has no code block, so the integration check is the
/// project's test command.
#[test]
fn a_tiny_intake_runs_to_a_verified_feature() {
    use zforge::intake::templates::tiny_stage;
    let outcome = tiny_stage("01-outcome.md", "F")
        .unwrap()
        .replace("## Summary\n", "## Summary\nadd returns the sum.\n")
        .replace(
            "## Requirements\n",
            "## Requirements\n\n- REQ-001: add returns the sum.\n",
        );
    let mut files = vec![("01-outcome.md", outcome)];
    for stage in ["02-behavior.md", "03-solution.md", "04-breakdown.md"] {
        files.push((stage, tiny_stage(stage, "F").unwrap()));
    }
    files.push(("tasks/TASK-001.md", task("TASK-001", "")));
    let p = Project::with_files(3.0, 3, &files);
    p.stub(&per_task(&p, ""));

    let out = p.zforge(&["run", "HANDOVER-001"]);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(
        states(&p, "HANDOVER-001"),
        expect(&[("TASK-001", "verified")])
    );
    assert_eq!(status_json(&p, "RUN-002")["meta"]["kind"], "integration");
    assert_eq!(p.last("RUN-002")["event"], "verified");
}
