//! `<tasks_dir>/<ID>/trace.jsonl`: one [`PhaseTrace`] per line, append-only.

use super::schema::PhaseTrace;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn path(tasks_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir.join(task_id).join("trace.jsonl")
}

pub fn append(tasks_dir: &Path, record: &PhaseTrace) -> Result<()> {
    let path = path(tasks_dir, &record.task_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(record)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("append {}", path.display()))
}

/// Every record, oldest first. Lines that do not parse (a torn write, a
/// newer schema) are skipped and counted so the caller can say so.
pub fn read(tasks_dir: &Path, task_id: &str) -> (Vec<PhaseTrace>, usize) {
    let Ok(text) = std::fs::read_to_string(path(tasks_dir, task_id)) else {
        return (Vec::new(), 0);
    };
    let mut skipped = 0;
    let records = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let parsed = serde_json::from_str(l).ok();
            if parsed.is_none() {
                skipped += 1;
            }
            parsed
        })
        .collect();
    (records, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::schema::Expected;

    fn record(phase: &str) -> PhaseTrace {
        crate::trace::uncaptured("T1", phase, "claude", Expected::default())
    }

    #[test]
    fn appends_and_reads_back_in_order_skipping_torn_lines() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), &record("review")).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(path(tmp.path(), "T1"))
            .unwrap()
            .write_all(b"{\"torn\n")
            .unwrap();
        append(tmp.path(), &record("code")).unwrap();

        let (records, skipped) = read(tmp.path(), "T1");
        let phases: Vec<&str> = records.iter().map(|r| r.phase.as_str()).collect();
        assert_eq!(phases, ["review", "code"]);
        assert_eq!(skipped, 1);
    }
}
