//! Append-only log of every verification of a task.
//!
//! `verify.md` is rewritten on each run, so earlier attempts — a pass that
//! was later invalidated, the failures a retry loop worked through — were
//! lost. Each run now also appends one JSON line to
//! `<tasks_dir>/<ID>/verify-history.jsonl`. Best-effort: a failure to append
//! is reported, never allowed to change the verification result.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifyRecord {
    pub ran_at: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub timed_out: bool,
    pub total_tests: usize,
    pub failed_tests: usize,
    pub command: String,
    /// Tree hash of the code verified, when one could be taken.
    pub candidate: Option<String>,
}

pub fn path(tasks_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir.join(task_id).join("verify-history.jsonl")
}

pub fn append(tasks_dir: &Path, task_id: &str, record: &VerifyRecord) -> anyhow::Result<()> {
    let line = serde_json::to_string(record)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path(tasks_dir, task_id))?;
    writeln!(f, "{line}")?;
    Ok(())
}

pub fn read(tasks_dir: &Path, task_id: &str) -> Vec<VerifyRecord> {
    std::fs::read_to_string(path(tasks_dir, task_id))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(passed: bool) -> VerifyRecord {
        VerifyRecord {
            ran_at: "2026-01-01T00:00:00Z".into(),
            passed,
            timed_out: false,
            total_tests: 2,
            failed_tests: if passed { 0 } else { 1 },
            command: "cargo test".into(),
            candidate: Some("abc".into()),
        }
    }

    #[test]
    fn appends_one_line_per_run_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("T1")).unwrap();
        append(tmp.path(), "T1", &rec(true)).unwrap();
        append(tmp.path(), "T1", &rec(false)).unwrap();
        let all = read(tmp.path(), "T1");
        assert_eq!(all.len(), 2);
        assert!(all[0].passed && !all[1].passed);
    }
}
