use crate::cost::schema::CostEntry;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `<project>/.zforge/cost-log.jsonl`
pub fn log_path(project_root: &Path) -> PathBuf {
    project_root.join(".zforge").join("cost-log.jsonl")
}

/// Append one entry. Best-effort: caller must not block work on a logging
/// failure (telemetry is observability, not business logic). Returns Result
/// so callers can surface unexpected I/O errors during debugging, but the
/// orchestrator hot path discards the result.
pub fn record(project_root: &Path, entry: &CostEntry) -> Result<()> {
    let path = log_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
    let line = serde_json::to_string(entry).context("serialize cost entry")?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {path:?}"))?;
    writeln!(f, "{line}").with_context(|| format!("write {path:?}"))?;
    Ok(())
}

/// Read every entry. Parse failures on individual lines are skipped with a
/// stderr warning so a corrupted line doesn't poison the whole report.
pub fn load_all(project_root: &Path) -> Result<Vec<CostEntry>> {
    let path = log_path(project_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read {path:?}"))?;
    let mut entries = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<CostEntry>(line) {
            Ok(e) => entries.push(e),
            Err(e) => {
                eprintln!(
                    "warning: skipping cost-log.jsonl line {}: {e}",
                    idx + 1
                );
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::schema::CostEntry;
    use chrono::Utc;

    fn sample() -> CostEntry {
        CostEntry {
            timestamp: Utc::now(),
            task_id: "T1".into(),
            phase: "plan".into(),
            agent: "claude".into(),
            model: Some("opus".into()),
            prompt_chars: 1200,
            stdout_chars: 800,
            stderr_chars: 0,
            duration_ms: 12_345,
            exit_code: 0,
            timed_out: false,
            est_input_tokens: 300,
            est_output_tokens: 200,
            reported_total_tokens: None,
            est_cost_usd: 0.0195,
        }
    }

    #[test]
    fn record_then_load_roundtrips_entry() {
        let tmp = tempfile::tempdir().unwrap();
        record(tmp.path(), &sample()).unwrap();
        let entries = load_all(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].task_id, "T1");
        assert_eq!(entries[0].phase, "plan");
    }

    #[test]
    fn load_returns_empty_when_log_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let entries = load_all(tmp.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn malformed_lines_skipped_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".zforge")).unwrap();
        let path = log_path(tmp.path());
        std::fs::write(&path, "not json\n").unwrap();
        record(tmp.path(), &sample()).unwrap();
        let entries = load_all(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1, "malformed line skipped, valid one kept");
    }
}
