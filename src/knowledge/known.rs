//! The baseline's known-failure list (ONBOARD REQ-002, REQ-010; business
//! rule 8): a test the baseline probe found failing, tolerated by name only
//! because the user recorded it at a terminal (`zforge onboard baseline
//! --known|--clear`).
//!
//! It is *not* part of `baseline.md` — the probe rewrites that file every
//! run (Output), so anything meant to survive a probe cannot live there.
//! It is recorded in knowledge's own decision log instead
//! (`intake::record`, shared with intake via `DocSet`), the same append-
//! only, terminal-only way an accept or a revision request is (D1). It has
//! no expiry: the latest recorded decision is the list in force, whatever
//! the baseline finds afterwards (03-solution "Binding decisions").

use super::Knowledge;
use crate::intake::hash;
use crate::intake::record::{self, Decision, DecisionKind, CHANNEL_TTY};
use anyhow::Result;
use chrono::Utc;

/// `Decision.file` for these decisions. Not a name any reviewable file
/// ever has (`Knowledge::FILES`), so it cannot collide with a real file's
/// review history.
pub const KEY: &str = "baseline.md#known";

/// Comma-separated, sorted and de-duplicated — a stable form so recording
/// the same set twice hashes the same and reads back the same order.
fn note_of(tests: &[String]) -> String {
    let mut cleaned: Vec<String> = tests
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    cleaned.sort();
    cleaned.dedup();
    cleaned.join(",")
}

fn from_note(note: &str) -> Vec<String> {
    if note.is_empty() {
        Vec::new()
    } else {
        note.split(',').map(str::to_string).collect()
    }
}

/// The known-failure list currently in force.
pub fn list(k: &Knowledge) -> Result<Vec<String>> {
    let log = record::read(k)?;
    let last = log
        .iter()
        .rfind(|d| d.file == KEY && d.decision == DecisionKind::BaselineKnown);
    Ok(last.map(|d| from_note(&d.note)).unwrap_or_default())
}

fn append(k: &Knowledge, tests: &[String], by: Option<String>) -> Result<Vec<String>> {
    // Same lock a review decision takes (`intake::review::lock`): two
    // `onboard baseline` calls, or one racing a review, never interleave.
    let _lock = crate::intake::review::lock(k)?;
    let log = record::read(k)?;
    let revision = log
        .iter()
        .filter(|d| d.file == KEY && d.decision == DecisionKind::BaselineKnown)
        .count() as u32
        + 1;
    let note = note_of(tests);
    record::append(
        k,
        &Decision {
            at: Utc::now(),
            file: KEY.to_string(),
            revision,
            sha256: hash::sha256(&note),
            decision: DecisionKind::BaselineKnown,
            channel: CHANNEL_TTY.into(),
            by,
            note: note.clone(),
        },
    )?;
    Ok(from_note(&note))
}

/// Record `tests` as known failures (`zforge onboard baseline --known`).
/// Replaces whatever list was in force, it does not add to it.
pub fn record_known(k: &Knowledge, tests: &[String], by: Option<String>) -> Result<Vec<String>> {
    append(k, tests, by)
}

/// Clear the known-failure list (`zforge onboard baseline --clear`).
pub fn clear(k: &Knowledge, by: Option<String>) -> Result<()> {
    append(k, &[], by)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &std::path::Path) {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
    }

    fn knowledge(root: &std::path::Path) -> Knowledge {
        Knowledge {
            dir: root.join("docs").join("knowledge"),
        }
    }

    #[test]
    fn empty_before_anything_is_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();
        assert_eq!(list(&k).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn records_sorted_and_deduplicated_and_can_be_cleared() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();

        let got = record_known(
            &k,
            &["TestB".into(), "TestA".into(), "TestB".into()],
            Some("zane".into()),
        )
        .unwrap();
        assert_eq!(got, vec!["TestA".to_string(), "TestB".to_string()]);
        assert_eq!(
            list(&k).unwrap(),
            vec!["TestA".to_string(), "TestB".to_string()]
        );

        clear(&k, Some("zane".into())).unwrap();
        assert_eq!(list(&k).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_later_decision_replaces_the_list_entirely() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();

        record_known(&k, &["TestA".into()], None).unwrap();
        record_known(&k, &["TestC".into()], None).unwrap();
        assert_eq!(list(&k).unwrap(), vec!["TestC".to_string()]);
    }
}
