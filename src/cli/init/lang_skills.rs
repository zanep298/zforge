//! Per-language skill bundles. Data lives in `crate::embedded`; this module
//! owns the AGENTS.md table renderer. Paths come from `StorePaths`, so the
//! table points at wherever the skills actually are (project or global
//! store) — see `store_paths`.

pub(crate) use crate::embedded::lang_skill_templates;

pub(crate) fn build_lang_skills_section(
    language: &str,
    skills: &[(String, &str)],
    skills_dir: &str,
) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let rows: Vec<String> = skills
        .iter()
        .map(|(name, _)| {
            let stem = std::path::Path::new(name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (purpose, _) = super::claude_skills::language_skill(language, &stem);
            format!("| `{}/{}` | {} |", skills_dir, name, purpose)
        })
        .collect();
    format!(
        "## Language Skills\n\nGeneral {} checklists — the project's conventions and nearby code win where they differ:\n\n| File | Purpose |\n|------|---------|\n{}\n",
        super::claude_skills::language_label(language),
        rows.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flutter_skills_listed() {
        let names: Vec<String> = lang_skill_templates("flutter")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            names,
            vec![
                "flutter-patterns.md",
                "flutter-testing.md",
                "flutter-ui-patterns.md",
            ]
        );
    }

    #[test]
    fn ios_skills_listed() {
        let names: Vec<String> = lang_skill_templates("ios")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            names,
            vec![
                "ios-patterns.md",
                "ios-testing.md",
                "ios-ui-patterns.md",
                "ios-snapshot-accessibility.md",
            ]
        );
    }

    #[test]
    fn android_skills_listed() {
        let names: Vec<String> = lang_skill_templates("android")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            names,
            vec![
                "android-patterns.md",
                "android-testing.md",
                "android-compose-ui.md",
                "android-instrumented-testing.md",
            ]
        );
    }

    #[test]
    fn unknown_language_returns_empty() {
        assert!(lang_skill_templates("cobol").is_empty());
    }

    #[test]
    fn build_section_renders_markdown_table() {
        let skills = lang_skill_templates("flutter");
        let section = build_lang_skills_section("flutter", &skills, ".zforge/skills");
        assert!(!section.contains("\\n"));
        assert!(section.contains("|------|---------|\n| `.zforge/skills/flutter-patterns.md`"));
        let shared = build_lang_skills_section("flutter", &skills, "/home/u/.zforge/skills");
        assert!(shared.contains("| `/home/u/.zforge/skills/flutter-patterns.md`"));
        assert!(section.contains("General Flutter (Dart) checklists"));
        // Each row says what its checklist is for, from the native catalog.
        let (patterns, _) =
            crate::cli::init::claude_skills::language_skill("flutter", "flutter-patterns");
        assert!(section.contains(&format!(
            "| `.zforge/skills/flutter-patterns.md` | {patterns} |"
        )));
        assert!(section.contains("General Flutter (Dart) testing checklist"));
        assert!(!section.contains("patterns and testing"));
    }

    #[test]
    fn build_section_empty_for_unknown_language() {
        let section = build_lang_skills_section("cobol", &[], ".zforge/skills");
        assert!(section.is_empty());
    }
}
