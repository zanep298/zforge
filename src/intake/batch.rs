//! The whole intake in one pass: send every file that needs the user for
//! review together ([`review_all`]) and record the user's acceptance of a
//! set in dependency order ([`accept_all`]), so a file accepted in the same
//! batch never rests on something accepted after it (D6).
//!
//! Like [`super::review`], nothing here checks *who* calls it: the CLI
//! lets only a human at a terminal reach [`accept_all`], and the prompt
//! hook only the user's own `/accept` message (D1).

use super::lint::{self, TaskMeta};
use super::record::{self, Decider, DecisionKind};
use super::review::{self, Reviewed};
use super::status::{DocState, Rev};
use super::{readiness, Intake, STAGES};
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};

fn task_id(file: &str) -> Option<&str> {
    file.strip_prefix("tasks/")?.strip_suffix(".md")
}

/// Files `rel` builds on directly: the stages before a stage; every stage
/// and the tasks it depends on for a task. A change request builds on
/// nothing here — it is decided on its own.
fn upstream(rel: &str, metas: &BTreeMap<String, TaskMeta>) -> Vec<String> {
    if let Some(t) = task_id(rel) {
        return STAGES
            .iter()
            .map(|s| s.to_string())
            .chain(
                metas
                    .get(t)
                    .into_iter()
                    .flat_map(|m| m.depends_on.iter())
                    .map(|d| format!("tasks/{d}.md")),
            )
            .collect();
    }
    if STAGES.contains(&rel) {
        return STAGES
            .iter()
            .take_while(|s| **s != rel)
            .map(|s| s.to_string())
            .collect();
    }
    Vec::new()
}

/// `files` in the order decisions on them are recorded: stages 01 → 04,
/// then tasks, each after the tasks it depends on (by name otherwise; a
/// cycle keeps name order for what is left), then everything else.
pub fn decision_order(files: &[String], metas: &BTreeMap<String, TaskMeta>) -> Vec<String> {
    let set: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let mut out: Vec<String> = STAGES
        .iter()
        .filter(|s| set.contains(**s))
        .map(|s| s.to_string())
        .collect();
    let mut tasks: Vec<&str> = set
        .iter()
        .copied()
        .filter(|f| task_id(f).is_some())
        .collect();
    while !tasks.is_empty() {
        let next = tasks
            .iter()
            .position(|f| {
                upstream(f, metas)
                    .iter()
                    .all(|u| !tasks.contains(&u.as_str()))
            })
            .unwrap_or(0);
        out.push(tasks.remove(next).to_string());
    }
    out.extend(
        set.iter()
            .filter(|f| !STAGES.contains(*f) && task_id(f).is_none())
            .map(|f| f.to_string()),
    );
    out
}

/// Task frontmatter from each task's text as `text` gives it.
fn metas(files: &[String], text: impl Fn(&str) -> Option<String>) -> BTreeMap<String, TaskMeta> {
    files
        .iter()
        .filter_map(|f| {
            let t = task_id(f)?;
            let meta = lint::parse_task_meta(&text(f)?).ok()?;
            Some((t.to_string(), meta))
        })
        .collect()
}

/// What [`review_all`] did.
#[derive(Debug)]
pub struct ReviewAll {
    /// Files sent (or already under review as they are), in decision order.
    pub sent: Vec<(String, Reviewed)>,
    /// Accepted files left as they are: unchanged, and nothing they build
    /// on is sent.
    pub kept: Vec<String>,
}

/// Send every stage and task that needs the user's decision for review,
/// in dependency order: drafts, files changed since review or since their
/// accepted revision, files the user asked to change — and accepted files
/// that rest on one of those, or on something accepted since (D6), sent
/// again unchanged so the user confirms them in the same pass.
///
/// Atomic: every file is checked first, and when any has a structural
/// error, or is still exactly what the user asked to change, nothing is
/// sent and the error lists every problem.
pub fn review_all(intake: &Intake) -> Result<ReviewAll> {
    let _lock = review::lock(intake)?;
    let log = record::read(intake)?;
    let statuses: BTreeMap<String, _> = review::statuses(intake)?
        .into_iter()
        .filter(|s| STAGES.contains(&s.file.as_str()) || task_id(&s.file).is_some())
        .map(|s| (s.file.clone(), s))
        .collect();
    let files: Vec<String> = statuses.keys().cloned().collect();
    let read = |f: &str| std::fs::read_to_string(intake.dir.join(f)).ok();
    let metas = metas(&files, read);
    let stale = readiness::stale(intake)?;

    let mut selected: BTreeSet<String> = BTreeSet::new();
    let mut chosen = Vec::new();
    let mut kept = Vec::new();
    for f in decision_order(&files, &metas) {
        let s = &statuses[&f];
        let settled = s.state == DocState::Accepted && !s.has_draft;
        let send = !settled
            || stale.contains(&f)
            || upstream(&f, &metas).iter().any(|u| selected.contains(u));
        if send {
            selected.insert(f.clone());
            chosen.push(f);
        } else {
            kept.push(f);
        }
    }

    let mut problems = Vec::new();
    let mut ready = Vec::new();
    for f in chosen {
        let text = read(&f).ok_or_else(|| anyhow!("cannot read {f}"))?;
        let s = &statuses[&f];
        if s.state == DocState::NeedsRevision || s.revision_note.is_some() {
            let asked = log
                .iter()
                .rev()
                .find(|d| d.file == f && d.decision == DecisionKind::NeedsRevision);
            if let Some(d) = asked.filter(|d| d.sha256 == super::hash::sha256(&text)) {
                problems.push(format!(
                    "{f}:\n  - unchanged since the user asked for changes to revision {}: {}",
                    d.revision, d.note
                ));
                continue;
            }
        }
        match review::lint_for_review(intake, &f, &text) {
            Ok(warnings) => ready.push((f, text, warnings)),
            Err(errors) => problems.push(format!("{f}:\n{}", errors.join("\n"))),
        }
    }
    if !problems.is_empty() {
        bail!(
            "nothing sent for review; fix every file first:\n{}",
            problems.join("\n")
        );
    }

    let mut sent = Vec::new();
    for (f, text, warnings) in ready {
        let r = review::review_locked(intake, &f, &text, warnings, true)?;
        sent.push((f, r));
    }
    Ok(ReviewAll { sent, kept })
}

/// The outcome of one file in [`accept_all`].
pub type Decided = (String, Result<Rev>);

/// Record the user's acceptance of `files` (each under review) in
/// dependency order — stages, then tasks after what they depend on — under
/// one lock, each at a time no earlier than the one before, so none rests
/// on a file of the batch accepted after it. A file that cannot be
/// accepted (edited since review, say) is refused, and so is every file of
/// the batch that builds on it: accepting those would leave them resting
/// on a revision the user has not accepted yet.
pub fn accept_all(intake: &Intake, files: &[String], who: &Decider) -> Result<Vec<Decided>> {
    let _lock = review::lock(intake)?;
    let pending: BTreeMap<String, u32> = review::statuses(intake)?
        .into_iter()
        .filter_map(|s| Some((s.file, s.in_review?.revision)))
        .collect();
    let metas = metas(files, |f| {
        let rev = pending.get(f)?;
        record::read_snapshot(intake, f, *rev).ok()
    });
    let mut refused: BTreeSet<String> = BTreeSet::new();
    let mut last = Utc::now();
    let mut out = Vec::new();
    for f in decision_order(files, &metas) {
        let blocked = upstream(&f, &metas)
            .into_iter()
            .find(|u| refused.contains(u));
        let result = match blocked {
            Some(u) => Err(anyhow!(
                "not recorded: it builds on {u}, which was not accepted"
            )),
            None => {
                last = last.max(Utc::now());
                review::decide_locked(intake, &f, DecisionKind::Accepted, who, String::new(), last)
            }
        };
        if result.is_err() {
            refused.insert(f.clone());
        }
        out.push((f, result));
    }
    Ok(out)
}

/// Files of the intake sent for review and not decided on, in decision
/// order — including those edited since, so accepting them is refused out
/// loud rather than skipped.
pub fn under_review(intake: &Intake) -> Result<Vec<String>> {
    let statuses = review::statuses(intake)?;
    let files: Vec<String> = statuses
        .iter()
        .filter(|s| matches!(s.state, DocState::InReview | DocState::ChangedSinceReview))
        .map(|s| s.file.clone())
        .collect();
    let metas = metas(&files, |f| std::fs::read_to_string(intake.dir.join(f)).ok());
    Ok(decision_order(&files, &metas))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(deps: &[&str]) -> TaskMeta {
        TaskMeta {
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
            ..TaskMeta::default()
        }
    }

    #[test]
    fn stages_come_first_then_each_task_after_its_dependencies() {
        let files: Vec<String> = [
            "changes/CHANGE-1.md",
            "tasks/TASK-001.md",
            "tasks/TASK-002.md",
            "tasks/TASK-003.md",
            "04-breakdown.md",
            "01-outcome.md",
        ]
        .map(String::from)
        .to_vec();
        let metas = BTreeMap::from([
            ("TASK-001".to_string(), meta(&["TASK-003"])),
            ("TASK-002".to_string(), meta(&[])),
            ("TASK-003".to_string(), meta(&["TASK-002"])),
        ]);
        assert_eq!(
            decision_order(&files, &metas),
            [
                "01-outcome.md",
                "04-breakdown.md",
                "tasks/TASK-002.md",
                "tasks/TASK-003.md",
                "tasks/TASK-001.md",
                "changes/CHANGE-1.md"
            ]
        );
        // A cycle still yields every file once.
        let cyclic = BTreeMap::from([
            ("TASK-001".to_string(), meta(&["TASK-002"])),
            ("TASK-002".to_string(), meta(&["TASK-001"])),
        ]);
        assert_eq!(decision_order(&files, &cyclic).len(), files.len());
    }
}
