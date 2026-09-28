//! Thin re-export shim over the canonical embedded template store at
//! `crate::embedded`. Kept so existing `super::registry::{...}` imports in
//! `cli::init` continue to compile while the data lives in one place.

pub(crate) use crate::embedded::{AGENTS, SKILLS};

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
    fn every_agent_name_ends_with_agent_md() {
        for (name, _) in AGENTS {
            assert!(
                name.ends_with("-agent.md"),
                "agent {name:?} must end with -agent.md"
            );
        }
    }
}
