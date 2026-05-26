use crate::registry::schema::{ProjectEntry, Registry};
use anyhow::{anyhow, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9_-]{1,64}$").expect("regex literal"))
}

pub fn validate_name(name: &str) -> Result<()> {
    if name_re().is_match(name) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid project name {name:?}: must match {pat}",
            pat = name_re().as_str()
        ))
    }
}

pub fn sanitize_basename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    let s = if trimmed.is_empty() {
        "project"
    } else {
        trimmed
    };
    s.chars().take(64).collect()
}

pub fn canonicalize_path(p: &Path) -> Result<PathBuf> {
    let canon = std::fs::canonicalize(p).map_err(|e| anyhow!("canonicalize {p:?}: {e}"))?;
    if !canon.is_dir() {
        return Err(anyhow!("{canon:?} is not a directory"));
    }
    Ok(canon)
}

pub fn ensure_zforge_dir(p: &Path) -> Result<()> {
    let zf = p.join(".zforge");
    if !zf.is_dir() {
        return Err(anyhow!("{p:?} does not contain a .zforge/ directory"));
    }
    Ok(())
}

pub fn find_by_name<'a>(r: &'a Registry, name: &str) -> Option<&'a ProjectEntry> {
    r.projects.iter().find(|e| e.name == name)
}

pub fn find_by_path<'a>(r: &'a Registry, path: &Path) -> Option<&'a ProjectEntry> {
    r.projects.iter().find(|e| e.path == path)
}

pub fn ensure_unique_name(r: &Registry, name: &str) -> Result<()> {
    if find_by_name(r, name).is_some() {
        Err(anyhow!("project name {name:?} already registered"))
    } else {
        Ok(())
    }
}

pub fn next_available_name(r: &Registry, base: &str) -> String {
    if find_by_name(r, base).is_none() {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if find_by_name(r, &candidate).is_none() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::schema::{ProjectEntry, RegisteredBy};
    use chrono::Utc;
    use std::path::PathBuf;

    fn entry(name: &str) -> ProjectEntry {
        ProjectEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{name}")),
            registered_at: Utc::now(),
            registered_by: RegisteredBy::Manual,
            agent_overrides: Default::default(),
        }
    }

    #[test]
    fn validate_name_accepts_legal_chars() {
        assert!(validate_name("abc").is_ok());
        assert!(validate_name("abc-123").is_ok());
        assert!(validate_name("Abc_Def").is_ok());
        assert!(validate_name(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn validate_name_rejects_bad_inputs() {
        assert!(validate_name("").is_err());
        assert!(validate_name(&"a".repeat(65)).is_err());
        assert!(validate_name("bad name").is_err());
        assert!(validate_name("bad.name").is_err());
        assert!(validate_name("bad/name").is_err());
    }

    #[test]
    fn sanitize_basename_replaces_illegal_chars() {
        assert_eq!(sanitize_basename("My Project!"), "My-Project");
        assert_eq!(sanitize_basename("/"), "project");
        assert_eq!(sanitize_basename("good_name-1"), "good_name-1");
    }

    #[test]
    fn next_available_name_returns_base_when_free() {
        let r = Registry::default();
        assert_eq!(next_available_name(&r, "foo"), "foo");
    }

    #[test]
    fn next_available_name_finds_first_gap() {
        let mut r = Registry::default();
        r.projects.push(entry("foo"));
        assert_eq!(next_available_name(&r, "foo"), "foo-2");
        r.projects.push(entry("foo-2"));
        assert_eq!(next_available_name(&r, "foo"), "foo-3");
    }

    #[test]
    fn ensure_zforge_dir_errors_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(ensure_zforge_dir(tmp.path()).is_err());
        std::fs::create_dir(tmp.path().join(".zforge")).unwrap();
        assert!(ensure_zforge_dir(tmp.path()).is_ok());
    }
}
