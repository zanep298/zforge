//! Embedded template registry. All zforge prompt templates, agent files, and
//! generic skill bundles are baked into the binary via `include_str!` so
//! `zforge init` works in offline / network-restricted environments. The
//! per-language skill bundles live in [`super::lang_skills`].

pub(crate) const PROMPT_TEMPLATES: &[(&str, &str)] = &[
    ("spec.tmpl", include_str!("../../../templates/spec.tmpl")),
    (
        "testspec.tmpl",
        include_str!("../../../templates/testspec.tmpl"),
    ),
    ("plan.tmpl", include_str!("../../../templates/plan.tmpl")),
    ("code.tmpl", include_str!("../../../templates/code.tmpl")),
    (
        "review.tmpl",
        include_str!("../../../templates/review.tmpl"),
    ),
    (
        "verify-analysis.tmpl",
        include_str!("../../../templates/verify-analysis.tmpl"),
    ),
];

pub(crate) const AGENTS: &[(&str, &str)] = &[
    (
        "spec-agent.md",
        include_str!("../../../templates/agents/spec-agent.md"),
    ),
    (
        "testspec-agent.md",
        include_str!("../../../templates/agents/testspec-agent.md"),
    ),
    (
        "plan-agent.md",
        include_str!("../../../templates/agents/plan-agent.md"),
    ),
    (
        "code-agent.md",
        include_str!("../../../templates/agents/code-agent.md"),
    ),
    (
        "review-agent.md",
        include_str!("../../../templates/agents/review-agent.md"),
    ),
];

pub(crate) const SKILLS: &[(&str, &str)] = &[
    (
        "clarify-spec.md",
        include_str!("../../../templates/skills/clarify-spec.md"),
    ),
    (
        "derive-test-cases.md",
        include_str!("../../../templates/skills/derive-test-cases.md"),
    ),
    (
        "implementation-planning.md",
        include_str!("../../../templates/skills/implementation-planning.md"),
    ),
    (
        "write-tests-first.md",
        include_str!("../../../templates/skills/write-tests-first.md"),
    ),
    (
        "implement-minimal-patch.md",
        include_str!("../../../templates/skills/implement-minimal-patch.md"),
    ),
    (
        "review-patch.md",
        include_str!("../../../templates/skills/review-patch.md"),
    ),
    (
        "debug.md",
        include_str!("../../../templates/skills/debug.md"),
    ),
    (
        "security-review.md",
        include_str!("../../../templates/skills/security-review.md"),
    ),
    (
        "performance-optimize.md",
        include_str!("../../../templates/skills/performance-optimize.md"),
    ),
    (
        "backend/api-contracts.md",
        include_str!("../../../templates/skills/backend/api-contracts.md"),
    ),
    (
        "backend/database-migrations.md",
        include_str!("../../../templates/skills/backend/database-migrations.md"),
    ),
    (
        "backend/observability.md",
        include_str!("../../../templates/skills/backend/observability.md"),
    ),
    (
        "backend/background-jobs.md",
        include_str!("../../../templates/skills/backend/background-jobs.md"),
    ),
    (
        "frontend/react-patterns.md",
        include_str!("../../../templates/skills/frontend/react-patterns.md"),
    ),
    (
        "frontend/frontend-testing.md",
        include_str!("../../../templates/skills/frontend/frontend-testing.md"),
    ),
    (
        "frontend/accessibility.md",
        include_str!("../../../templates/skills/frontend/accessibility.md"),
    ),
    (
        "frontend/figma-to-ui.md",
        include_str!("../../../templates/skills/frontend/figma-to-ui.md"),
    ),
    (
        "frontend/state-data-fetching.md",
        include_str!("../../../templates/skills/frontend/state-data-fetching.md"),
    ),
];

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
        // spec → testspec → plan → code → review. If a phase is added or
        // removed, this count is the canary that flags it.
        assert_eq!(AGENTS.len(), 5);
    }

    #[test]
    fn no_embedded_template_is_empty() {
        for (name, body) in PROMPT_TEMPLATES.iter().chain(AGENTS).chain(SKILLS) {
            assert!(
                !body.trim().is_empty(),
                "embedded template {name:?} is empty"
            );
        }
    }
}
