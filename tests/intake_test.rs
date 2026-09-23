//! v1.5 intake through the real binary (Mốc A, decisions D1/D2/D4).
//!
//! The binary runs without a terminal, exactly as an agent does, so it can
//! create, fill and review files but must refuse to accept them. The user's
//! acceptance is exercised through the library: there is deliberately no
//! environment switch that would let a test — or an agent — accept through
//! the CLI without a TTY.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

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
    assert!(err.contains("section \"Mục tiêu\" is empty"), "{err}");

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
    assert!(p.root.join(".zforge/knowledge/index.md").is_file());

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

/// D6: a file accepted before something it builds on changed is not handed
/// over until the user confirms it again — which they can, unchanged.
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
        "04-breakdown.md was accepted before 03-solution.md revision 2; review it again"
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
