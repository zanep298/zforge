//! Creating an intake and moving its files through review (D1, D2).
//!
//! `review` snapshots the file and records its hash; `accept` records a
//! decision for exactly that hash, so a file edited after it was sent for
//! review cannot be accepted until it is reviewed again. These functions
//! do not check *who* calls them: the CLI only lets a human at a terminal
//! reach [`accept`] and [`revise`] (see `cli::intake`).

use super::lint::{self, Issue, Known, Severity};
use super::record::{self, Decider, Decision, DecisionKind, CHANNEL_CLI};
use super::status::{self, DocStatus, Rev};
use super::{hash, templates, validate_id, Intake, CHANGES_DIR, STAGES, TASKS_DIR};
use crate::knowledge::docs::DocSet;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Create `.zforge/intakes/<id>/` with a template for every stage.
pub fn create(project_root: &Path, id: &str) -> Result<Intake> {
    validate_id(id)?;
    let dir = super::intakes_dir(project_root).join(id);
    if dir.exists() {
        bail!("intake {id} already exists at {}", dir.display());
    }
    for sub in [TASKS_DIR, CHANGES_DIR] {
        std::fs::create_dir_all(dir.join(sub))?;
    }
    for stage in STAGES {
        let body = templates::stage(stage, id).expect("template for every stage");
        crate::fs::write_atomic(&dir.join(stage), body.as_bytes())?;
    }
    Ok(Intake {
        id: id.to_string(),
        dir,
    })
}

/// Add `tasks/<task_id>.md` from the leaf task template.
pub fn create_task(intake: &Intake, task_id: &str) -> Result<PathBuf> {
    validate_id(task_id)?;
    let path = intake.dir.join(TASKS_DIR).join(format!("{task_id}.md"));
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    std::fs::create_dir_all(intake.dir.join(TASKS_DIR))?;
    crate::fs::write_atomic(&path, templates::task(&intake.id, task_id).as_bytes())?;
    Ok(path)
}

/// Requirement and task IDs defined by the intake's current files, plus the
/// item IDs the project's accepted knowledge states now (ONBOARD REQ-007,
/// TASK-009), so a stage citing one that is not accepted gets a warning
/// naming it.
pub fn known(intake: &Intake) -> Known {
    let outcome = std::fs::read_to_string(intake.dir.join(lint::OUTCOME)).unwrap_or_default();
    Known {
        requirements: lint::defined_requirements(&outcome).into_iter().collect(),
        tasks: intake
            .files()
            .iter()
            .filter_map(|f| {
                f.strip_prefix("tasks/")?
                    .strip_suffix(".md")
                    .map(String::from)
            })
            .collect(),
        knowledge_ids: knowledge_ids(intake),
    }
}

/// The project's accepted knowledge item IDs, or empty when the project
/// root, its config, or its knowledge cannot be read — never an error: a
/// project that has not been onboarded, or an intake opened outside a full
/// project layout (as some tests do), still lints.
fn knowledge_ids(intake: &Intake) -> BTreeSet<String> {
    (|| -> Result<BTreeSet<String>> {
        // `intake.dir` is `<project_root>/.zforge/intakes/<id>` (see
        // `intakes_dir`); walk back up to the project root.
        let root = intake
            .dir
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| {
                anyhow::anyhow!("cannot resolve the project root from {:?}", intake.dir)
            })?;
        let config = crate::config::load_from(&root.join(".zforge").join("config.yaml"))?;
        let k = crate::knowledge::Knowledge::open(&config);
        crate::knowledge::accepted_item_ids(&k)
    })()
    .unwrap_or_default()
}

/// Status of every file in a document set (an intake, or the knowledge).
pub fn statuses<D: DocSet>(doc: &D) -> Result<Vec<DocStatus>> {
    let log = record::read(doc)?;
    Ok(doc
        .files()
        .iter()
        .map(|f| {
            let text = std::fs::read_to_string(doc.dir().join(f)).ok();
            status::derive(f, &log, text.as_deref())
        })
        .collect())
}

pub fn file_status<D: DocSet>(doc: &D, rel: &str) -> Result<DocStatus> {
    let path = doc.file(rel)?;
    let log = record::read(doc)?;
    let text = std::fs::read_to_string(&path).ok();
    Ok(status::derive(rel, &log, text.as_deref()))
}

#[derive(Debug)]
pub struct Reviewed {
    pub revision: u32,
    pub sha256: String,
    /// The same content was already under review; nothing was recorded.
    pub unchanged: bool,
    /// Warnings from the linter (errors refuse the review).
    pub warnings: Vec<Issue>,
    /// `git diff` against the accepted revision; `None` for a first review.
    pub diff: Option<String>,
}

pub(crate) fn lock<D: DocSet>(doc: &D) -> Result<crate::lock::LockGuard> {
    crate::lock::lock_at(&doc.lock_dir(), doc.id())
}

/// Send the file's current content for review as a new revision.
pub fn review<D: DocSet>(doc: &D, rel: &str) -> Result<Reviewed> {
    let path = doc.file(rel)?;
    let _lock = lock(doc)?;
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;

    let issues = doc.lint(rel, &text);
    let (errors, warnings): (Vec<Issue>, Vec<Issue>) = issues
        .into_iter()
        .partition(|i| i.severity == Severity::Error);
    if !errors.is_empty() {
        let list: Vec<String> = errors
            .iter()
            .map(|i| format!("  - {}", i.message))
            .collect();
        bail!("{rel} is not ready for review:\n{}", list.join("\n"));
    }

    let log = record::read(doc)?;
    let st = status::derive(rel, &log, Some(&text));
    let current = hash::sha256(&text);
    // Unchanged since it was accepted: nothing to review — unless something
    // it builds on was accepted since (D6), when the user confirms it again
    // as a new revision with the same text.
    if let Some(a) = st.accepted.as_ref().filter(|a| a.sha256 == current) {
        if !doc.reconfirm_needed(rel)? {
            bail!(
                "{rel} is already accepted as revision {}; edit it first",
                a.revision
            );
        }
    }
    if let Some(r) = st.in_review.as_ref().filter(|r| r.sha256 == current) {
        return Ok(Reviewed {
            revision: r.revision,
            sha256: current,
            unchanged: true,
            warnings,
            diff: None,
        });
    }

    let revision = st.last_revision + 1;
    record::write_snapshot(doc, rel, revision, &hash::normalize(&text))?;
    record::append(
        doc,
        &Decision {
            at: Utc::now(),
            file: rel.to_string(),
            revision,
            sha256: current.clone(),
            decision: DecisionKind::Review,
            channel: CHANNEL_CLI.into(),
            by: None,
            session: None,
            note: String::new(),
        },
    )?;
    let diff = st
        .accepted
        .as_ref()
        .map(|a| diff_against(doc, rel, a.revision, revision));
    Ok(Reviewed {
        revision,
        sha256: current,
        unchanged: false,
        warnings,
        diff,
    })
}

/// `git diff --no-index` between two recorded revisions of `rel`.
pub fn diff_against<D: DocSet>(doc: &D, rel: &str, from: u32, to: u32) -> String {
    let a = record::snapshot_path(doc, rel, from);
    let b = record::snapshot_path(doc, rel, to);
    let out = std::process::Command::new("git")
        .args(["diff", "--no-index", "--no-color", "--"])
        .arg(&a)
        .arg(&b)
        .output();
    match out {
        // Exit 1 means "files differ" for --no-index.
        Ok(o) if o.status.code() == Some(0) || o.status.code() == Some(1) => {
            String::from_utf8_lossy(&o.stdout).into_owned()
        }
        Ok(o) => format!(
            "(git diff failed: {})",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => format!("(git not available for diff: {e})"),
    }
}

/// The revision under review, checked against the file on disk: it must
/// still be exactly what was sent for review.
pub fn pending_review<D: DocSet>(doc: &D, rel: &str) -> Result<Rev> {
    let st = file_status(doc, rel)?;
    let Some(pending) = st.in_review else {
        bail!(
            "{rel} is not under review; send it with `{}`",
            doc.review_hint(rel)
        );
    };
    if st.current_sha256.as_deref() != Some(pending.sha256.as_str()) {
        bail!(
            "{rel} changed after revision {} was sent for review; review it again before deciding",
            pending.revision
        );
    }
    Ok(pending)
}

/// Record the user's acceptance of the revision under review, made at a
/// terminal. Callers must have confirmed a human decided (see module docs).
pub fn accept<D: DocSet>(doc: &D, rel: &str, by: Option<String>) -> Result<Rev> {
    accept_as(doc, rel, &Decider::terminal(by))
}

/// Record the user's request for changes to the revision under review,
/// made at a terminal.
pub fn revise<D: DocSet>(doc: &D, rel: &str, by: Option<String>, note: &str) -> Result<Rev> {
    revise_as(doc, rel, &Decider::terminal(by), note)
}

/// [`accept`] through `who`'s channel (D1: a terminal, or the user's own
/// message to Claude Code read by the prompt hook).
pub fn accept_as<D: DocSet>(doc: &D, rel: &str, who: &Decider) -> Result<Rev> {
    decide(doc, rel, DecisionKind::Accepted, who, String::new())
}

/// [`revise`] through `who`'s channel.
pub fn revise_as<D: DocSet>(doc: &D, rel: &str, who: &Decider, note: &str) -> Result<Rev> {
    if note.trim().is_empty() {
        bail!("say what needs to change (--note)");
    }
    decide(
        doc,
        rel,
        DecisionKind::NeedsRevision,
        who,
        note.trim().to_string(),
    )
}

fn decide<D: DocSet>(
    doc: &D,
    rel: &str,
    kind: DecisionKind,
    who: &Decider,
    note: String,
) -> Result<Rev> {
    let _lock = lock(doc)?;
    let pending = pending_review(doc, rel)?;
    if kind == DecisionKind::Accepted {
        // Checked here, under the lock, on the exact revision this call is
        // about to accept — not on whatever was pending when the caller
        // first looked, which a concurrent review could have moved past
        // (Output, AC-06).
        let text = record::read_snapshot(doc, rel, pending.revision)?;
        doc.check_acceptable(rel, pending.revision, &text)?;
    }
    record::append(
        doc,
        &Decision {
            at: Utc::now(),
            file: rel.to_string(),
            revision: pending.revision,
            sha256: pending.sha256.clone(),
            decision: kind,
            channel: who.channel.into(),
            by: who.by.clone(),
            session: who.session.clone(),
            note,
        },
    )?;
    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intake::status::DocState;

    const OUTCOME: &str = "# F — Outcome\n\n## Yêu cầu\n\n- REQ-001: lọc\n\n## Câu hỏi còn mở\n";

    fn setup() -> (tempfile::TempDir, Intake) {
        let tmp = tempfile::tempdir().unwrap();
        let intake = create(tmp.path(), "F").unwrap();
        std::fs::write(intake.dir.join("01-outcome.md"), OUTCOME).unwrap();
        (tmp, intake)
    }

    fn state(intake: &Intake) -> DocState {
        file_status(intake, "01-outcome.md").unwrap().state
    }

    #[test]
    fn review_then_accept_records_the_reviewed_hash() {
        let (_t, i) = setup();
        let r = review(&i, "01-outcome.md").unwrap();
        assert_eq!((r.revision, r.unchanged), (1, false));
        assert!(r.diff.is_none(), "first review has nothing to diff against");
        assert_eq!(state(&i), DocState::InReview);
        assert!(
            review(&i, "01-outcome.md").unwrap().unchanged,
            "same content, no new revision"
        );

        let rev = accept(&i, "01-outcome.md", Some("zane".into())).unwrap();
        assert_eq!(rev.revision, 1);
        assert_eq!(state(&i), DocState::Accepted);
        assert!(review(&i, "01-outcome.md")
            .unwrap_err()
            .to_string()
            .contains("already accepted"));
        assert_eq!(
            record::read_snapshot(&i, "01-outcome.md", 1).unwrap(),
            OUTCOME,
            "the snapshot is what was reviewed"
        );
    }

    /// D2: an edit between review and decision must not be accepted.
    #[test]
    fn a_file_edited_after_review_cannot_be_accepted() {
        let (_t, i) = setup();
        review(&i, "01-outcome.md").unwrap();
        std::fs::write(
            i.dir.join("01-outcome.md"),
            format!("{OUTCOME}- REQ-002: thêm\n"),
        )
        .unwrap();
        assert_eq!(state(&i), DocState::ChangedSinceReview);
        let err = accept(&i, "01-outcome.md", None).unwrap_err().to_string();
        assert!(err.contains("changed after revision 1"), "{err}");
    }

    #[test]
    fn revision_request_then_new_revision_with_diff() {
        let (_t, i) = setup();
        review(&i, "01-outcome.md").unwrap();
        accept(&i, "01-outcome.md", None).unwrap();

        std::fs::write(
            i.dir.join("01-outcome.md"),
            OUTCOME.replace("lọc", "lọc theo trạng thái"),
        )
        .unwrap();
        let r = review(&i, "01-outcome.md").unwrap();
        assert_eq!(r.revision, 2);
        let diff = r.diff.unwrap();
        assert!(
            diff.contains("-- REQ-001: lọc") && diff.contains("+- REQ-001: lọc theo trạng thái"),
            "{diff}"
        );

        assert!(
            revise(&i, "01-outcome.md", None, " ").is_err(),
            "a reason is required"
        );
        revise(&i, "01-outcome.md", None, "nêu rõ trạng thái").unwrap();
        let st = file_status(&i, "01-outcome.md").unwrap();
        assert_eq!(st.state, DocState::NeedsRevision);
        assert_eq!(
            st.accepted.unwrap().revision,
            1,
            "revision 1 stays in force"
        );
    }

    #[test]
    fn lint_errors_refuse_the_review() {
        let (_t, i) = setup();
        let err = review(&i, "02-behavior.md").unwrap_err().to_string();
        assert!(err.contains("no content yet"), "{err}");
        assert!(accept(&i, "02-behavior.md", None)
            .unwrap_err()
            .to_string()
            .contains("not under review"));
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }

    fn head(dir: &std::path::Path) -> String {
        String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(dir)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    /// AC-01: a stage citing a knowledge ID that is not (yet) accepted gets
    /// a warning naming it; a citation of an ID that *is* accepted does not.
    #[test]
    fn citing_an_unaccepted_knowledge_id_warns_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        std::fs::write(root.join("x.rs"), "one\ntwo\nthree\n").unwrap();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "c"]);
        let pinned = head(root);

        let config = crate::config::load_from(&root.join(".zforge/config.yaml")).unwrap();
        let k = crate::knowledge::Knowledge::open(&config);
        std::fs::create_dir_all(&k.dir).unwrap();
        std::fs::write(
            k.dir.join("rules.md"),
            format!(
                "---\npinned: {pinned}\n---\n## m\n- RULE-001: r1. (x.rs:1)\n- RULE-002: r2. (x.rs:2)\n- RULE-003: r3. (x.rs:3)\n\n## Open questions\n"
            ),
        )
        .unwrap();
        for f in ["domain.md", "conventions.md"] {
            std::fs::write(
                k.dir.join(f),
                "# k\n\nSomething happens.\n\n## Open questions\n",
            )
            .unwrap();
        }
        for f in crate::knowledge::FILES {
            review(&k, f).unwrap();
            accept(&k, f, None).unwrap();
        }

        let intake = create(root, "F").unwrap();
        std::fs::write(
            intake.dir.join("01-outcome.md"),
            "# F — Outcome\n\n## Yêu cầu\n\n- REQ-001: Keeps RULE-001; departs from RULE-009 because reasons.\n\n## Câu hỏi còn mở\n",
        )
        .unwrap();

        let k2 = known(&intake);
        assert!(k2.knowledge_ids.contains("RULE-001"));
        assert!(!k2.knowledge_ids.contains("RULE-009"));

        let text = std::fs::read_to_string(intake.dir.join("01-outcome.md")).unwrap();
        let issues = lint::lint("01-outcome.md", &text, "F", &k2);
        let warnings: Vec<&str> = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .map(|i| i.message.as_str())
            .collect();
        assert!(
            warnings.iter().any(|m| m.contains("RULE-009")),
            "{warnings:?}"
        );
        assert!(
            !warnings.iter().any(|m| m.contains("RULE-001")),
            "an accepted id must not warn: {warnings:?}"
        );
    }

    #[test]
    fn tasks_get_their_own_template_and_ids() {
        let (_t, i) = setup();
        create_task(&i, "TASK-001").unwrap();
        assert!(create_task(&i, "TASK-001").is_err());
        let k = known(&i);
        assert!(k.requirements.contains("REQ-001"));
        assert!(k.tasks.contains("TASK-001"));
        assert!(create(
            i.dir.parent().unwrap().parent().unwrap().parent().unwrap(),
            "F"
        )
        .is_err());
    }
}
