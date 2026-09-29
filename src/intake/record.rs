//! `.records/`: what the runtime saw and what the user decided (D1).
//!
//! - `revisions/<file>/<n>.md` — the exact text sent for review as revision
//!   `n` of that file. Runs read these, never the working file (D2).
//! - `decisions.jsonl` — append-only log of reviews, acceptances and
//!   revision requests, each bound to a revision and its hash.
//!
//! Nothing here is ever rewritten. The status of a file is derived from the
//! log ([`super::status`]).

#[cfg(test)]
use super::Intake;
use crate::knowledge::docs::DocSet;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// Revision sent for review (by anyone).
    Review,
    /// Revision accepted by the user at a terminal.
    Accepted,
    /// The user asked for changes to the revision under review.
    NeedsRevision,
    /// The user recorded (or cleared) the baseline's known-failure list at
    /// a terminal (ONBOARD TASK-003, business rule 8). Not bound to a
    /// reviewable file's revision/hash like the other three kinds — `file`
    /// names a stable key (`knowledge::known::KEY`), `note` the
    /// comma-separated test names (empty for a clear), `revision` an
    /// incrementing counter of these decisions only.
    BaselineKnown,
}

impl DecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Accepted => "accepted",
            Self::NeedsRevision => "needs_revision",
            Self::BaselineKnown => "baseline_known",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    pub at: DateTime<Utc>,
    /// Intake-relative path, e.g. `01-outcome.md`, `tasks/TASK-001.md`.
    pub file: String,
    pub revision: u32,
    pub sha256: String,
    pub decision: DecisionKind,
    /// How the decision was made: `cli` for reviews, `cli-tty` for a human
    /// at a terminal. Authority rests on the channel, not on a name.
    pub channel: String,
    /// Login name of the terminal user, for accepts and revision requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

pub const DECISIONS_FILE: &str = "decisions.jsonl";
pub const CHANNEL_CLI: &str = "cli";
pub const CHANNEL_TTY: &str = "cli-tty";

fn log_path<D: DocSet>(doc: &D) -> PathBuf {
    doc.records_dir().join(DECISIONS_FILE)
}

pub fn append<D: DocSet>(doc: &D, decision: &Decision) -> Result<()> {
    crate::fs::writer::append_record_line(&log_path(doc), &serde_json::to_string(decision)?)
}

/// Every decision, oldest first. A torn last line (crash mid-append) is
/// skipped: the decision it would have recorded did not happen.
pub fn read<D: DocSet>(doc: &D) -> Result<Vec<Decision>> {
    let Ok(text) = std::fs::read_to_string(log_path(doc)) else {
        return Ok(Vec::new());
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut decisions = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        match serde_json::from_str(line) {
            Ok(d) => decisions.push(d),
            Err(_) if i + 1 == lines.len() => {}
            Err(e) => anyhow::bail!(
                "{} line {} is corrupt ({e}); the decision log is append-only — restore it from version control",
                log_path(doc).display(),
                i + 1
            ),
        }
    }
    Ok(decisions)
}

/// `revisions/<doc.snapshot_key(file)>/<n>.md`.
pub fn snapshot_path<D: DocSet>(doc: &D, file: &str, revision: u32) -> PathBuf {
    doc.records_dir()
        .join("revisions")
        .join(doc.snapshot_key(file))
        .join(format!("{revision}.md"))
}

pub fn write_snapshot<D: DocSet>(doc: &D, file: &str, revision: u32, text: &str) -> Result<()> {
    let path = snapshot_path(doc, file, revision);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::fs::write_atomic(&path, text.as_bytes())
}

pub fn read_snapshot<D: DocSet>(doc: &D, file: &str, revision: u32) -> Result<String> {
    let path = snapshot_path(doc, file, revision);
    std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn intake(dir: &std::path::Path) -> Intake {
        Intake {
            id: "F".into(),
            dir: dir.to_path_buf(),
        }
    }

    fn decision(rev: u32, kind: DecisionKind) -> Decision {
        Decision {
            at: Utc::now(),
            file: "01-outcome.md".into(),
            revision: rev,
            sha256: "h".into(),
            decision: kind,
            channel: CHANNEL_CLI.into(),
            by: None,
            note: String::new(),
        }
    }

    #[test]
    fn log_appends_and_skips_only_a_torn_last_line() {
        let tmp = tempfile::tempdir().unwrap();
        let i = intake(tmp.path());
        append(&i, &decision(1, DecisionKind::Review)).unwrap();
        append(&i, &decision(1, DecisionKind::Accepted)).unwrap();
        let path = log_path(&i);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"{\"at\":").unwrap();
        assert_eq!(read(&i).unwrap().len(), 2);

        std::fs::write(&path, "garbage\n{}\n").unwrap();
        let err = read(&i).unwrap_err().to_string();
        assert!(err.contains("line 1 is corrupt"), "{err}");
    }

    #[test]
    fn snapshots_are_keyed_by_file_and_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let i = intake(tmp.path());
        write_snapshot(&i, "tasks/TASK-001.md", 2, "text").unwrap();
        assert!(
            snapshot_path(&i, "tasks/TASK-001.md", 2).ends_with("revisions/tasks__TASK-001/2.md")
        );
        assert_eq!(read_snapshot(&i, "tasks/TASK-001.md", 2).unwrap(), "text");
    }
}

#[cfg(test)]
mod torn_append_tests {
    use super::*;
    use std::io::Write;

    /// After a torn write, the next decision must be recorded, not merged
    /// into the torn line and then skipped as torn itself.
    #[test]
    fn a_decision_after_a_torn_line_is_not_lost() {
        let tmp = tempfile::tempdir().unwrap();
        let i = Intake {
            id: "F".into(),
            dir: tmp.path().to_path_buf(),
        };
        let d = |kind| Decision {
            at: Utc::now(),
            file: "01-outcome.md".into(),
            revision: 1,
            sha256: "h".into(),
            decision: kind,
            channel: CHANNEL_CLI.into(),
            by: None,
            note: String::new(),
        };
        append(&i, &d(DecisionKind::Review)).unwrap();
        let path = log_path(&i);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"{\"at\":\"2026").unwrap();
        append(&i, &d(DecisionKind::Accepted)).unwrap();

        let kinds: Vec<DecisionKind> = read(&i).unwrap().into_iter().map(|d| d.decision).collect();
        assert_eq!(kinds, [DecisionKind::Review, DecisionKind::Accepted]);
    }
}
