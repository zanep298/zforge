use anyhow::{Context, Result};
use indexmap::IndexMap;
use std::collections::HashSet;
use std::path::Path;

pub fn set_frontmatter(path: &Path, key: &str, value: serde_yaml::Value) -> Result<()> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "failed to read {} for frontmatter update: {e}",
                path.display()
            ))
        }
    };
    let normalized = raw.replace("\r\n", "\n");

    let (mut fm, body) = if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let fm_str = &rest[..end];
            let fm: IndexMap<String, serde_yaml::Value> =
                serde_yaml::from_str(fm_str).map_err(|e| {
                    anyhow::anyhow!(
                        "refusing to overwrite {}: existing frontmatter is malformed ({e})",
                        path.display()
                    )
                })?;
            (fm, rest[end + 5..].to_string())
        } else {
            (IndexMap::new(), normalized)
        }
    } else {
        (IndexMap::new(), normalized)
    };

    fm.insert(key.to_string(), value);

    let fm_str = serde_yaml::to_string(&fm)?;
    let result = format!("---\n{}---\n{}", fm_str, body);
    write_file(path, &result)
}

pub fn append_to_file(path: &Path, content: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Appends only lines whose key is not already present in the file or earlier in `lines`.
/// Dedup key: text before `:` in `- key: desc` lines, lowercased and trimmed.
/// Returns count of lines actually appended.
pub fn append_unique_lines(path: &Path, lines: &[String]) -> Result<usize> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut seen: HashSet<String> = existing
        .lines()
        .map(extract_pattern_key)
        .filter(|k| !k.is_empty())
        .collect();

    let mut to_append = String::new();
    let mut count = 0usize;

    for line in lines {
        let key = extract_pattern_key(line);
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        to_append.push_str(line);
        to_append.push('\n');
        count += 1;
    }

    if !to_append.is_empty() {
        append_to_file(path, &format!("\n{}", to_append))?;
    }

    Ok(count)
}

fn extract_pattern_key(line: &str) -> String {
    let trimmed = line.trim().trim_start_matches('-').trim();
    match trimmed.find(':') {
        Some(pos) => trimmed[..pos].trim().to_lowercase(),
        None => trimmed.to_lowercase(),
    }
}

/// Write an artifact atomically (temp file + fsync + rename): a crash
/// leaves the previous version or the new one, never a truncated file that
/// a later read would take as the whole report.
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::state::write_atomic(path, content.as_bytes())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn append_unique_lines_skips_duplicate_key() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "# Patterns\n\n- use Result: for fallible ops\n").unwrap();
        let lines = vec!["- use Result: new description".to_string()];
        let count = append_unique_lines(f.path(), &lines).unwrap();
        assert_eq!(count, 0);
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(content.matches("use Result").count(), 1);
    }

    #[test]
    fn append_unique_lines_appends_new_key() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "# Patterns\n\n- use Result: for fallible ops\n").unwrap();
        let lines = vec!["- prefer iterators: over manual loops".to_string()];
        let count = append_unique_lines(f.path(), &lines).unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("prefer iterators"));
    }

    #[test]
    fn append_unique_lines_case_insensitive_dedup() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "# Patterns\n\n- Use Anyhow: for errors\n").unwrap();
        let lines = vec!["- use anyhow: new context".to_string()];
        let count = append_unique_lines(f.path(), &lines).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn append_unique_lines_creates_file_if_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("new.md");
        let lines = vec!["- new pattern: description".to_string()];
        let count = append_unique_lines(&path, &lines).unwrap();
        assert_eq!(count, 1);
        assert!(path.exists());
    }

    #[test]
    fn append_unique_lines_dedup_within_batch() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("p.md");
        let lines = vec![
            "- same key: first".to_string(),
            "- same key: second".to_string(),
        ];
        let count = append_unique_lines(&path, &lines).unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches("same key").count(), 1);
    }

    #[test]
    fn append_unique_lines_dedup_handles_indented_bullets() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "# Patterns\n\n  - foo: indented existing\n").unwrap();
        let lines = vec!["- foo: new desc".to_string()];
        let count = append_unique_lines(f.path(), &lines).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn append_unique_lines_filters_partial_batch() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "# Patterns\n\n- existing key: old desc\n").unwrap();
        let lines = vec![
            "- existing key: duplicate".to_string(),
            "- new key: fresh".to_string(),
        ];
        let count = append_unique_lines(f.path(), &lines).unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("new key"));
        assert_eq!(content.matches("existing key").count(), 1);
    }

    #[test]
    fn test_set_frontmatter_preserves_body() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "---\ntitle: old\n---\n\nbody line 1\nbody line 2").unwrap();
        set_frontmatter(f.path(), "reviewed", serde_yaml::Value::Bool(true)).unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("body line 1"));
        assert!(result.contains("body line 2"));
        assert!(result.contains("reviewed: true"));
    }

    #[test]
    fn test_set_frontmatter_creates_block() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "body without frontmatter").unwrap();
        set_frontmatter(f.path(), "key", serde_yaml::Value::String("val".into())).unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.starts_with("---\n"));
        assert!(result.contains("key: val"));
        assert!(result.contains("body without frontmatter"));
    }

    #[test]
    fn set_frontmatter_errors_on_malformed_yaml_instead_of_wiping() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "---\n[: not valid yaml :]\n---\nbody\n").unwrap();
        let err = set_frontmatter(f.path(), "reviewed", serde_yaml::Value::Bool(true)).unwrap_err();
        assert!(err.to_string().contains("malformed"));
        let after = std::fs::read_to_string(f.path()).unwrap();
        assert!(after.contains("[: not valid yaml :]"));
        assert!(after.contains("body"));
    }

    #[test]
    fn test_frontmatter_roundtrip() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "---\ntitle: test\n---\nbody").unwrap();
        set_frontmatter(f.path(), "reviewed", serde_yaml::Value::Bool(true)).unwrap();
        let raw = std::fs::read_to_string(f.path()).unwrap();
        let normalized = raw.replace("\r\n", "\n");
        let end = normalized[4..].find("\n---\n").unwrap();
        let fm_str = &normalized[4..4 + end];
        let fm: indexmap::IndexMap<String, serde_yaml::Value> =
            serde_yaml::from_str(fm_str).unwrap();
        assert_eq!(fm.get("reviewed").and_then(|v| v.as_bool()), Some(true));
    }

    // Regression for HashMap-based frontmatter scrambling keys on every set call.
    #[test]
    fn set_frontmatter_preserves_existing_key_order() {
        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            "---\nid: \"TASK-1\"\ntype: verify\npassed: true\ntotal_tests: 3\npassed_tests: 3\nfailed_tests: 0\n---\nbody"
        )
        .unwrap();

        set_frontmatter(f.path(), "tokens", serde_yaml::Value::Number(42.into())).unwrap();
        set_frontmatter(
            f.path(),
            "model",
            serde_yaml::Value::String("runner".into()),
        )
        .unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        let body_start = content.find("\n---\n").unwrap();
        let fm = &content[4..body_start];
        let lines: Vec<&str> = fm.lines().filter_map(|l| l.split(':').next()).collect();

        // Original keys keep their relative order; appended keys land at the end.
        assert_eq!(
            lines,
            vec![
                "id",
                "type",
                "passed",
                "total_tests",
                "passed_tests",
                "failed_tests",
                "tokens",
                "model",
            ]
        );
    }
}
