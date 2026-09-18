//! Structured result of a business operation.
//!
//! A zforge operation has three distinct ways of not succeeding, and the
//! difference matters to every caller:
//!
//! - **`Err(_)`** — zforge itself could not carry out the request (config
//!   missing, task not found, IO failure). The operation did not happen.
//! - **[`OperationOutcome::Failed`]** — the operation ran and produced a
//!   negative result. Tests failed; the verifier budget ran out. Nothing is
//!   broken in zforge, but the caller must not treat this as success.
//! - **[`OperationOutcome::Blocked`]** — a precondition gate refused the
//!   request. No work was attempted and no state changed.
//!
//! Before this type existed, `Failed` and `Blocked` were both flattened into
//! `Ok(())` and surfaced as exit code 0, so `zforge verify` reported success
//! while `verify.md` recorded `passed: false`, and `ship --async` marked the
//! job successful on a red test suite.
//!
//! Every transport renders the same outcome its own way — the CLI maps it to
//! an exit code, the MCP server to `isError`, the job worker to a terminal job
//! status — but none of them may invent a different verdict.

use std::process::ExitCode;

/// Exit code for an operation that ran and produced a negative result.
pub const EXIT_FAILED: u8 = 1;
/// Exit code for an operation refused by a precondition gate.
pub const EXIT_BLOCKED: u8 = 2;
/// Exit code for an operation stopped by its time budget. Matches GNU
/// `timeout(1)`, which the orchestrator already synthesizes on spawn timeout.
pub const EXIT_TIMEOUT: u8 = 124;
/// Exit code for an operation cancelled by an operator. Matches the shell's
/// 128+SIGINT convention.
pub const EXIT_CANCELLED: u8 = 130;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationOutcome {
    Success,
    /// The operation ran; the result is negative. `reason` is user-facing.
    Failed {
        reason: String,
    },
    /// A gate refused the request. No work ran, no state changed.
    Blocked {
        reason: String,
    },
    /// The operation exceeded its time budget.
    Timeout {
        reason: String,
    },
    /// An operator cancelled the operation.
    Cancelled {
        reason: String,
    },
}

impl OperationOutcome {
    pub fn failed(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
        }
    }

    pub fn blocked(reason: impl Into<String>) -> Self {
        Self::Blocked {
            reason: reason.into(),
        }
    }

    #[allow(dead_code)]
    pub fn timeout(reason: impl Into<String>) -> Self {
        Self::Timeout {
            reason: reason.into(),
        }
    }

    #[allow(dead_code)]
    pub fn cancelled(reason: impl Into<String>) -> Self {
        Self::Cancelled {
            reason: reason.into(),
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// The user-facing explanation, or `None` on success.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Success => None,
            Self::Failed { reason }
            | Self::Blocked { reason }
            | Self::Timeout { reason }
            | Self::Cancelled { reason } => Some(reason),
        }
    }

    /// Short tag used in job records and log lines.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed { .. } => "failed",
            Self::Blocked { .. } => "blocked",
            Self::Timeout { .. } => "timeout",
            Self::Cancelled { .. } => "cancelled",
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Success => ExitCode::SUCCESS,
            Self::Failed { .. } => ExitCode::from(EXIT_FAILED),
            Self::Blocked { .. } => ExitCode::from(EXIT_BLOCKED),
            Self::Timeout { .. } => ExitCode::from(EXIT_TIMEOUT),
            Self::Cancelled { .. } => ExitCode::from(EXIT_CANCELLED),
        }
    }
}

impl From<()> for OperationOutcome {
    fn from(_: ()) -> Self {
        Self::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_has_no_reason() {
        assert!(OperationOutcome::Success.is_success());
        assert_eq!(OperationOutcome::Success.reason(), None);
    }

    #[test]
    fn non_success_variants_carry_a_reason() {
        for o in [
            OperationOutcome::failed("tests failed"),
            OperationOutcome::blocked("gate refused"),
            OperationOutcome::timeout("budget exceeded"),
            OperationOutcome::cancelled("operator cancelled"),
        ] {
            assert!(!o.is_success(), "{o:?} must not be success");
            assert!(o.reason().is_some(), "{o:?} must explain itself");
        }
    }

    // Guards the contract every transport depends on: a non-success outcome
    // never renders as exit code 0. This is the regression that FIX-001
    // closes — `verify` used to exit 0 on a red suite.
    #[test]
    fn only_success_maps_to_exit_zero() {
        assert_eq!(OperationOutcome::Success.exit_code(), ExitCode::SUCCESS);
        for o in [
            OperationOutcome::failed("x"),
            OperationOutcome::blocked("x"),
            OperationOutcome::timeout("x"),
            OperationOutcome::cancelled("x"),
        ] {
            assert_ne!(
                format!("{:?}", o.exit_code()),
                format!("{:?}", ExitCode::SUCCESS),
                "{o:?} must not exit 0"
            );
        }
    }

    #[test]
    fn labels_are_distinct() {
        let labels = [
            OperationOutcome::Success.label(),
            OperationOutcome::failed("x").label(),
            OperationOutcome::blocked("x").label(),
            OperationOutcome::timeout("x").label(),
            OperationOutcome::cancelled("x").label(),
        ];
        let mut sorted = labels.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }
}
