//! Embedded zforge templates baked into the binary via `include_str!`.
//!
//! Single source of truth for prompt templates, agent definitions, generic
//! skill bundles, and per-language skill bundles. Consumed by:
//! - `cli::init` — writes files to `.zforge/` or `~/.zforge/`
//! - `cli::install` — writes files to global `~/.zforge/` store
//! - `prompt::engine` — falls back to embedded prompt templates when no
//!   disk file is found in `agents_dir`

pub const PROMPT_TEMPLATES: &[(&str, &str)] = &[
    ("spec.tmpl", include_str!("../templates/spec.tmpl")),
    ("testspec.tmpl", include_str!("../templates/testspec.tmpl")),
    ("plan.tmpl", include_str!("../templates/plan.tmpl")),
    ("code.tmpl", include_str!("../templates/code.tmpl")),
    ("review.tmpl", include_str!("../templates/review.tmpl")),
    (
        "verify-analysis.tmpl",
        include_str!("../templates/verify-analysis.tmpl"),
    ),
];

pub const AGENTS: &[(&str, &str)] = &[
    (
        "spec-agent.md",
        include_str!("../templates/agents/spec-agent.md"),
    ),
    (
        "testspec-agent.md",
        include_str!("../templates/agents/testspec-agent.md"),
    ),
    (
        "plan-agent.md",
        include_str!("../templates/agents/plan-agent.md"),
    ),
    (
        "code-agent.md",
        include_str!("../templates/agents/code-agent.md"),
    ),
    (
        "review-agent.md",
        include_str!("../templates/agents/review-agent.md"),
    ),
];

pub const SKILLS: &[(&str, &str)] = &[
    (
        "clarify-spec.md",
        include_str!("../templates/skills/clarify-spec.md"),
    ),
    (
        "derive-test-cases.md",
        include_str!("../templates/skills/derive-test-cases.md"),
    ),
    (
        "implementation-planning.md",
        include_str!("../templates/skills/implementation-planning.md"),
    ),
    (
        "write-tests-first.md",
        include_str!("../templates/skills/write-tests-first.md"),
    ),
    (
        "implement-minimal-patch.md",
        include_str!("../templates/skills/implement-minimal-patch.md"),
    ),
    (
        "review-patch.md",
        include_str!("../templates/skills/review-patch.md"),
    ),
    ("debug.md", include_str!("../templates/skills/debug.md")),
    (
        "security-review.md",
        include_str!("../templates/skills/security-review.md"),
    ),
    (
        "performance-optimize.md",
        include_str!("../templates/skills/performance-optimize.md"),
    ),
    (
        "backend/api-contracts.md",
        include_str!("../templates/skills/backend/api-contracts.md"),
    ),
    (
        "backend/database-migrations.md",
        include_str!("../templates/skills/backend/database-migrations.md"),
    ),
    (
        "backend/observability.md",
        include_str!("../templates/skills/backend/observability.md"),
    ),
    (
        "backend/background-jobs.md",
        include_str!("../templates/skills/backend/background-jobs.md"),
    ),
    (
        "frontend/react-patterns.md",
        include_str!("../templates/skills/frontend/react-patterns.md"),
    ),
    (
        "frontend/frontend-testing.md",
        include_str!("../templates/skills/frontend/frontend-testing.md"),
    ),
    (
        "frontend/accessibility.md",
        include_str!("../templates/skills/frontend/accessibility.md"),
    ),
    (
        "frontend/figma-to-ui.md",
        include_str!("../templates/skills/frontend/figma-to-ui.md"),
    ),
    (
        "frontend/state-data-fetching.md",
        include_str!("../templates/skills/frontend/state-data-fetching.md"),
    ),
];

/// All per-language skill bundles. Used by `zforge install` to populate the
/// global skills store with every language at once.
pub fn all_lang_skills() -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for lang in &[
        "rust",
        "go",
        "typescript",
        "python",
        "ios",
        "android",
        "flutter",
    ] {
        out.extend(lang_skill_templates(lang));
    }
    out
}

/// Per-language skill bundles. Returns `(filename, contents)` for the given
/// language. Unknown languages return an empty vec.
pub fn lang_skill_templates(language: &str) -> Vec<(String, &'static str)> {
    match language {
        "rust" => vec![
            (
                "rust-patterns.md".into(),
                include_str!("../templates/skills/lang/rust-patterns.md"),
            ),
            (
                "rust-testing.md".into(),
                include_str!("../templates/skills/lang/rust-testing.md"),
            ),
        ],
        "go" => vec![
            (
                "go-patterns.md".into(),
                include_str!("../templates/skills/lang/go-patterns.md"),
            ),
            (
                "go-testing.md".into(),
                include_str!("../templates/skills/lang/go-testing.md"),
            ),
        ],
        "typescript" => vec![
            (
                "typescript-patterns.md".into(),
                include_str!("../templates/skills/lang/typescript-patterns.md"),
            ),
            (
                "typescript-testing.md".into(),
                include_str!("../templates/skills/lang/typescript-testing.md"),
            ),
        ],
        "python" => vec![
            (
                "python-patterns.md".into(),
                include_str!("../templates/skills/lang/python-patterns.md"),
            ),
            (
                "python-testing.md".into(),
                include_str!("../templates/skills/lang/python-testing.md"),
            ),
        ],
        "ios" => vec![
            (
                "ios-patterns.md".into(),
                include_str!("../templates/skills/lang/ios-patterns.md"),
            ),
            (
                "ios-testing.md".into(),
                include_str!("../templates/skills/lang/ios-testing.md"),
            ),
            (
                "ios-ui-patterns.md".into(),
                include_str!("../templates/skills/lang/ios-ui-patterns.md"),
            ),
            (
                "ios-snapshot-accessibility.md".into(),
                include_str!("../templates/skills/lang/ios-snapshot-accessibility.md"),
            ),
        ],
        "android" => vec![
            (
                "android-patterns.md".into(),
                include_str!("../templates/skills/lang/android-patterns.md"),
            ),
            (
                "android-testing.md".into(),
                include_str!("../templates/skills/lang/android-testing.md"),
            ),
            (
                "android-compose-ui.md".into(),
                include_str!("../templates/skills/lang/android-compose-ui.md"),
            ),
            (
                "android-instrumented-testing.md".into(),
                include_str!("../templates/skills/lang/android-instrumented-testing.md"),
            ),
        ],
        "flutter" => vec![
            (
                "flutter-patterns.md".into(),
                include_str!("../templates/skills/lang/flutter-patterns.md"),
            ),
            (
                "flutter-testing.md".into(),
                include_str!("../templates/skills/lang/flutter-testing.md"),
            ),
            (
                "flutter-ui-patterns.md".into(),
                include_str!("../templates/skills/lang/flutter-ui-patterns.md"),
            ),
        ],
        _ => vec![],
    }
}

/// Look up a prompt template body by name (e.g. `"spec.tmpl"`).
pub fn prompt_template(name: &str) -> Option<&'static str> {
    PROMPT_TEMPLATES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, body)| *body)
}

/// Global zforge store directory (`~/.zforge/`). None if home dir cannot be
/// determined.
pub fn global_store_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".zforge"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_embedded_template_is_empty() {
        for (name, body) in PROMPT_TEMPLATES.iter().chain(AGENTS).chain(SKILLS) {
            assert!(
                !body.trim().is_empty(),
                "embedded template {name:?} is empty"
            );
        }
    }

    #[test]
    fn five_agent_files_for_five_pipeline_phases() {
        assert_eq!(AGENTS.len(), 5);
    }

    #[test]
    fn every_prompt_template_name_ends_with_tmpl() {
        for (name, _) in PROMPT_TEMPLATES {
            assert!(name.ends_with(".tmpl"));
        }
    }

    #[test]
    fn prompt_template_lookup_finds_spec() {
        assert!(prompt_template("spec.tmpl").is_some());
        assert!(prompt_template("nonexistent.tmpl").is_none());
    }

    #[test]
    fn all_lang_skills_includes_every_language() {
        let langs: Vec<String> = all_lang_skills().into_iter().map(|(n, _)| n).collect();
        assert!(langs.iter().any(|n| n.starts_with("rust-")));
        assert!(langs.iter().any(|n| n.starts_with("flutter-")));
        assert!(langs.iter().any(|n| n.starts_with("ios-")));
    }
}
