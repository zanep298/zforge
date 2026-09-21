//! Where a project's agents and skills live, computed once and used for
//! every place that names them: `.zforge/config.yaml`, the generated
//! instruction files (CLAUDE.md, AGENTS.md, .zforge/README.md) and the
//! language-skills table.
//!
//! FIX-013: shared mode keeps skills in the global store (`~/.zforge/skills`,
//! or `$ZFORGE_HOME/skills`), but the instruction files hardcoded
//! `.zforge/skills/<name>.md` — a path that does not exist in a shared-mode
//! project. Every skill reference an agent was told to load was dangling.
//! Local mode happened to work because it copies the skills into the
//! project. One resolver for all consumers keeps them from drifting apart
//! again.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StorePaths {
    /// Value for `paths.skills` in config.yaml.
    pub config_skills: String,
    /// Value for `paths.agents` in config.yaml.
    pub config_agents: String,
    /// Skills directory as written into instruction files an agent reads.
    /// Project-relative in local mode; absolute in shared mode, because
    /// agents' file tools do not expand `~` and must not have to guess.
    pub skills_ref: String,
}

impl StorePaths {
    /// Local mode: everything is copied into `<project>/.zforge/`.
    pub fn local() -> Self {
        Self {
            config_skills: "./.zforge/skills".into(),
            config_agents: "./.zforge/agents".into(),
            skills_ref: ".zforge/skills".into(),
        }
    }

    /// Shared mode: agents and skills live in the global store at
    /// `store_root` (which already honours `$ZFORGE_HOME`).
    pub fn shared(store_root: &Path, home: Option<&Path>) -> Self {
        let skills = store_root.join("skills");
        let agents = store_root.join("agents");
        Self {
            config_skills: config_form(&skills, home),
            config_agents: config_form(&agents, home),
            skills_ref: skills.display().to_string(),
        }
    }
}

/// `~/…` when under the home directory (config.yaml expands `~`), so the
/// file reads the same on every machine with a default install; absolute
/// otherwise, e.g. a custom `$ZFORGE_HOME`.
fn config_form(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rel) => format!("~/{}", rel.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn local_mode_is_project_relative() {
        let p = StorePaths::local();
        assert_eq!(p.skills_ref, ".zforge/skills");
        assert_eq!(p.config_skills, "./.zforge/skills");
    }

    #[test]
    fn shared_mode_under_home_uses_tilde_in_config_and_absolute_in_refs() {
        let home = PathBuf::from("/home/u");
        let p = StorePaths::shared(&home.join(".zforge"), Some(&home));
        assert_eq!(p.config_skills, "~/.zforge/skills");
        assert_eq!(p.config_agents, "~/.zforge/agents");
        assert_eq!(p.skills_ref, "/home/u/.zforge/skills");
    }

    // A custom $ZFORGE_HOME outside $HOME must not be rewritten as ~/.zforge.
    #[test]
    fn shared_mode_with_custom_store_uses_that_store_everywhere() {
        let home = PathBuf::from("/home/u");
        let p = StorePaths::shared(Path::new("/opt/zforge"), Some(&home));
        assert_eq!(p.config_skills, "/opt/zforge/skills");
        assert_eq!(p.config_agents, "/opt/zforge/agents");
        assert_eq!(p.skills_ref, "/opt/zforge/skills");
    }
}
