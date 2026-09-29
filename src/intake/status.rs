//! Status of an intake file, derived from the decision log and the file's
//! current hash (D1). Nothing about status is stored in the file itself.
//!
//! ```text
//! draft → in_review → accepted
//!              ↘ needs_revision → in_review
//! accepted → superseded when a later revision is accepted
//! ```
//!
//! A file whose accepted revision has since been edited is `accepted` with
//! a pending draft: the accepted revision stays in force until another is
//! accepted.

use super::hash;
use super::record::{Decision, DecisionKind};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocState {
    /// Never sent for review.
    Draft,
    /// The revision under review matches the file.
    InReview,
    /// Sent for review, then edited: the review no longer describes the file.
    ChangedSinceReview,
    /// The user asked for changes and the file has not been sent again
    /// (edited or not).
    NeedsRevision,
    Accepted,
}

impl DocState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InReview => "in_review",
            Self::ChangedSinceReview => "changed_since_review",
            Self::NeedsRevision => "needs_revision",
            Self::Accepted => "accepted",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rev {
    pub revision: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocStatus {
    pub file: String,
    pub state: DocState,
    /// The accepted revision in force, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<Rev>,
    /// The revision currently under review, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_review: Option<Rev>,
    /// The file differs from the accepted revision.
    pub has_draft: bool,
    /// Highest revision number used for this file.
    pub last_revision: u32,
    /// `None` when the file is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_sha256: Option<String>,
    /// Note of the latest revision request, while it applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_note: Option<String>,
}

/// Derive the status of `file` from `log` and its current text.
pub fn derive(file: &str, log: &[Decision], current: Option<&str>) -> DocStatus {
    let mut accepted: Option<Rev> = None;
    let mut in_review: Option<Rev> = None;
    let mut revision_note: Option<String> = None;
    let mut last_revision = 0;
    for d in log.iter().filter(|d| d.file == file) {
        last_revision = last_revision.max(d.revision);
        let rev = Rev {
            revision: d.revision,
            sha256: d.sha256.clone(),
        };
        match d.decision {
            DecisionKind::Review => {
                in_review = Some(rev);
                revision_note = None;
            }
            DecisionKind::Accepted => {
                accepted = Some(rev);
                in_review = None;
                revision_note = None;
            }
            DecisionKind::NeedsRevision => {
                in_review = None;
                revision_note = Some(d.note.clone());
            }
            // Not a file review decision (ONBOARD TASK-003) — `file` here
            // is never one of a document set's reviewable files, so this
            // is unreachable in practice; still handled so the match stays
            // exhaustive without a wildcard on a business-critical enum.
            DecisionKind::BaselineKnown => {}
        }
    }

    let current_sha256 = current.map(hash::sha256);
    let matches =
        |r: &Option<Rev>| matches!((r, &current_sha256), (Some(r), Some(h)) if &r.sha256 == h);
    let has_draft = !matches(&accepted);
    let state = if in_review.is_some() {
        if matches(&in_review) {
            DocState::InReview
        } else {
            DocState::ChangedSinceReview
        }
    } else if revision_note.is_some() && has_draft {
        DocState::NeedsRevision
    } else if accepted.is_some() {
        DocState::Accepted
    } else {
        DocState::Draft
    };
    if state != DocState::NeedsRevision {
        revision_note = None;
    }
    DocStatus {
        file: file.to_string(),
        state,
        accepted,
        in_review,
        has_draft: has_draft && current_sha256.is_some(),
        last_revision,
        current_sha256,
        revision_note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intake::record::CHANNEL_CLI;
    use chrono::Utc;

    fn d(rev: u32, text: &str, kind: DecisionKind) -> Decision {
        Decision {
            at: Utc::now(),
            file: "f.md".into(),
            revision: rev,
            sha256: hash::sha256(text),
            decision: kind,
            channel: CHANNEL_CLI.into(),
            by: None,
            note: if kind == DecisionKind::NeedsRevision {
                "clarify".into()
            } else {
                String::new()
            },
        }
    }

    use DecisionKind::{Accepted, NeedsRevision, Review};

    #[test]
    fn walks_the_document_lifecycle() {
        assert_eq!(derive("f.md", &[], Some("v1")).state, DocState::Draft);

        let mut log = vec![d(1, "v1", Review)];
        assert_eq!(derive("f.md", &log, Some("v1")).state, DocState::InReview);
        assert_eq!(
            derive("f.md", &log, Some("v1 edited")).state,
            DocState::ChangedSinceReview
        );

        log.push(d(1, "v1", NeedsRevision));
        let s = derive("f.md", &log, Some("v1"));
        assert_eq!(s.state, DocState::NeedsRevision);
        assert_eq!(s.revision_note.as_deref(), Some("clarify"));

        log.push(d(2, "v2", Review));
        log.push(d(2, "v2", Accepted));
        let s = derive("f.md", &log, Some("v2"));
        assert_eq!(s.state, DocState::Accepted);
        assert_eq!(s.accepted.as_ref().unwrap().revision, 2);
        assert!(!s.has_draft);
        assert_eq!(s.last_revision, 2);

        // Editing an accepted file: the accepted revision stays in force.
        let s = derive("f.md", &log, Some("v3 draft"));
        assert_eq!(s.state, DocState::Accepted);
        assert!(s.has_draft);
    }

    #[test]
    fn other_files_and_a_missing_file() {
        let log = vec![d(1, "v1", Review), d(1, "v1", Accepted)];
        assert_eq!(derive("g.md", &log, Some("x")).state, DocState::Draft);
        let s = derive("f.md", &log, None);
        assert_eq!(s.state, DocState::Accepted);
        assert!(!s.has_draft, "no file, no draft");
        assert!(s.current_sha256.is_none());
    }
}
