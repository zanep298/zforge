//! Creating an intake and moving its files through review (D1, D2).
//!
//! `review` snapshots the file and records its hash; `accept` records a
//! decision for exactly that hash, so a file edited after it was sent for
//! review cannot be accepted until it is reviewed again. These functions
//! do not check *who* calls them: the CLI only lets a human at a terminal
//! reach [`accept`] and [`revise`] (see `cli::intake`).

use super::lint::{self, Issue, Known, Severity};
use super::record::{self, Decision, DecisionKind, CHANNEL_CLI, CHANNEL_TTY};
use super::status::{self, DocStatus, Rev};
use super::{hash, templates, validate_id, Intake, CHANGES_DIR, STAGES, TASKS_DIR};
use anyhow::{bail, Context, Result};
use chrono::Utc;
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
        crate::state::write_atomic(&dir.join(stage), body.as_bytes())?;
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
    crate::state::write_atomic(&path, templates::task(&intake.id, task_id).as_bytes())?;
    Ok(path)
}

/// Requirement and task IDs defined by the intake's current files.
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
    }
}

/// Status of every file in the intake.
pub fn statuses(intake: &Intake) -> Result<Vec<DocStatus>> {
    let log = record::read(intake)?;
    Ok(intake
        .files()
        .iter()
        .map(|f| {
            let text = std::fs::read_to_string(intake.dir.join(f)).ok();
            status::derive(f, &log, text.as_deref())
        })
        .collect())
}

pub fn file_status(intake: &Intake, rel: &str) -> Result<DocStatus> {
    let path = intake.file(rel)?;
    let log = record::read(intake)?;
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

pub(crate) fn lock(intake: &Intake) -> Result<crate::state::TaskLockGuard> {
    let parent = intake
        .dir
        .parent()
        .context("intake directory has no parent")?;
    crate::state::lock_task(parent, &intake.id)
}

/// Send the file's current content for review as a new revision.
pub fn review(intake: &Intake, rel: &str) -> Result<Reviewed> {
    let path = intake.file(rel)?;
    let _lock = lock(intake)?;
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;

    let issues = lint::lint(rel, &text, &intake.id, &known(intake));
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

    let log = record::read(intake)?;
    let st = status::derive(rel, &log, Some(&text));
    let current = hash::sha256(&text);
    // Unchanged since it was accepted: nothing to review — unless something
    // it builds on was accepted since (D6), when the user confirms it again
    // as a new revision with the same text.
    if let Some(a) = st.accepted.as_ref().filter(|a| a.sha256 == current) {
        if !super::readiness::stale(intake)?.contains(rel) {
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
    record::write_snapshot(intake, rel, revision, &hash::normalize(&text))?;
    record::append(
        intake,
        &Decision {
            at: Utc::now(),
            file: rel.to_string(),
            revision,
            sha256: current.clone(),
            decision: DecisionKind::Review,
            channel: CHANNEL_CLI.into(),
            by: None,
            note: String::new(),
        },
    )?;
    let diff = st
        .accepted
        .as_ref()
        .map(|a| diff_against(intake, rel, a.revision, revision));
    Ok(Reviewed {
        revision,
        sha256: current,
        unchanged: false,
        warnings,
        diff,
    })
}

/// `git diff --no-index` between two recorded revisions of `rel`.
pub fn diff_against(intake: &Intake, rel: &str, from: u32, to: u32) -> String {
    let a = record::snapshot_path(intake, rel, from);
    let b = record::snapshot_path(intake, rel, to);
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
pub fn pending_review(intake: &Intake, rel: &str) -> Result<Rev> {
    let st = file_status(intake, rel)?;
    let Some(pending) = st.in_review else {
        bail!(
            "{rel} is not under review; send it with `zforge intake review {} {rel}`",
            intake.id
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

/// Record the user's acceptance of the revision under review. Callers must
/// have confirmed a human decided (see module docs).
pub fn accept(intake: &Intake, rel: &str, by: Option<String>) -> Result<Rev> {
    decide(intake, rel, DecisionKind::Accepted, by, String::new())
}

/// Record the user's request for changes to the revision under review.
pub fn revise(intake: &Intake, rel: &str, by: Option<String>, note: &str) -> Result<Rev> {
    if note.trim().is_empty() {
        bail!("say what needs to change (--note)");
    }
    decide(
        intake,
        rel,
        DecisionKind::NeedsRevision,
        by,
        note.trim().to_string(),
    )
}

fn decide(
    intake: &Intake,
    rel: &str,
    kind: DecisionKind,
    by: Option<String>,
    note: String,
) -> Result<Rev> {
    let _lock = lock(intake)?;
    let pending = pending_review(intake, rel)?;
    record::append(
        intake,
        &Decision {
            at: Utc::now(),
            file: rel.to_string(),
            revision: pending.revision,
            sha256: pending.sha256.clone(),
            decision: kind,
            channel: CHANNEL_TTY.into(),
            by,
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
