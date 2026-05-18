//! Thin re-export shim over the canonical embedded template store at
//! `crate::embedded`. Kept so existing `super::registry::{...}` imports in
//! `cli::init` continue to compile while the data lives in one place.

pub(crate) use crate::embedded::{AGENTS, PROMPT_TEMPLATES, SKILLS};

pub(crate) fn prompt_templates() -> &'static [(&'static str, &'static str)] {
    PROMPT_TEMPLATES
}

pub(crate) fn agent_templates() -> &'static [(&'static str, &'static str)] {
    AGENTS
}

pub(crate) fn skill_templates() -> &'static [(&'static str, &'static str)] {
    SKILLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_prompt_template_name_ends_with_tmpl() {
        for (name, _) in PROMPT_TEMPLATES {
            assert!(
                name.ends_with(".tmpl"),
                "prompt template {name:?} must end with .tmpl"
            );
        }
    }

    #[test]
    fn every_agent_name_ends_with_agent_md() {
        for (name, _) in AGENTS {
            assert!(
                name.ends_with("-agent.md"),
                "agent {name:?} must end with -agent.md"
            );
        }
    }

    #[test]
    fn five_agent_files_for_five_pipeline_phases() {
        assert_eq!(AGENTS.len(), 5);
    }
}
