//! v1.5 intake through the real binary (Mốc A, decisions D1/D2/D4).
//!
//! The binary runs without a terminal, exactly as an agent does, so it can
//! create, fill and review files but must refuse to accept them. The user's
//! acceptance is exercised through the library: there is deliberately no
//! environment switch that would let a test — or an agent — accept through
//! the CLI without a TTY.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use zforge::intake::record::Decider;

const OUTCOME: &str = "# F-1 — Outcome\n\n## Vấn đề\nCần lọc.\n\n## Yêu cầu\n\n- REQ-001: lọc theo trạng thái\n\n## Câu hỏi còn mở\n\n- [ ] trạng thái archived có tính không?\n";

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();
        let p = Self { _dir: dir, root };
        p.register_runner("sh");
        p
    }

    /// Register `claude` as `command` in this project's agent registry.
    fn register_runner(&self, command: &str) {
        std::fs::create_dir_all(self.root.join(".home")).unwrap();
        std::fs::write(
            self.root.join(".home/registry.yaml"),
            format!("agents:\n  claude:\n    command: {command}\n"),
        )
        .unwrap();
    }

    fn zforge(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zforge"))
            .args(args)
            .current_dir(&self.root)
            .env("ZFORGE_HOME", self.root.join(".home"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.zforge(args);
        assert!(
            out.status.success(),
            "zforge {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn intake_dir(&self) -> PathBuf {
        self.root.join(".zforge/intakes/F-1")
    }

    fn status(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["intake", "status", "F-1", "--json"])).unwrap()
    }

    fn state_of(&self, file: &str) -> String {
        self.status()
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == file)
            .map(|f| f["state"].as_str().unwrap().to_string())
            .unwrap()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn an_agent_can_prepare_and_review_but_not_accept() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    for stage in [
        "01-outcome.md",
        "02-behavior.md",
        "03-solution.md",
        "04-breakdown.md",
    ] {
        assert!(p.intake_dir().join(stage).is_file(), "{stage}");
    }

    // An untouched template is not reviewable.
    let out = p.zforge(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("no content yet"), "{}", stderr(&out));

    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    let out = p.ok(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(out.contains("revision 1"), "{out}");
    assert_eq!(p.state_of("01-outcome.md"), "in_review");

    // The decision needs a human at a terminal.
    let out = p.zforge(&["intake", "accept", "F-1", "01-outcome.md"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("needs an interactive terminal"),
        "{}",
        stderr(&out)
    );
    let out = p.zforge(&["intake", "revise", "F-1", "01-outcome.md", "--note", "x"]);
    assert!(!out.status.success());
    assert_eq!(
        p.state_of("01-outcome.md"),
        "in_review",
        "nothing was recorded"
    );

    let status = p.status();
    let outcome = status
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == "01-outcome.md")
        .unwrap();
    assert_eq!(
        outcome["open_questions"][0],
        "trạng thái archived có tính không?"
    );
}

/// D2 through the files the CLI shows: acceptance binds to the reviewed
/// hash, and an edit afterwards is a draft over the accepted revision.
#[test]
fn acceptance_binds_to_the_reviewed_revision() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    p.ok(&["intake", "review", "F-1", "01-outcome.md"]);

    let intake = zforge::intake::Intake::open(&p.root, "F-1").unwrap();
    zforge::intake::review::accept(&intake, "01-outcome.md", Some("user".into())).unwrap();
    assert_eq!(p.state_of("01-outcome.md"), "accepted");

    std::fs::write(
        p.intake_dir().join("01-outcome.md"),
        OUTCOME.replace("lọc theo trạng thái", "lọc theo trạng thái và nhãn"),
    )
    .unwrap();
    let st = p.status();
    let outcome = st
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == "01-outcome.md")
        .unwrap();
    assert_eq!(outcome["state"], "accepted");
    assert_eq!(outcome["has_draft"], true);
    assert_eq!(outcome["accepted"]["revision"], 1);

    let out = p.ok(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(
        out.contains("revision 2") && out.contains("+- REQ-001: lọc theo trạng thái và nhãn"),
        "{out}"
    );

    // Edited again before the decision: the review no longer describes it.
    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    assert_eq!(p.state_of("01-outcome.md"), "changed_since_review");
    let err = zforge::intake::review::accept(&intake, "01-outcome.md", None).unwrap_err();
    assert!(
        err.to_string().contains("changed after revision 2"),
        "{err}"
    );
}

#[test]
fn tasks_are_linted_against_the_outcome() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    std::fs::write(p.intake_dir().join("01-outcome.md"), OUTCOME).unwrap();
    p.ok(&["intake", "task", "F-1", "TASK-001"]);

    let out = p.zforge(&["intake", "review", "F-1", "tasks/TASK-001.md"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("links to no requirement"), "{err}");
    assert!(err.contains("section \"Goal\" is empty"), "{err}");

    let out = p.zforge(&["intake", "review", "F-1", "../../config.yaml"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("is not an intake file"));
}

// ─── readiness and handover (§6.2, §6.3) ────────────────────────────────────

const BEHAVIOR: &str =
    "# F-1 — Behavior\n\n## Tình huống\nREQ-001: status=open trả task open.\n\n## Câu hỏi còn mở\n";
const SOLUTION: &str =
    "# F-1 — Solution\n\n## Luồng xử lý\nLọc trong listing.\n\n## Câu hỏi còn mở\n";
const BREAKDOWN: &str = "# F-1 — Breakdown\n\n## Task và dependency\nTASK-001 rồi TASK-002.\n\n## Kiểm chứng tích hợp\nChạy toàn bộ test listing trên cùng tree.\n\n## Câu hỏi còn mở\n";

fn task(id: &str, reqs: &str, deps: &str) -> String {
    format!(
        "---\nid: {id}\nparent: F-1\nrequirements: [{reqs}]\ndepends_on: [{deps}]\n---\n\n# {id}\n\n\
         ## Mục tiêu\nLọc.\n## Input\nAPI.\n## Output\nDanh sách.\n## Ràng buộc\nGiữ shape.\n\
         ## Tự chủ\nTự chọn hàm.\n## Acceptance và kiểm chứng\n- AC-01: lọc đúng\n## Bàn giao\nLocal.\n\
         ## Cần amendment khi\nĐổi shape.\n## Câu hỏi còn mở\n"
    )
}

impl Project {
    /// Everything written, reviewed and accepted, in a git repo with a
    /// budget configured.
    fn ready_intake(&self) -> zforge::intake::Intake {
        std::fs::write(
            self.root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\nexecution:\n  budget_usd: 2.5\n",
        )
        .unwrap();
        for args in [
            &["init", "-q", "."][..],
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "base",
            ],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&self.root)
                .status()
                .unwrap()
                .success());
        }
        self.ok(&["intake", "new", "F-1"]);
        let d = self.intake_dir();
        std::fs::write(
            d.join("01-outcome.md"),
            OUTCOME.replace("- [ ] trạng thái", "- [x] trạng thái"),
        )
        .unwrap();
        std::fs::write(d.join("02-behavior.md"), BEHAVIOR).unwrap();
        std::fs::write(d.join("03-solution.md"), SOLUTION).unwrap();
        std::fs::write(d.join("04-breakdown.md"), BREAKDOWN).unwrap();
        std::fs::write(d.join("tasks/TASK-001.md"), task("TASK-001", "REQ-001", "")).unwrap();
        std::fs::write(
            d.join("tasks/TASK-002.md"),
            task("TASK-002", "REQ-001", "TASK-001"),
        )
        .unwrap();
        let intake = zforge::intake::Intake::open(&self.root, "F-1").unwrap();
        for f in intake.files() {
            self.accept(&f);
        }
        intake
    }

    fn accept(&self, file: &str) {
        self.ok(&["intake", "review", "F-1", file]);
        let intake = zforge::intake::Intake::open(&self.root, "F-1").unwrap();
        zforge::intake::review::accept(&intake, file, None).unwrap();
    }

    fn readiness(&self, extra: &[&str]) -> (bool, serde_json::Value) {
        let mut args = vec!["readiness", "F-1", "--json"];
        args.extend_from_slice(extra);
        let out = self.zforge(&args);
        (
            out.status.success(),
            serde_json::from_slice(&out.stdout).unwrap(),
        )
    }
}

fn failing(report: &serde_json::Value) -> Vec<String> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["ok"] == false)
        .flat_map(|c| {
            c["problems"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap().to_string())
        })
        .collect()
}

#[test]
fn a_fully_accepted_intake_is_ready_and_hands_over_what_was_accepted() {
    let p = Project::new();
    let intake = p.ready_intake();

    let (ok, r) = p.readiness(&[]);
    assert!(ok, "{:?}", failing(&r));
    assert_eq!(r["tasks"], serde_json::json!(["TASK-001", "TASK-002"]));
    assert_eq!(r["files"].as_array().unwrap().len(), 6);
    assert!(p.intake_dir().join("readiness.md").is_file());

    // The handover itself needs a human at a terminal.
    let out = p.zforge(&["handover", "F-1"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("needs an interactive terminal"));

    let config = zforge::config::load_from(&p.root.join(".zforge/config.yaml")).unwrap();
    let pinned: Vec<zforge::intake::readiness::Pinned> =
        serde_json::from_value(r["files"].clone()).unwrap();
    let m =
        zforge::intake::handover::create(&intake, &p.root, &[], &config, &pinned, None).unwrap();
    assert_eq!(m.id, "HANDOVER-001");
    assert_eq!(m.tasks, ["TASK-001", "TASK-002"]);
    assert_eq!(m.baseline.branch, "main");
    assert_eq!(m.baseline.commit.len(), 40);
    assert_eq!(m.policy.budget_usd, 2.5);
    assert_eq!(m.files, pinned);

    // Accepting a new revision after readiness was shown: refused. (A leaf
    // task, so nothing below it goes stale and readiness itself still passes.)
    let leaf = p.intake_dir().join("tasks/TASK-002.md");
    let text = std::fs::read_to_string(&leaf).unwrap();
    std::fs::write(
        &leaf,
        text.replacen("## Mục tiêu\n", "## Mục tiêu\nRõ hơn.\n", 1),
    )
    .unwrap();
    p.accept("tasks/TASK-002.md");
    let err = zforge::intake::handover::create(&intake, &p.root, &[], &config, &pinned, None)
        .unwrap_err();
    assert!(
        err.to_string().contains("changed after they were shown"),
        "{err}"
    );
}

#[test]
fn readiness_names_each_reason_it_is_not_ready() {
    let p = Project::new();
    p.ready_intake();

    // A draft over an accepted file, an open question, an uncovered
    // requirement and a dependency outside the scope.
    let d = p.intake_dir();
    std::fs::write(
        d.join("02-behavior.md"),
        format!("{BEHAVIOR}- [ ] còn lỗi?\n"),
    )
    .unwrap();
    std::fs::write(
        d.join("01-outcome.md"),
        OUTCOME
            .replace("- [ ] trạng thái", "- [x] trạng thái")
            .replace(
                "- REQ-001: lọc theo trạng thái\n",
                "- REQ-001: lọc theo trạng thái\n- REQ-002: giữ thứ tự\n",
            )
            + "- [ ] phân quyền?\n",
    )
    .unwrap();
    p.accept("01-outcome.md");

    let (ok, r) = p.readiness(&[]);
    assert!(!ok);
    let problems = failing(&r).join("\n");
    for want in [
        "02-behavior.md has changes after accepted revision 1 (draft)",
        "01-outcome.md: phân quyền?",
        "REQ-002 has no task",
    ] {
        assert!(problems.contains(want), "missing {want:?} in:\n{problems}");
    }

    let (_, r) = p.readiness(&["--task", "TASK-002"]);
    assert!(failing(&r)
        .join("\n")
        .contains("TASK-002 depends on TASK-001, which is outside the handover scope"));

    std::fs::write(p.root.join(".zforge/config.yaml"), "project:\n  name: t\n").unwrap();
    let (_, r) = p.readiness(&[]);
    assert!(failing(&r).join("\n").contains("no budget"));
}

/// §9: the index comes from accepted revisions only, and keeps the
/// decision and implementation statuses apart.
#[test]
fn knowledge_index_follows_accepted_revisions_and_handovers() {
    let p = Project::new();
    let intake = p.ready_intake();
    let index = |p: &Project| -> serde_json::Value {
        serde_json::from_str(&p.ok(&["knowledge", "index", "--json"])).unwrap()
    };
    let entry = |v: &serde_json::Value, id: &str| {
        v.as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .cloned()
            .unwrap()
    };

    let v = index(&p);
    let req = entry(&v, "REQ-001");
    assert_eq!(req["decision"], "active");
    assert_eq!(req["implementation"], "not_implemented");
    assert_eq!(req["tasks"], serde_json::json!(["TASK-001", "TASK-002"]));
    assert_eq!(req["source"], "01-outcome.md");
    assert!(p.root.join(".zforge/knowledge/commitments.json").is_file());
    assert!(p.root.join("docs/knowledge/commitments.md").is_file());

    let config = zforge::config::load_from(&p.root.join(".zforge/config.yaml")).unwrap();
    let (_, r) = p.readiness(&[]);
    let pinned: Vec<zforge::intake::readiness::Pinned> =
        serde_json::from_value(r["files"].clone()).unwrap();
    zforge::intake::handover::create(&intake, &p.root, &[], &config, &pinned, None).unwrap();
    assert_eq!(
        entry(&index(&p), "REQ-001")["implementation"],
        "handed_over"
    );

    // A requirement dropped by a later accepted revision is superseded,
    // citing the last revision that had it; a draft changes nothing.
    let outcome = p.intake_dir().join("01-outcome.md");
    let base = std::fs::read_to_string(&outcome).unwrap();
    std::fs::write(
        &outcome,
        base.replace(
            "- REQ-001: lọc theo trạng thái\n",
            "- REQ-001: lọc theo trạng thái\n- REQ-009: tạm\n",
        ),
    )
    .unwrap();
    p.accept("01-outcome.md");
    std::fs::write(&outcome, &base).unwrap();
    p.accept("01-outcome.md");
    std::fs::write(&outcome, base.replace("lọc theo", "DRAFT")).unwrap();

    let v = index(&p);
    let dropped = entry(&v, "REQ-009");
    assert_eq!(dropped["decision"], "superseded");
    assert_eq!(dropped["revision"], 2);
    assert_eq!(
        entry(&v, "REQ-001")["text"],
        "REQ-001: lọc theo trạng thái",
        "not the draft"
    );
}

/// Workflow §13: what a run needs on this machine is checked before the
/// handover, naming what is missing.
#[test]
fn readiness_names_a_missing_runner() {
    let p = Project::new();
    p.ready_intake();
    p.register_runner("no-such-claude-binary");
    let (ok, r) = p.readiness(&[]);
    assert!(!ok);
    assert_eq!(
        failing(&r),
        ["runner `claude` runs `no-such-claude-binary`, which is not installed or not on PATH"]
    );

    std::fs::write(p.root.join(".home/registry.yaml"), "agents: {}\n").unwrap();
    let (ok, r) = p.readiness(&[]);
    assert!(!ok);
    assert_eq!(
        failing(&r),
        ["runner `claude` is not in the agent registry; run `zforge init --agent claude`"]
    );
}

/// D6: a file accepted before something it rests on changed is not handed
/// over until the user confirms it again — which they can, unchanged. This
/// solution has no binding decisions section, so it is compared whole.
#[test]
fn a_file_resting_on_a_changed_upstream_is_confirmed_again() {
    let p = Project::new();
    p.ready_intake();
    std::fs::write(
        p.intake_dir().join("03-solution.md"),
        SOLUTION.replace("listing", "listing module"),
    )
    .unwrap();
    p.accept("03-solution.md");

    let (ok, r) = p.readiness(&[]);
    assert!(!ok);
    let problems = failing(&r);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].starts_with(
        "04-breakdown.md rests on 03-solution.md, changed in revision 2 (not readable as items, so compared whole); review it again"
    ));

    // Unchanged, but stale: it may be reviewed and accepted again. Its
    // hash does not change; its revision does.
    let before = p.status();
    p.accept("04-breakdown.md");
    let breakdown = |v: &serde_json::Value| {
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == "04-breakdown.md")
            .unwrap()["accepted"]
            .clone()
    };
    let (old, new) = (breakdown(&before), breakdown(&p.status()));
    assert_eq!(old["sha256"], new["sha256"]);
    assert_eq!(new["revision"], old["revision"].as_u64().unwrap() + 1);
    // The tasks rest on the breakdown too, so they follow.
    for t in ["tasks/TASK-001.md", "tasks/TASK-002.md"] {
        p.accept(t);
    }
    let (ok, r) = p.readiness(&[]);
    assert!(ok, "{:?}", failing(&r));

    // A file that is not stale still cannot be "reviewed" unchanged.
    let out = p.zforge(&["intake", "review", "F-1", "tasks/TASK-001.md"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("already accepted"),
        "{}",
        stderr(&out)
    );
}

/// `intake amend`: a new intake whose files are drafts copied from the
/// source's accepted revisions — not its working files — marked with what
/// they amend; nothing is accepted for the user.
#[test]
fn amend_starts_a_new_intake_from_the_accepted_revisions() {
    let p = Project::new();
    p.ready_intake();
    // A draft on top of the accepted outcome is not what gets copied.
    let outcome = p.intake_dir().join("01-outcome.md");
    let accepted = std::fs::read_to_string(&outcome).unwrap();
    std::fs::write(&outcome, accepted.replace("lọc theo", "DRAFT lọc theo")).unwrap();

    let out = p.ok(&["intake", "amend", "G-1", "--from", "F-1"]);
    assert!(out.contains("created G-1, amending F-1"), "{out}");
    let g = p.root.join(".zforge/intakes/G-1");
    let copy = std::fs::read_to_string(g.join("01-outcome.md")).unwrap();
    assert!(
        copy.starts_with("# G-1 — Outcome\n\n<!-- amends: F-1."),
        "{copy}"
    );
    assert!(copy.contains("- REQ-001: lọc theo trạng thái") && !copy.contains("DRAFT"));
    let task = std::fs::read_to_string(g.join("tasks/TASK-002.md")).unwrap();
    assert!(task.contains("parent: G-1\namends: F-1\n"), "{task}");
    assert!(task.contains("depends_on: [TASK-001]"));
    let brief = std::fs::read_to_string(g.join("brief.md")).unwrap();
    assert!(
        brief.contains("Amends F-1: write only what changes."),
        "{brief}"
    );

    // Drafts, every one: the user still accepts them.
    let status: serde_json::Value =
        serde_json::from_str(&p.ok(&["intake", "status", "G-1", "--json"])).unwrap();
    let states: Vec<&str> = status
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["state"].as_str().unwrap())
        .collect();
    assert_eq!(states.len(), 6);
    assert!(states.iter().all(|s| *s == "draft"), "{states:?}");
    // And they pass review as they are (the parent is the new intake).
    for f in [
        "01-outcome.md",
        "02-behavior.md",
        "03-solution.md",
        "04-breakdown.md",
        "tasks/TASK-001.md",
        "tasks/TASK-002.md",
    ] {
        p.ok(&["intake", "review", "G-1", f]);
    }

    // Amending into an existing intake, or from one with nothing accepted,
    // is refused.
    assert!(!p
        .zforge(&["intake", "amend", "G-1", "--from", "F-1"])
        .status
        .success());
    assert!(!p
        .zforge(&["intake", "amend", "H-1", "--from", "G-1"])
        .status
        .success());
}

/// `intake amend --fixup`: compact stages referring to the source's
/// accepted revisions and one task stub; once the fixes are written they
/// lint clean and go through review, acceptance, readiness and handover
/// like any intake — all four stages pinned.
#[test]
fn a_fixup_amend_is_short_and_goes_through_the_whole_flow() {
    let p = Project::new();
    p.ready_intake();
    let out = p.ok(&["intake", "amend", "G-1", "--from", "F-1", "--fixup"]);
    assert!(out.contains("created G-1, a fixup of F-1"), "{out}");
    let g = p.root.join(".zforge/intakes/G-1");
    let read = |f: &str| std::fs::read_to_string(g.join(f)).unwrap();

    let outcome = read("01-outcome.md");
    assert!(
        outcome.starts_with("---\nprofile: fixup\namends: F-1\n---\n"),
        "{outcome}"
    );
    assert!(zforge::intake::lint::is_small(&outcome));
    let behavior = read("02-behavior.md");
    assert!(
        behavior.starts_with("# G-1 — Behavior\n\nAs accepted in F-1 (revision 1), except:"),
        "{behavior}"
    );
    assert!(behavior.contains(".zforge/intakes/F-1/.records/revisions/02-behavior/1.md"));
    assert!(read("03-solution.md").contains("As accepted in F-1 (revision 1), except:"));
    assert!(read("04-breakdown.md").contains("| TASK-001 Fixes | REQ-002 |"));
    let stub = read("tasks/TASK-001.md");
    assert!(stub.contains("requirements: [REQ-002]"), "{stub}");
    assert!(stub.contains("parent: G-1\namends: F-1\n"), "{stub}");
    assert!(read("brief.md").contains("Fixup of F-1"));
    let i = zforge::intake::Intake::open(&p.root, "G-1").unwrap();
    assert_eq!(i.files().len(), 5);

    // Until the fixes are written, nothing is sent.
    let out = p.zforge(&["intake", "review", "G-1", "--all"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("defines no requirement"),
        "{}",
        stderr(&out)
    );

    let outcome = outcome
        .replace(
            "## Summary\n",
            "## Summary\nOne fix: the filter ignores case.\n",
        )
        .replace(
            "## Problem\n",
            "## Problem\n- Filtering by `Open` finds nothing.\n",
        )
        .replace(
            "## Requirements\n",
            "## Requirements\n\n- REQ-002: The status filter ignores case.\n",
        );
    std::fs::write(g.join("01-outcome.md"), outcome).unwrap();
    let mut task = stub;
    for (section, text) in [
        ("Goal", "Case-insensitive filter."),
        ("Input", "The listing."),
        ("Output", "The filter."),
        ("Constraints", "Keep the API."),
        ("Autonomy", "Any helper."),
        (
            "Acceptance and verification",
            "- AC-01: `Open` finds open tasks",
        ),
        ("Amend the contract when", "The API must change."),
    ] {
        task = task.replacen(
            &format!("## {section}\n"),
            &format!("## {section}\n{text}\n"),
            1,
        );
    }
    std::fs::write(g.join("tasks/TASK-001.md"), task).unwrap();

    let out = p.zforge(&["intake", "review", "G-1", "--all"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains('⚠'),
        "the fixup files lint clean: {}",
        stderr(&out)
    );
    let pack = p.ok(&["intake", "pack", "G-1"]);
    assert_eq!(
        pack.matches("Standard text for a fixup intake — nothing of its own to decide.")
            .count(),
        3,
        "untouched 02/03/04 take one line each: {pack}"
    );
    let decided =
        zforge::intake::batch::accept_all(&i, &i.files(), &Decider::terminal(None)).unwrap();
    assert!(decided.iter().all(|(_, r)| r.is_ok()), "{decided:?}");

    let out = p.zforge(&["readiness", "G-1", "--json"]);
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(out.status.success(), "{:?}", failing(&r));
    assert_eq!(r["tasks"], serde_json::json!(["TASK-001"]));
    assert_eq!(r["files"].as_array().unwrap().len(), 5);
    let config = zforge::config::load_from(&p.root.join(".zforge/config.yaml")).unwrap();
    let pinned: Vec<zforge::intake::readiness::Pinned> =
        serde_json::from_value(r["files"].clone()).unwrap();
    let m = zforge::intake::handover::create(&i, &p.root, &[], &config, &pinned, None).unwrap();
    assert_eq!(m.tasks, ["TASK-001"]);

    // A fixup amends settled work: a source with a stage not accepted is refused.
    p.ok(&["intake", "new", "H-1"]);
    let out = p.zforge(&["intake", "amend", "K-1", "--from", "H-1", "--fixup"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("no accepted revision"),
        "{}",
        stderr(&out)
    );
}

/// Revision notes: recorded on the review, warned about when a later
/// revision goes out without one, and listed newest first in the pack.
#[test]
fn revision_notes_are_recorded_warned_about_and_packed() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    let outcome = p.intake_dir().join("01-outcome.md");
    std::fs::write(&outcome, OUTCOME).unwrap();
    let out = p.zforge(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(out.status.success());
    assert!(!stderr(&out).contains("without a note"), "first revision");
    let i = zforge::intake::Intake::open(&p.root, "F-1").unwrap();
    zforge::intake::review::accept(&i, "01-outcome.md", None).unwrap();

    // Revision 2, without a note: warned, still sent.
    std::fs::write(&outcome, OUTCOME.replace("lọc theo", "lọc nhanh theo")).unwrap();
    let out = p.zforge(&["intake", "review", "F-1", "01-outcome.md"]);
    assert!(out.status.success());
    assert!(
        stderr(&out).contains("revision 2 was sent without a note"),
        "{}",
        stderr(&out)
    );
    zforge::intake::review::revise(&i, "01-outcome.md", None, "say which statuses").unwrap();

    // Revision 3, with a note (through --all, so the other stages go too).
    for (f, text) in [
        ("02-behavior.md", BEHAVIOR),
        ("03-solution.md", SOLUTION),
        ("04-breakdown.md", BREAKDOWN),
    ] {
        std::fs::write(p.intake_dir().join(f), text).unwrap();
    }
    std::fs::write(
        &outcome,
        OUTCOME.replace("lọc theo", "lọc nhanh theo open/done"),
    )
    .unwrap();
    let out = p.zforge(&[
        "intake",
        "review",
        "F-1",
        "--all",
        "--note",
        "named the statuses, as asked",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stderr(&out).contains("without a note"), "{}", stderr(&out));

    let log = zforge::intake::record::read(&i).unwrap();
    let last = log.iter().rfind(|d| d.file == "01-outcome.md").unwrap();
    assert_eq!(
        (last.revision, last.note.as_str()),
        (3, "named the statuses, as asked")
    );

    let pack = p.ok(&["intake", "pack", "F-1"]);
    let why = "Why it changed, newest first:\n\n\
               - revision 3: named the statuses, as asked\n\
               - revision 2, changes asked: say which statuses\n\
               - revision 2: no note\n";
    assert!(pack.contains(why), "{pack}");
}

/// `intake new --tiny`: one task created with the intake, 02/03/04 standard
/// text that lints clean and collapses to one line in the pack until it is
/// edited; all five files are still reviewed, accepted and pinned.
#[test]
fn a_tiny_intake_has_one_task_and_goes_through_the_whole_flow() {
    let p = Project::new();
    p.ready_intake();
    let out = p.zforge(&["intake", "new", "X-1", "--tiny", "--small"]);
    assert!(!out.status.success(), "--tiny with --small is refused");

    let out = p.ok(&["intake", "new", "T-1", "--tiny"]);
    assert!(out.contains("tiny profile"), "{out}");
    let t = p.root.join(".zforge/intakes/T-1");
    let read = |f: &str| std::fs::read_to_string(t.join(f)).unwrap();
    let i = zforge::intake::Intake::open(&p.root, "T-1").unwrap();
    assert_eq!(
        i.files(),
        [
            "01-outcome.md",
            "02-behavior.md",
            "03-solution.md",
            "04-breakdown.md",
            "tasks/TASK-001.md"
        ]
    );
    let outcome = read("01-outcome.md");
    assert!(
        outcome.starts_with("---\nprofile: tiny\n---\n"),
        "{outcome}"
    );
    assert!(zforge::intake::lint::is_small(&outcome) && zforge::intake::lint::is_tiny(&outcome));
    assert!(read("02-behavior.md").contains("TASK-001's acceptance criteria"));
    assert!(read("03-solution.md").contains("TASK-001's Constraints"));
    assert!(read("04-breakdown.md").contains("| TASK-001 | every requirement of 01-outcome |"));
    assert!(t.join("brief.md").is_file());

    // Until 01-outcome and the task are written, nothing is sent.
    let out = p.zforge(&["intake", "review", "T-1", "--all"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("defines no requirement"),
        "{}",
        stderr(&out)
    );

    let outcome = outcome
        .replace("## Summary\n", "## Summary\nThe filter ignores case.\n")
        .replace(
            "## Requirements\n",
            "## Requirements\n\n- REQ-001: The status filter ignores case.\n",
        )
        .replace(
            "## Must not change\n",
            "## Must not change\n\n- The CLI flags.\n",
        );
    std::fs::write(t.join("01-outcome.md"), outcome).unwrap();
    let mut task = read("tasks/TASK-001.md")
        .replace("requirements: []", "requirements: [REQ-001]")
        .replace("# TASK-001 — \n", "# TASK-001 — Case-insensitive filter\n");
    for (section, text) in [
        ("Goal", "Case-insensitive filter."),
        ("Input", "The listing."),
        ("Output", "The filter."),
        ("Constraints", "Keep the API."),
        ("Autonomy", "Any helper."),
        (
            "Acceptance and verification",
            "- AC-01: `Open` finds open tasks",
        ),
        ("Amend the contract when", "The API must change."),
    ] {
        task = task.replacen(
            &format!("## {section}\n"),
            &format!("## {section}\n{text}\n"),
            1,
        );
    }
    std::fs::write(t.join("tasks/TASK-001.md"), task).unwrap();

    let out = p.zforge(&["intake", "review", "T-1", "--all"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains('⚠'),
        "the tiny files lint clean: {}",
        stderr(&out)
    );
    let pack = p.ok(&["intake", "pack", "T-1"]);
    assert_eq!(
        pack.matches("Standard text for a tiny intake — nothing of its own to decide.")
            .count(),
        3,
        "{pack}"
    );
    assert!(pack.contains("The filter ignores case."), "{pack}");

    // An edited 03-solution is the user's to read: shown like any file.
    let solution = format!(
        "{}\n## Binding decisions\n\n| # | Decision | Why |\n|---|---|---|\n| D-1 | Compare lowercased | REQ-001 |\n",
        read("03-solution.md")
    );
    std::fs::write(t.join("03-solution.md"), solution).unwrap();
    let out = p.zforge(&["intake", "review", "T-1", "--all"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let pack = p.ok(&["intake", "pack", "T-1"]);
    assert_eq!(
        pack.matches("Standard text for a tiny intake").count(),
        2,
        "{pack}"
    );
    let solution_part = pack.split("### 03-solution.md").nth(1).unwrap();
    assert!(
        solution_part
            .split("### ")
            .next()
            .unwrap()
            .contains("New: no accepted revision yet."),
        "{pack}"
    );

    let decided =
        zforge::intake::batch::accept_all(&i, &i.files(), &Decider::terminal(None)).unwrap();
    assert!(decided.iter().all(|(_, r)| r.is_ok()), "{decided:?}");
    let out = p.zforge(&["readiness", "T-1", "--json"]);
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(out.status.success(), "{:?}", failing(&r));
    assert_eq!(r["tasks"], serde_json::json!(["TASK-001"]));
    assert_eq!(r["files"].as_array().unwrap().len(), 5);
    let config = zforge::config::load_from(&p.root.join(".zforge/config.yaml")).unwrap();
    let pinned: Vec<zforge::intake::readiness::Pinned> =
        serde_json::from_value(r["files"].clone()).unwrap();
    let m = zforge::intake::handover::create(&i, &p.root, &[], &config, &pinned, None).unwrap();
    assert_eq!(m.tasks, ["TASK-001"]);

    // A second task does not fit a tiny intake: an error naming the fix.
    p.ok(&["intake", "task", "T-1", "TASK-002"]);
    let status: serde_json::Value =
        serde_json::from_str(&p.ok(&["intake", "status", "T-1", "--json"])).unwrap();
    let errors: Vec<String> = status
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["file"] == "tasks/TASK-001.md")
        .flat_map(|f| f["issues"].as_array().unwrap().clone())
        .filter(|i| i["severity"] == "error")
        .map(|i| i["message"].as_str().unwrap().to_string())
        .collect();
    assert!(
        errors
            .iter()
            .any(|m| m.starts_with("a tiny intake has one task") && m.contains("`small`")),
        "{errors:?}"
    );
    let out = p.zforge(&["intake", "review", "T-1", "tasks/TASK-001.md"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("a tiny intake has one task"),
        "{}",
        stderr(&out)
    );
}

/// A full-profile intake with exactly one task gets a hint, not an error.
#[test]
fn intake_status_suggests_the_tiny_profile_for_one_task() {
    let p = Project::new();
    p.ok(&["intake", "new", "F-1"]);
    assert!(!p.ok(&["intake", "status", "F-1"]).contains("--tiny"));
    p.ok(&["intake", "task", "F-1", "TASK-001"]);
    let out = p.ok(&["intake", "status", "F-1"]);
    assert!(out.contains("--tiny"), "{out}");
    p.ok(&["intake", "task", "F-1", "TASK-002"]);
    assert!(!p.ok(&["intake", "status", "F-1"]).contains("--tiny"));
    p.ok(&["intake", "new", "T-1", "--tiny"]);
    assert!(!p.ok(&["intake", "status", "T-1"]).contains("--tiny"));
}
