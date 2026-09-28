use anyhow::{Context, Result};
use std::path::Path;

/// Write an artifact atomically (temp file + fsync + rename): a crash
/// leaves the previous version or the new one, never a truncated file that
/// a later read would take as the whole report.
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::fs::write_atomic(path, content.as_bytes())
}

/// Append one record line to an append-only JSONL log and sync it.
///
/// A crash mid-append leaves a torn last line. Appending straight after it
/// would glue the new record onto the torn one, so the new record would be
/// unreadable too (and dropped by readers that skip a torn tail). The torn
/// tail — a record that never completed — is cut off first. Callers must
/// serialize appends (hold the log's lock).
pub fn append_record_line(path: &Path, line: &str) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let len = file.metadata()?.len();
    if len > 0 {
        file.seek(SeekFrom::Start(len - 1))?;
        let mut last = [0u8; 1];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            let mut text = Vec::with_capacity(len as usize);
            file.seek(SeekFrom::Start(0))?;
            file.read_to_end(&mut text)?;
            let keep = text.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
            file.set_len(keep as u64)?;
        }
    }
    file.seek(SeekFrom::End(0))?;
    let mut record = line.trim_end_matches('\n').to_string();
    record.push('\n');
    file.write_all(record.as_bytes())?;
    file.sync_all()
        .with_context(|| format!("sync {}", path.display()))
}
