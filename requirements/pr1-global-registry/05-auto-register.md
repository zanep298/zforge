# PR1-05 — Auto-register helper

## Goal

Implement the `auto_register` function used by `zforge init` to insert/update the current project in the global registry.

## Files touched

- `src/registry/auto.rs` (new)
- `src/registry/mod.rs` — re-export

## Implementation

```rust
use crate::registry::{
    io, lock, schema::{ProjectEntry, RegisteredBy, Registry}, validate,
};
use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum AutoResult {
    Registered { name: String },
    AlreadyExists { name: String },
    Suffixed { requested: String, final_name: String },
    Updated { old_name: String, new_name: String },
}

pub fn auto_register(
    cwd: &Path,
    name_override: Option<&str>,
    switch: bool,
) -> Result<AutoResult> {
    let canon = validate::canonicalize_path(cwd)?;
    validate::ensure_zforge_dir(&canon)?;

    let requested = match name_override {
        Some(n) => {
            validate::validate_name(n)?;
            n.to_string()
        }
        None => {
            let base = canon
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            validate::sanitize_basename(base)
        }
    };

    lock::with_lock(|| {
        let mut registry = io::load()?;
        let outcome = apply(&mut registry, &canon, &requested);
        if switch || registry.current_project.is_none() {
            // Determine the final name from the outcome.
            let final_name = match &outcome {
                AutoResult::Registered { name }
                | AutoResult::AlreadyExists { name }
                | AutoResult::Updated { new_name: name, .. }
                | AutoResult::Suffixed { final_name: name, .. } => name.clone(),
            };
            registry.current_project = Some(final_name);
        }
        io::save_atomic(&registry)?;
        Ok(outcome)
    })
}

fn apply(registry: &mut Registry, canon: &PathBuf, requested: &str) -> AutoResult {
    // 1. Same path, already registered.
    if let Some(existing) = validate::find_by_path(registry, canon) {
        if existing.name == requested {
            return AutoResult::AlreadyExists { name: existing.name.clone() };
        }
        // 3. Same path, different name → update.
        let old_name = existing.name.clone();
        let registered_at = existing.registered_at;
        let registered_by = existing.registered_by;
        // Remove old entry, insert new with preserved registered_at.
        registry.projects.retain(|e| e.path != *canon);
        registry.projects.push(ProjectEntry {
            name: requested.to_string(),
            path: canon.clone(),
            registered_at,
            registered_by,
            agent_overrides: Default::default(),
        });
        return AutoResult::Updated {
            old_name,
            new_name: requested.to_string(),
        };
    }

    // 2. Different path, same name → suffix.
    let final_name = validate::next_available_name(registry, requested);
    let suffixed = final_name != requested;

    registry.projects.push(ProjectEntry {
        name: final_name.clone(),
        path: canon.clone(),
        registered_at: Utc::now(),
        registered_by: RegisteredBy::Init,
        agent_overrides: Default::default(),
    });

    if suffixed {
        AutoResult::Suffixed {
            requested: requested.to_string(),
            final_name,
        }
    } else {
        AutoResult::Registered { name: final_name }
    }
}
```

## Acceptance criteria

- [ ] Fresh registry + new path → `AutoResult::Registered`.
- [ ] Same canonical path already registered with same name → `AutoResult::AlreadyExists`, no mutation.
- [ ] Same canonical path with different name override → `AutoResult::Updated`, `registered_at` preserved, `registered_by` preserved.
- [ ] Different path but name collision → `AutoResult::Suffixed`, with `final_name = base-2`.
- [ ] `switch = true` always sets `current_project`.
- [ ] `switch = false` only sets `current_project` if it was previously `None`.
- [ ] All writes happen under `with_lock`.
- [ ] Invalid name override is rejected before mutation.
- [ ] `cwd` lacking `.zforge/` is rejected with a clear error.

## Depends on

- PR1-03, PR1-04.

## Notes

- `registered_by` for new entries is always `Init` here. The CLI `project add` path uses `Manual`.
- `Updated` case preserves the original `registered_by` — a manually-added project that later gets renamed via `init --name` stays `Manual`.

---

```yaml
status: done
owner: ""
pr: ""
```
