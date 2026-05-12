use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn set_frontmatter(path: &Path, key: &str, value: serde_yaml::Value) -> Result<()> {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let normalized = raw.replace("\r\n", "\n");

    let (mut fm, body) = if normalized.starts_with("---\n") {
        if let Some(end) = normalized[4..].find("\n---\n") {
            let fm_str = &normalized[4..4 + end];
            let body_start = 4 + end + 5;
            let fm: HashMap<String, serde_yaml::Value> =
                serde_yaml::from_str(fm_str).unwrap_or_default();
            (fm, normalized[body_start..].to_string())
        } else {
            (HashMap::new(), normalized)
        }
    } else {
        (HashMap::new(), normalized)
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

pub fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
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
    fn test_frontmatter_roundtrip() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "---\ntitle: test\n---\nbody").unwrap();
        set_frontmatter(f.path(), "reviewed", serde_yaml::Value::Bool(true)).unwrap();
        let raw = std::fs::read_to_string(f.path()).unwrap();
        let normalized = raw.replace("\r\n", "\n");
        let end = normalized[4..].find("\n---\n").unwrap();
        let fm_str = &normalized[4..4 + end];
        let fm: std::collections::HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(fm_str).unwrap();
        assert_eq!(fm.get("reviewed").and_then(|v| v.as_bool()), Some(true));
    }
}
