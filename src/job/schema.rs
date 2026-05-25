use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lifecycle state of a background job. Terminal states (`Success`, `Failed`,
/// `Cancelled`, `Timeout`) freeze `finished_at` and never transition further.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Success,
    Failed,
    Cancelled,
    Timeout,
}

impl JobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Success | JobStatus::Failed | JobStatus::Cancelled | JobStatus::Timeout
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
            JobStatus::Timeout => "timeout",
        }
    }
}

/// Which kind of work the worker performs. PR5 only ships `Ship` (full
/// code+verify with optional verifier loop). Individual phase async lands
/// later when there's demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Ship,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Ship => "ship",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub job_id: String,
    pub task_id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    // Ship-specific params — persisted so the worker can reconstruct the
    // call without inheriting CLI argv.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_override: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_iterations")]
    pub max_iterations: u32,
}

fn default_timeout() -> u64 {
    600
}
fn default_iterations() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_classified() {
        assert!(JobStatus::Success.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(JobStatus::Timeout.is_terminal());
        assert!(!JobStatus::Queued.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
    }

    #[test]
    fn legacy_yaml_without_new_fields_loads() {
        let raw = r#"
job_id: J-1
task_id: T-1
kind: ship
status: queued
created_at: 2026-01-01T00:00:00Z
"#;
        let j: Job = serde_yaml::from_str(raw).unwrap();
        assert_eq!(j.timeout_secs, 600);
        assert_eq!(j.max_iterations, 1);
        assert!(j.worker_pid.is_none());
    }
}
