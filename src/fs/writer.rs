use anyhow::Result;
use std::collections::HashMap;
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
