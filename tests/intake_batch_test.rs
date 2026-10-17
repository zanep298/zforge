//! The whole intake in one pass, through the real binary: `intake review
//! --all`, the user's `/accept all` (recorded through the library, as the
//! prompt hook does — the binary refuses without a terminal), `intake
//! pack`, and the small profile.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const OUTCOME: &str = "# F-1 — Outcome\n\n## Vấn đề\nCần lọc.\n\n## Yêu cầu\n\n- REQ-001: lọc theo trạng thái\n\n## Câu hỏi còn mở\n\n- [x] trạng thái archived có tính không?\n";
const BEHAVIOR: &str =
    "# F-1 — Behavior\n\n## Tình huống\nREQ-001: status=open trả task open.\n\n## Câu hỏi còn mở\n";
const SOLUTION: &str =
    "# F-1 — Solution\n\n## Luồng xử lý\nLọc trong listing.\n\n## Câu hỏi còn mở\n";
const BREAKDOWN: &str = "# F-1 — Breakdown\n\n## Task và dependency\nTASK-001 rồi TASK-002.\n\n## Kiểm chứng tích hợp\nChạy toàn bộ test listing trên cùng tree.\n\n## Câu hỏi còn mở\n";

fn task(id: &str, deps: &str) -> String {
    format!(
        "---\nid: {id}\nparent: F-1\nrequirements: [REQ-001]\ndepends_on: [{deps}]\n---\n\n# {id}\n\n\
         ## Mục tiêu\nLọc.\n## Input\nAPI.\n## Output\nDanh sách.\n## Ràng buộc\nGiữ shape.\n\
         ## Tự chủ\nTự chọn hàm.\n## Acceptance và kiểm chứng\n- AC-01: lọc đúng\n## Bàn giao\nLocal.\n\
         ## Cần amendment khi\nĐổi shape.\n## Câu hỏi còn mở\n"
    )
}

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    /// A git repo with a budget and a runner, and intake F-1 written in
    /// full — TASK-002 depends on TASK-001 — none of it reviewed.
    fn new(new_args: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::create_dir_all(root.join(".home")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\nexecution:\n  budget_usd: 2.5\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".home/registry.yaml"),
            "agents:\n  claude:\n    command: sh\n",
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
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        let p = Self { _dir: dir, root };
        let mut args = vec!["intake", "new", "F-1"];
        args.extend_from_slice(new_args);
        p.ok(&args);
        for (f, text) in [
            ("01-outcome.md", OUTCOME.to_string()),
            ("02-behavior.md", BEHAVIOR.to_string()),
            ("03-solution.md", SOLUTION.to_string()),
            ("04-breakdown.md", BREAKDOWN.to_string()),
            ("tasks/TASK-001.md", task("TASK-001", "")),
            ("tasks/TASK-002.md", task("TASK-002", "TASK-001")),
        ] {
            p.write(f, &text);
        }
        p
    }

    fn write(&self, file: &str, text: &str) {
        std::fs::write(self.dir().join(file), text).unwrap();
    }

    fn dir(&self) -> PathBuf {
        self.root.join(".zforge/intakes/F-1")
    }

    fn intake(&self) -> zforge::intake::Intake {
        zforge::intake::Intake::open(&self.root, "F-1").unwrap()
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
        assert!(out.status.success(), "zforge {args:?}: {}", stderr(&out));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn decisions(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.dir().join(".records/decisions.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Files with a `kind` decision, in the order recorded.
    fn files_with(&self, kind: &str) -> Vec<String> {
        self.decisions()
            .into_iter()
            .filter(|d| d["decision"] == kind)
            .map(|d| d["file"].as_str().unwrap().to_string())
            .collect()
    }

    /// The user's `/accept all`, as the prompt hook records it.
    fn accept_all(&self) {
        let intake = self.intake();
        let files = zforge::intake::batch::under_review(&intake).unwrap();
        let who = zforge::intake::record::Decider::prompt(Some("u".into()), None);
        for (f, r) in zforge::intake::batch::accept_all(&intake, &files, &who).unwrap() {
            r.unwrap_or_else(|e| panic!("{f}: {e:#}"));
        }
    }

    fn readiness(&self) -> (bool, serde_json::Value) {
        let out = self.zforge(&["readiness", "F-1", "--json"]);
        (
            out.status.success(),
            serde_json::from_slice(&out.stdout).unwrap(),
        )
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn failing(report: &serde_json::Value) -> Vec<String> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["ok"] == false)
        .flat_map(|c| c["problems"].as_array().unwrap().clone())
        .map(|p| p.as_str().unwrap().to_string())
        .collect()
}

const EVERY_FILE: [&str; 6] = [
    "01-outcome.md",
    "02-behavior.md",
    "03-solution.md",
    "04-breakdown.md",
    "tasks/TASK-001.md",
    "tasks/TASK-002.md",
];

/// Atomic: one broken file and nothing is sent; every problem is listed.
#[test]
fn review_all_sends_nothing_while_any_file_has_an_error() {
    let p = Project::new(&[]);
    p.write(
        "02-behavior.md",
        &zforge::intake::templates::stage("02-behavior.md", "F-1").unwrap(),
    );
    p.write(
        "tasks/TASK-002.md",
        &task("TASK-002", "TASK-001").replace("## Bàn giao\nLocal.\n", ""),
    );

    let out = p.zforge(&["intake", "review", "F-1", "--all"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("nothing sent"), "{err}");
    assert!(
        err.contains("02-behavior.md:\n  - has no content yet"),
        "{err}"
    );
    assert!(
        err.contains("tasks/TASK-002.md:\n  - missing section"),
        "{err}"
    );
    assert!(p.decisions().is_empty(), "nothing may be recorded");

    p.write("02-behavior.md", BEHAVIOR);
    p.write("tasks/TASK-002.md", &task("TASK-002", "TASK-001"));
    p.ok(&["intake", "review", "F-1", "--all"]);
    assert_eq!(p.files_with("review"), EVERY_FILE);

    // Already under review as they are: nothing new is recorded.
    let again = p.ok(&["intake", "review", "F-1", "--all"]);
    assert!(again.contains("already under review"), "{again}");
    assert_eq!(p.files_with("review").len(), EVERY_FILE.len());
}

/// A file the user asked to change, sent again unchanged, is refused.
#[test]
fn review_all_refuses_a_file_still_as_the_user_asked_to_change_it() {
    let p = Project::new(&[]);
    p.ok(&["intake", "review", "F-1", "--all"]);
    zforge::intake::review::revise(&p.intake(), "02-behavior.md", None, "more cases").unwrap();
    let out = p.zforge(&["intake", "review", "F-1", "--all"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("02-behavior.md:\n  - unchanged since the user asked for changes to revision 1: more cases"),
        "{err}"
    );
}

/// One `/accept all` on a fresh intake whose tasks are named against their
/// dependency order: stages first, each task after what it depends on,
/// and nothing of the batch stale.
#[test]
fn accept_all_records_in_dependency_order_and_leaves_nothing_stale() {
    let p = Project::new(&[]);
    p.write("tasks/TASK-001.md", &task("TASK-001", "TASK-002"));
    p.write("tasks/TASK-002.md", &task("TASK-002", ""));
    p.ok(&["intake", "review", "F-1", "--all"]);
    p.accept_all();

    assert_eq!(
        p.files_with("accepted"),
        [
            "01-outcome.md",
            "02-behavior.md",
            "03-solution.md",
            "04-breakdown.md",
            "tasks/TASK-002.md",
            "tasks/TASK-001.md"
        ]
    );
    assert!(zforge::intake::readiness::stale(&p.intake())
        .unwrap()
        .is_empty());
    let (ok, r) = p.readiness();
    assert!(ok, "{:?}", failing(&r));

    // At a terminal, `accept --all` needs a human like a single accept.
    let out = p.zforge(&["intake", "accept", "F-1", "--all"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("needs an interactive terminal"));
}

/// A file that cannot be accepted holds back what builds on it, so
/// nothing is accepted on a revision the user has not accepted.
#[test]
fn accept_all_refuses_what_builds_on_a_refused_file() {
    let p = Project::new(&[]);
    p.ok(&["intake", "review", "F-1", "--all"]);
    p.write(
        "tasks/TASK-001.md",
        &task("TASK-001", "").replace("Lọc.", "Lọc nhanh."),
    );
    let intake = p.intake();
    let files = zforge::intake::batch::under_review(&intake).unwrap();
    let who = zforge::intake::record::Decider::prompt(None, None);
    let out = zforge::intake::batch::accept_all(&intake, &files, &who).unwrap();
    let refused: Vec<(&str, String)> = out
        .iter()
        .filter_map(|(f, r)| r.as_ref().err().map(|e| (f.as_str(), e.to_string())))
        .collect();
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert!(
        refused[0].1.contains("changed after revision 1"),
        "{refused:?}"
    );
    assert_eq!(refused[1].0, "tasks/TASK-002.md");
    assert!(
        refused[1].1.contains("builds on tasks/TASK-001.md"),
        "{refused:?}"
    );
    assert_eq!(p.files_with("accepted"), &EVERY_FILE[..4]);
}

/// Changing an upstream file of an accepted intake: `review --all` sends
/// what rests on it again, unchanged, so one `/accept all` settles it all.
#[test]
fn a_changed_upstream_is_settled_in_one_pass() {
    let p = Project::new(&[]);
    p.ok(&["intake", "review", "F-1", "--all"]);
    p.accept_all();
    p.write(
        "02-behavior.md",
        &BEHAVIOR.replace("task open", "task đang mở"),
    );

    let before = p.files_with("review").len();
    let out = p.ok(&["intake", "review", "F-1", "--all"]);
    assert!(
        out.contains("accepted and unchanged: 01-outcome.md"),
        "{out}"
    );
    assert_eq!(p.files_with("review")[before..], EVERY_FILE[1..]);

    let pack = p.ok(&["intake", "pack", "F-1"]);
    assert!(pack.starts_with("# F-1 — Review pack"), "{pack}");
    assert!(pack.contains("### 02-behavior.md — revision 2"), "{pack}");
    assert!(pack.contains("```diff"), "{pack}");
    assert!(
        pack.contains("+REQ-001: status=open trả task đang mở."),
        "{pack}"
    );
    assert!(
        pack.contains("Unchanged since accepted revision 1: confirm it again"),
        "{pack}"
    );
    assert!(pack.contains("/accept F-1 all"), "{pack}");
    assert!(!pack.contains("### 01-outcome.md"), "{pack}");

    p.accept_all();
    let (ok, r) = p.readiness();
    assert!(ok, "{:?}", failing(&r));
}

#[test]
fn the_pack_shows_the_brief_and_each_summary_in_order() {
    let p = Project::new(&[]);
    p.write(
        "01-outcome.md",
        &OUTCOME.replace(
            "## Vấn đề",
            "## Summary\n\nLists can be filtered by status.\n\n## Vấn đề",
        ),
    );
    p.write("brief.md", "# F-1 — Brief\n\n## Goal\n\nFilter tasks.\n");
    let empty = p.ok(&["intake", "pack", "F-1"]);
    assert!(empty.contains("Nothing is under review"), "{empty}");

    p.ok(&["intake", "review", "F-1", "--all"]);
    let pack = p.ok(&["intake", "pack", "F-1"]);
    assert!(pack.contains("### Goal\n\nFilter tasks."), "{pack}");
    assert!(pack.contains("### 01-outcome.md — revision 1"), "{pack}");
    assert!(pack.contains("Lists can be filtered by status."), "{pack}");
    assert!(pack.contains("New: no accepted revision yet."), "{pack}");
    assert!(
        pack.contains("- 02-behavior.md: has no \"Summary\" section"),
        "{pack}"
    );
    let at = |s: &str| pack.find(s).unwrap_or_else(|| panic!("{s}: {pack}"));
    assert!(at("## Brief") < at("### 01-outcome.md"));
    assert!(at("### 04-breakdown.md") < at("### tasks/TASK-001.md"));
    assert!(at("### tasks/TASK-002.md") < at("## Warnings"));
    assert!(at("## Warnings") < at("## Decide"));
}

#[test]
fn a_small_intake_starts_from_compact_templates() {
    let tmp = tempfile::tempdir().unwrap();
    let i = zforge::intake::review::create_with(tmp.path(), "F-1", true).unwrap();
    let read = |f: &str| std::fs::read_to_string(i.dir.join(f)).unwrap();
    assert!(read("01-outcome.md").starts_with("---\nprofile: small\n---\n"));
    assert!(zforge::intake::lint::is_small(&read("01-outcome.md")));
    for f in ["02-behavior.md", "03-solution.md"] {
        assert!(!read(f).contains("## Summary"), "{f}");
    }
    assert!(read("03-solution.md").contains("## Binding decisions"));
    assert!(read("04-breakdown.md").contains("## Summary"));
}

/// The small profile: compact 02 and 03 pass review without a summary or
/// open questions, and the intake is ready and handed over like any other.
#[test]
fn a_small_intake_is_reviewed_accepted_and_handed_over() {
    let p = Project::new(&["--small"]);
    p.write(
        "01-outcome.md",
        &format!("---\nprofile: small\n---\n\n{OUTCOME}"),
    );
    p.write(
        "02-behavior.md",
        "# F-1 — Behavior\n\n## Situations\n\n| # | Kind | When | Then | REQ |\n|---|---|---|---|---|\n\
         | 1 | normal | status=open | open tasks | REQ-001 |\n",
    );
    p.write(
        "03-solution.md",
        "# F-1 — Solution\n\n## Binding decisions\n\nNone: the implementing agent chooses.\n",
    );
    let out = p.zforge(&["intake", "review", "F-1", "--all"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let warnings = stderr(&out);
    for f in ["02-behavior.md", "03-solution.md"] {
        assert!(
            !warnings.contains(&format!("{f}: has no")),
            "{f} is relaxed in a small intake: {warnings}"
        );
    }
    assert!(
        warnings.contains("01-outcome.md: has no \"Summary\""),
        "{warnings}"
    );

    p.accept_all();
    let (ok, r) = p.readiness();
    assert!(ok, "{:?}", failing(&r));
    let pinned: Vec<zforge::intake::readiness::Pinned> =
        serde_json::from_value(r["files"].clone()).unwrap();
    assert_eq!(pinned.len(), EVERY_FILE.len(), "every stage is pinned");
    let config = zforge::config::load_from(&p.root.join(".zforge/config.yaml")).unwrap();
    zforge::intake::handover::create(&p.intake(), &p.root, &[], &config, &pinned, None).unwrap();
}
