//! `models.yaml` entries for phases that no longer exist.

use anyhow::{Context, Result};
use std::path::Path;

/// Phases of the task pipeline, removed in v1.5.
pub const REMOVED_PHASES: [&str; 3] = ["spec", "testspec", "plan"];

/// `(client, phase)` entries of removed phases in `text`.
fn removed_entries(text: &str) -> Vec<(String, &'static str)> {
    let Ok(serde_yaml::Value::Mapping(top)) = serde_yaml::from_str::<serde_yaml::Value>(text)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (client, phases) in &top {
        let (Some(client), serde_yaml::Value::Mapping(phases)) = (client.as_str(), phases) else {
            continue;
        };
        for phase in REMOVED_PHASES {
            if phases.contains_key(phase) {
                out.push((client.to_string(), phase));
            }
        }
    }
    out
}

pub fn has_removed_phases(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| !removed_entries(&t).is_empty())
}

/// The text of `models.yaml` without removed phases; comments kept.
pub fn without_removed_phases(text: &str) -> Result<String> {
    let mut out = text.to_string();
    for (client, phase) in removed_entries(text) {
        out = crate::cli::models::set_in_yaml(&out, &client, phase, None)?;
    }
    Ok(out)
}

/// Rewrite the file at `path` without removed phases.
pub fn drop_removed_phases(path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let out = without_removed_phases(&text)?;
    crate::fs::write_atomic(path, out.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_phases_go_and_the_rest_stays() {
        let text = "# mine\nclaude:\n  spec: haiku\n  plan: opus # think\n  code: sonnet\n\
                    codex:\n  testspec: x\n";
        let out = without_removed_phases(text).unwrap();
        assert_eq!(out, "# mine\nclaude:\n  code: sonnet\n");
        assert!(removed_entries(&out).is_empty());
    }

    #[test]
    fn nothing_to_do_on_a_comment_only_file() {
        let text = "# only comments\n";
        assert!(removed_entries(text).is_empty());
        assert_eq!(without_removed_phases(text).unwrap(), text);
    }
}
