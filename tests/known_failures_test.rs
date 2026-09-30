#![cfg(unix)]
//! Known failures in run verification (ONBOARD TASK-011; REQ-010, business
//! rule 8), end to end through the real binary with a stub agent that
//! changes nothing: a run whose test command fails only pinned known tests
//! still verifies, tolerating them; one that also fails an unknown test
//! still fails, naming only that one; unreadable output still fails
//! regardless of the known list; and a handover with no known failures
//! carries neither new field at all.

#[path = "support/run_project.rs"]
mod run_project;

use run_project::*;

/// A test command whose output the "rust" parser can read: `test.sh` is
/// replaced (and committed, so the run's worktree sees it) with `script`,
/// the project's language is switched to "rust" so `run_with_language` can
/// parse test names out of it, `known` is recorded as the project's
/// known-failure list, and the intake is handed over again so the new
/// handover (`HANDOVER-002`) pins that list against the updated baseline
/// (TASK-006).
fn known_project(known: &[&str], script: &str) -> Project {
    let p = Project::with(3.0, 2, &[("TASK-001", "")], "sh test.sh");
    std::fs::write(p.root.join("test.sh"), script).unwrap();
    p.git(&["add", "-A"]);
    p.git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "known-setup",
    ]);

    let config_path = p.root.join(".zforge/config.yaml");
    let text = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        text.replace("language: shell", "language: rust"),
    )
    .unwrap();

    let config = zforge::config::load_from(&config_path).unwrap();
    let k = zforge::knowledge::Knowledge::open(&config);
    std::fs::create_dir_all(&k.dir).unwrap();
    zforge::knowledge::known::record_known(
        &k,
        &known.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        None,
    )
    .unwrap();
    p.hand_over_again();
    p
}

const FAILS_TEST_A: &str =
    "#!/bin/sh\necho \"test TestA ... FAILED\"\necho \"test result: FAILED.\"\nexit 1\n";

const FAILS_TEST_A_AND_LOGIN: &str = "#!/bin/sh\necho \"test TestA ... FAILED\"\necho \"test TestLogin ... FAILED\"\necho \"test result: FAILED.\"\nexit 1\n";

const UNREADABLE_FAILURE: &str = "#!/bin/sh\necho boom\nexit 1\n";

/// AC-01 and AC-04: a run failing only known tests verifies, recording what
/// it tolerated and which known test it found not failing; `zforge status`
/// then suggests dropping the recovered one.
#[test]
fn tolerates_known_failures_and_flags_a_recovered_one() {
    let p = known_project(&["TestA", "TestB"], FAILS_TEST_A);
    p.stub(&format!("cat {FIXTURE}"));

    let out = p.zforge(&["run", "HANDOVER-002", "--task", "TASK-001"]);
    assert!(out.status.success(), "{}", err(&out));

    let verified = p.last("RUN-001");
    assert_eq!(verified["event"], "verified");
    assert_eq!(verified["tolerated"], serde_json::json!(["TestA"]));
    assert_eq!(verified["known_passing"], serde_json::json!(["TestB"]));

    let text = String::from_utf8_lossy(&p.zforge(&["status"]).stdout).into_owned();
    assert!(text.contains("TestB"), "{text}");
    assert!(text.contains("known failures now passing"), "{text}");
}

/// AC-02: a run also failing a test that is not known still fails, and
/// names only that test — both in why it failed and in the feedback the
/// next attempt's prompt carries.
#[test]
fn fails_naming_only_the_unknown_test() {
    let p = known_project(&["TestA"], FAILS_TEST_A_AND_LOGIN);
    p.stub(&format!("cat {FIXTURE}"));

    let out = p.zforge(&["run", "HANDOVER-002", "--task", "TASK-001"]);
    assert!(!out.status.success());

    let failed = p.last("RUN-001");
    assert_eq!(failed["event"], "failed");
    let reason = failed["reason"].as_str().unwrap();
    assert!(reason.contains("TestLogin"), "{failed}");
    assert!(!reason.contains("TestA"), "{failed}");
    assert_eq!(p.count("RUN-001", "attempt"), 2, "both attempts ran");

    // The second attempt's prompt carries the first verification's
    // feedback: the `Failing:` summary names TestLogin only (the raw test
    // output further down still shows everything, for context).
    let prompt = std::fs::read_to_string(p.marks.join("prompt")).unwrap();
    let failing_line = prompt
        .lines()
        .find(|l| l.starts_with("Failing:"))
        .unwrap_or_else(|| panic!("no `Failing:` line in the prompt:\n{prompt}"));
    assert_eq!(failing_line, "Failing: TestLogin");
}

/// AC-03: output the runner cannot read any failing test name from still
/// fails, known list or not — a known list never turns silence into a pass.
#[test]
fn unreadable_output_still_fails() {
    let p = known_project(&["TestA"], UNREADABLE_FAILURE);
    p.stub(&format!("cat {FIXTURE}"));

    let out = p.zforge(&["run", "HANDOVER-002", "--task", "TASK-001"]);
    assert!(!out.status.success());
    assert_eq!(p.last("RUN-001")["event"], "failed");
}

/// AC-05: a handover with no known failures verifies exactly as before —
/// the `verified` event carries neither `tolerated` nor `known_passing`.
#[test]
fn no_known_failures_means_no_new_fields_on_the_event() {
    let p = Project::new(3.0, 3);
    p.stub(&fix_and_report());

    let out = p.zforge(&["run", "HANDOVER-001", "--task", "TASK-001"]);
    assert!(out.status.success(), "{}", err(&out));

    let verified = p.last("RUN-001");
    assert_eq!(verified["event"], "verified");
    assert!(verified.get("tolerated").is_none(), "{verified}");
    assert!(verified.get("known_passing").is_none(), "{verified}");
}
