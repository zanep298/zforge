//! Embedded zforge templates baked into the binary via `include_str!`.
//!
//! Single source of truth for agent definitions, generic skill bundles, and
//! per-language skill bundles. Consumed by:
//! - `cli::init` — writes files to `.zforge/` or `~/.zforge/`
//! - `cli::install` — writes files to global `~/.zforge/` store

pub const AGENTS: &[(&str, &str)] = &[
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
    ("intake.md", include_str!("../templates/skills/intake.md")),
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

/// The global store (`~/.zforge`, or `$ZFORGE_HOME`). Same resolution as
/// the registry, so a custom `ZFORGE_HOME` moves templates, agents and
/// skills together with `registry.yaml` instead of splitting them.
pub fn global_store_dir() -> Option<std::path::PathBuf> {
    crate::registry::paths::registry_dir().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The global store keeps skills verbatim — nothing renders them — and
    /// runs and non-Claude clients read them from there. A `{{…}}` left in
    /// one would reach the agent as is.
    #[test]
    fn skills_hold_no_template_placeholders() {
        let all = SKILLS
            .iter()
            .map(|(n, b)| (n.to_string(), *b))
            .chain(all_lang_skills());
        for (name, body) in all {
            assert!(!body.contains("{{"), "{name} has a placeholder");
        }
    }

    #[test]
    fn no_embedded_template_is_empty() {
        for (name, body) in AGENTS.iter().chain(SKILLS) {
            assert!(
                !body.trim().is_empty(),
                "embedded template {name:?} is empty"
            );
        }
    }

    #[test]
    fn one_agent_file_per_run_phase() {
        let names: Vec<&str> = AGENTS.iter().map(|(n, _)| *n).collect();
        let expected: Vec<String> = crate::config::PHASES
            .iter()
            .map(|p| format!("{p}-agent.md"))
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn all_lang_skills_includes_every_language() {
        let langs: Vec<String> = all_lang_skills().into_iter().map(|(n, _)| n).collect();
        assert!(langs.iter().any(|n| n.starts_with("rust-")));
        assert!(langs.iter().any(|n| n.starts_with("flutter-")));
        assert!(langs.iter().any(|n| n.starts_with("ios-")));
    }
}
