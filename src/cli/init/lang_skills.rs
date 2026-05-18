//! Per-language skill bundles installed under `.zforge/skills/`. Data lives
//! in `crate::embedded`; this module owns the AGENTS.md table renderer.

pub(crate) use crate::embedded::lang_skill_templates;

pub(crate) fn build_lang_skills_section(language: &str, skills: &[(String, &str)]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let rows: Vec<String> = skills
        .iter()
        .map(|(name, _)| {
            format!(
                "| `.zforge/skills/{}` | {} patterns and testing |",
                name, language
            )
        })
        .collect();
    format!(
        "## Language Skills\n\nRead these before writing any {} code:\n\n| File | Purpose |\n|------|---------|\n{}\n",
        language,
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
        let section = build_lang_skills_section("flutter", &skills);
        assert!(!section.contains("\\n"));
        assert!(section.contains("|------|---------|\n| `.zforge/skills/flutter-patterns.md`"));
    }

    #[test]
    fn build_section_empty_for_unknown_language() {
        let section = build_lang_skills_section("cobol", &[]);
        assert!(section.is_empty());
    }
}
