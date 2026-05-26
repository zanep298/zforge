//! Per-target agent file generation.
//!
//! Each agent CLI (Claude Code, Codex, OpenCode) reads its own
//! `<dir>/agents/<phase>-agent.md` file and acts on the `model:` frontmatter
//! key inside. We keep a single multi-key source template in `.zforge/agents/`
//! (with `model:`, `codex_model:`, `opencode_model:` fields), then render per-
//! target copies that contain ONLY the relevant `model:` line.
//!
//! Resolution precedence for the final `model:` value:
//! 1. `models.yaml` `[target].<phase>` override (project local > global)
//! 2. Template frontmatter `<target>_model:` (or `model:` for claude)
//! 3. `default_model_for(target)` fallback
//!
//! This means editing `~/.zforge/models.yaml` and re-running `zforge init
//! --force` is the authoritative path for changing per-agent models — no need
//! to hand-edit `.claude/agents/` or `.codex/agents/`.
//!
//! Codex profile selection at orchestrator runtime still flows via
//! `~/.codex/config.toml` `[profiles.zforge_<phase>]`. The `model:` line in
//! `.codex/agents/<phase>-agent.md` is informational for the agent's own UI
//! and stays in sync with the profile model.

use crate::config::ModelsConfig;
use anyhow::{Context, Result};
use std::path::Path;

/// Pipeline phases that have an agent file. Mirrors `AGENT_NAMES` in the
/// init scaffold but keyed by phase rather than filename.
const PHASES: &[&str] = &["spec", "testspec", "plan", "code", "review"];

fn default_model_for(target: &str) -> &'static str {
    match target {
        "claude" => "claude-sonnet-4-6",
        "codex" => "gpt-5-codex",
        "opencode" => "claude-sonnet-4-6",
        _ => "unknown",
    }
}

/// Strip every `model:` / `codex_model:` / `opencode_model:` line from the
/// frontmatter and prepend a single `model: <resolved>` line right after
/// the opening `---`. Body untouched.
pub fn rewrite_frontmatter_with_model(content: &str, resolved_model: &str) -> String {
    // Locate frontmatter bounds: file must begin with `---\n` and contain a
    // second `---\n` line. Anything else: return content unchanged with a
    // synthetic frontmatter prepended so the file still works.
    let rest = match content.strip_prefix("---\n") {
        Some(r) => r,
        None => {
            return format!("---\nmodel: {resolved_model}\n---\n{content}");
        }
    };
    let Some(end_idx) = find_frontmatter_end(rest) else {
        return format!("---\nmodel: {resolved_model}\n---\n{content}");
    };
    let frontmatter_body = &rest[..end_idx];
    let body = &rest[end_idx + "---\n".len()..];

    let mut filtered = String::with_capacity(frontmatter_body.len());
    for line in frontmatter_body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("model:")
            || trimmed.starts_with("codex_model:")
            || trimmed.starts_with("opencode_model:")
        {
            continue;
        }
        filtered.push_str(line);
        filtered.push('\n');
    }

    let mut out = String::with_capacity(content.len());
    out.push_str("---\n");
    out.push_str(&format!("model: {resolved_model}\n"));
    out.push_str(&filtered);
    out.push_str("---\n");
    out.push_str(body);
    out
}

fn find_frontmatter_end(rest: &str) -> Option<usize> {
    // Look for a line that is exactly `---` (with newline).
    let mut idx = 0;
    for line in rest.split_inclusive('\n') {
        if line == "---\n" || line.trim_end_matches('\n') == "---" {
            return Some(idx);
        }
        idx += line.len();
    }
    None
}

/// Read `<target>_model:` (or `model:` for claude) from a template's
/// frontmatter. Returns None when the key is absent.
pub fn frontmatter_target_model(content: &str, target: &str) -> Option<String> {
    let rest = content.strip_prefix("---\n")?;
    let end = find_frontmatter_end(rest)?;
    let frontmatter = &rest[..end];
    let key = match target {
        "claude" => "model:",
        "codex" => "codex_model:",
        "opencode" => "opencode_model:",
        _ => return None,
    };
    for line in frontmatter.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(key) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Resolve the model for `(target, phase)` from `models.yaml` → frontmatter
/// → hard default in that order.
pub fn resolve_model(
    target: &str,
    phase: &str,
    template_content: &str,
    models: Option<&ModelsConfig>,
) -> String {
    if let Some(m) = models.and_then(|m| m.for_assistant(target, phase)) {
        return m.to_string();
    }
    if let Some(m) = frontmatter_target_model(template_content, target) {
        return m;
    }
    default_model_for(target).to_string()
}

/// Render per-target agent files into `dst_dir`. Returns the count of files
/// written. Source files are read from `src_agents_dir`; missing source
/// files for a given phase are skipped (this matches symlink behavior).
pub fn materialize_agents_into(
    target: &str,
    src_agents_dir: &Path,
    dst_dir: &Path,
    models: Option<&ModelsConfig>,
    force: bool,
) -> Result<usize> {
    std::fs::create_dir_all(dst_dir).with_context(|| format!("create {}", dst_dir.display()))?;
    let mut count = 0;
    for phase in PHASES {
        let filename = format!("{phase}-agent.md");
        let src = src_agents_dir.join(&filename);
        let Ok(content) = std::fs::read_to_string(&src) else {
            continue;
        };
        let resolved = resolve_model(target, phase, &content, models);
        let rewritten = rewrite_frontmatter_with_model(&content, &resolved);
        let dst = dst_dir.join(&filename);
        if dst.exists() {
            if force {
                std::fs::remove_file(&dst).with_context(|| format!("remove {}", dst.display()))?;
            } else {
                continue;
            }
        }
        std::fs::write(&dst, rewritten).with_context(|| format!("write {}", dst.display()))?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PhaseModels;

    fn template_with_three_keys() -> &'static str {
        "---\n\
name: code-agent\n\
description: x\n\
model: claude-sonnet-4-6\n\
codex_model: gpt-5-codex\n\
opencode_model: claude-sonnet-4-6\n\
temperature: 0.1\n\
---\n\
\n\
## Body\n\
text\n"
    }

    #[test]
    fn rewrite_replaces_all_three_model_keys_with_one() {
        let out = rewrite_frontmatter_with_model(template_with_three_keys(), "haiku");
        assert!(out.contains("model: haiku\n"));
        assert!(!out.contains("codex_model:"));
        assert!(!out.contains("opencode_model:"));
        // Exactly one `model:` line.
        assert_eq!(out.matches("model: ").count(), 1);
        // Body preserved.
        assert!(out.contains("## Body\ntext"));
    }

    #[test]
    fn rewrite_preserves_other_frontmatter_keys() {
        let out = rewrite_frontmatter_with_model(template_with_three_keys(), "haiku");
        assert!(out.contains("name: code-agent"));
        assert!(out.contains("temperature: 0.1"));
    }

    #[test]
    fn resolve_prefers_models_yaml_over_frontmatter() {
        let mut models = ModelsConfig::default();
        models.agents.insert(
            "codex".into(),
            PhaseModels {
                code: Some("gpt-5-mini".into()),
                ..PhaseModels::default()
            },
        );
        let resolved = resolve_model("codex", "code", template_with_three_keys(), Some(&models));
        assert_eq!(resolved, "gpt-5-mini");
    }

    #[test]
    fn resolve_falls_back_to_frontmatter_when_no_yaml_override() {
        let resolved = resolve_model("codex", "code", template_with_three_keys(), None);
        assert_eq!(resolved, "gpt-5-codex");
    }

    #[test]
    fn resolve_falls_back_to_default_when_template_lacks_key() {
        let stripped = "---\nname: x\n---\nbody\n";
        let resolved = resolve_model("codex", "code", stripped, None);
        assert_eq!(resolved, "gpt-5-codex");
    }

    #[test]
    fn frontmatter_target_model_reads_correct_key_per_target() {
        let c = template_with_three_keys();
        assert_eq!(
            frontmatter_target_model(c, "claude").as_deref(),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            frontmatter_target_model(c, "codex").as_deref(),
            Some("gpt-5-codex")
        );
        assert_eq!(
            frontmatter_target_model(c, "opencode").as_deref(),
            Some("claude-sonnet-4-6")
        );
    }

    #[test]
    fn materialize_writes_per_target_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("code-agent.md"), template_with_three_keys()).unwrap();
        let n = materialize_agents_into("codex", &src, &dst, None, false).unwrap();
        assert_eq!(n, 1);
        let written = std::fs::read_to_string(dst.join("code-agent.md")).unwrap();
        assert!(written.contains("model: gpt-5-codex"));
        assert!(!written.contains("codex_model:"));
    }

    #[test]
    fn materialize_skips_existing_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("code-agent.md"), template_with_three_keys()).unwrap();
        std::fs::write(dst.join("code-agent.md"), "preexisting").unwrap();
        let n = materialize_agents_into("codex", &src, &dst, None, false).unwrap();
        assert_eq!(n, 0);
        assert_eq!(
            std::fs::read_to_string(dst.join("code-agent.md")).unwrap(),
            "preexisting"
        );
    }

    #[test]
    fn materialize_force_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("code-agent.md"), template_with_three_keys()).unwrap();
        std::fs::write(dst.join("code-agent.md"), "preexisting").unwrap();
        let n = materialize_agents_into("codex", &src, &dst, None, true).unwrap();
        assert_eq!(n, 1);
        assert!(std::fs::read_to_string(dst.join("code-agent.md"))
            .unwrap()
            .contains("model: gpt-5-codex"));
    }
}
