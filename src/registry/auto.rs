use crate::registry::{
    io, lock,
    schema::{ProjectEntry, RegisteredBy, Registry},
    validate,
};
use anyhow::Result;
use chrono::Utc;
use std::path::Path;

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

fn apply(registry: &mut Registry, canon: &Path, requested: &str) -> AutoResult {
    if let Some(existing) = validate::find_by_path(registry, canon) {
        if existing.name == requested {
            return AutoResult::AlreadyExists {
                name: existing.name.clone(),
            };
        }
        // Rename in place: keep registered_at, registered_by, and any
        // per-project agent_overrides the user may have hand-edited. Only
        // the human-facing `name` changes.
        let old_name = existing.name.clone();
        let registered_at = existing.registered_at;
        let registered_by = existing.registered_by;
        let agent_overrides = existing.agent_overrides.clone();
        registry.projects.retain(|e| e.path != canon);
        registry.projects.push(ProjectEntry {
            name: requested.to_string(),
            path: canon.to_path_buf(),
            registered_at,
            registered_by,
            agent_overrides,
        });
        return AutoResult::Updated {
            old_name,
            new_name: requested.to_string(),
        };
    }

    let final_name = validate::next_available_name(registry, requested);
    let suffixed = final_name != requested;

    registry.projects.push(ProjectEntry {
        name: final_name.clone(),
        path: canon.to_path_buf(),
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
