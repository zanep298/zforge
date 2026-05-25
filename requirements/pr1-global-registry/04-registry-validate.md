# PR1-04 — Validation helpers

## Goal

Centralize name, path, and uniqueness checks used by both `project add` and `init` auto-register.

## Files touched

- `src/registry/validate.rs` (new)
- `src/registry/mod.rs` — re-export

## Implementation

```rust
use crate::registry::schema::{ProjectEntry, Registry};
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;   // or use std::sync::OnceLock on MSRV 1.70+
use regex::Regex;
use std::path::{Path, PathBuf};

static NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_-]{1,64}$").expect("regex literal"));

pub fn validate_name(name: &str) -> Result<()> {
    if NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid project name {name:?}: must match {pat}",
            pat = NAME_RE.as_str()
        ))
    }
}

pub fn sanitize_basename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    let s = if trimmed.is_empty() { "project" } else { trimmed };
    s.chars().take(64).collect()
}

pub fn canonicalize_path(p: &Path) -> Result<PathBuf> {
    let canon = std::fs::canonicalize(p)
        .map_err(|e| anyhow!("canonicalize {p:?}: {e}"))?;
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
```

If `once_cell` is not a current dependency, use `std::sync::OnceLock<Regex>` (stable in 1.70+) to avoid a new dep:

```rust
use std::sync::OnceLock;
fn name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9_-]{1,64}$").expect("regex literal"))
}
```

## Acceptance criteria

- [ ] `validate_name` accepts `abc`, `abc-123`, `Abc_Def`, 64-char string of legal chars.
- [ ] `validate_name` rejects empty, 65-char, `bad name` (space), `bad.name` (dot), `bad/name`.
- [ ] `sanitize_basename("My Project!")` returns `"My-Project"`.
- [ ] `sanitize_basename("/")` returns `"project"` (fallback).
- [ ] `canonicalize_path` resolves symlinks and rejects non-directories.
- [ ] `ensure_zforge_dir` succeeds when `.zforge/` exists, errors otherwise.
- [ ] `next_available_name`: if `foo` taken, returns `foo-2`; if `foo` and `foo-2` taken, returns `foo-3`.
- [ ] Lookups are O(n) over `projects`; acceptable for expected size (<200).

## Depends on

- PR1-02.

## Notes

- Prefer `std::sync::OnceLock` to avoid adding `once_cell`; either is acceptable.
- `canonicalize` follows symlinks deliberately — registry stores the resolved physical path.

---

```yaml
status: done
owner: ""
pr: ""
```
