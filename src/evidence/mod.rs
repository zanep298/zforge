//! Verification evidence and whether it still describes the code (IMP-002).
//!
//! `verify.md` records what a run proved; [`current_status`] decides whether
//! that proof still applies to the task as it is now. `review --done` signs
//! off only on evidence that is current: a passing run, of the configured
//! test command, against the code that is on disk.

pub mod candidate;
pub mod history;

pub use candidate::{fingerprint, Candidate};

use crate::fs::reader::MarkdownFile;
use std::path::Path;

/// Frontmatter key holding the candidate's tree hash (`null` when none).
pub const CANDIDATE_KEY: &str = "candidate";
/// Frontmatter key explaining a missing candidate.
pub const CANDIDATE_NOTE_KEY: &str = "candidate_note";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStatus {
    /// Passed, same command, same code.
    Current,
    /// Passed, but no fingerprint could be taken — neither now nor when
    /// verified (e.g. not a git repository). Cannot be tied to the code.
    Unbound { reason: String },
    /// Must not be used; the reason says what to do.
    Invalid { reason: String },
}

/// Judge `verify.md` against the current code and configured command.
pub fn current_status(
    verify_md: &Path,
    project_root: &Path,
    configured_command: &str,
) -> EvidenceStatus {
    let invalid = |reason: String| EvidenceStatus::Invalid { reason };
    let Ok(report) = MarkdownFile::read(verify_md) else {
        return invalid("no verification report (verify.md); run verify".into());
    };
    if !report.get_bool("passed") {
        return invalid("the latest verification failed (verify.md records passed: false)".into());
    }
    let recorded_command = report.get_str("command").unwrap_or("");
    if recorded_command != configured_command {
        return invalid(format!(
            "verification ran `{recorded_command}`, but the configured test command is \
             `{configured_command}`; run the configured suite"
        ));
    }
    let Some(recorded) = report.frontmatter.get(CANDIDATE_KEY) else {
        return invalid(
            "the report does not record which code it verified (written by an older \
             zforge); run verify again"
                .into(),
        );
    };
    let recorded = recorded.as_str().map(str::to_string);

    match (recorded, fingerprint(project_root)) {
        (Some(was), Candidate::Git(now)) if was == now => EvidenceStatus::Current,
        (Some(was), Candidate::Git(now)) => invalid(format!(
            "the code changed after it was verified (verified {}, now {}); run verify again",
            short(&was),
            short(&now)
        )),
        (Some(_), Candidate::Unavailable(why)) => invalid(format!(
            "cannot confirm the code is the one verified: {why}"
        )),
        (None, Candidate::Git(_)) => invalid(
            "the code was not fingerprinted when verified but can be now; run verify again".into(),
        ),
        (None, Candidate::Unavailable(why)) => EvidenceStatus::Unbound {
            reason: report
                .get_str(CANDIDATE_NOTE_KEY)
                .map(str::to_string)
                .unwrap_or(why),
        },
    }
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}
