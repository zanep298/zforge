//! Starting content for intake files. Guidance lives in HTML comments,
//! which the linter ignores, so an untouched template reads as empty.

const OUTCOME: &str = include_str!("../../templates/intake/01-outcome.md");
const BEHAVIOR: &str = include_str!("../../templates/intake/02-behavior.md");
const SOLUTION: &str = include_str!("../../templates/intake/03-solution.md");
const BREAKDOWN: &str = include_str!("../../templates/intake/04-breakdown.md");
const TASK: &str = include_str!("../../templates/intake/task.md");
const CHANGE: &str = include_str!("../../templates/intake/change.md");
const BRIEF: &str = include_str!("../../templates/intake/brief.md");

/// Template for a stage file, by name.
pub fn stage(file: &str, intake_id: &str) -> Option<String> {
    let body = match file {
        "01-outcome.md" => OUTCOME,
        "02-behavior.md" => BEHAVIOR,
        "03-solution.md" => SOLUTION,
        "04-breakdown.md" => BREAKDOWN,
        _ => return None,
    };
    Some(body.replace("{{intake_id}}", intake_id))
}

const BEHAVIOR_SMALL: &str = include_str!("../../templates/intake/02-behavior-small.md");
const SOLUTION_SMALL: &str = include_str!("../../templates/intake/03-solution-small.md");

/// Template for a stage file of a small intake (`zforge intake new
/// --small`): 01-outcome marked `profile: small`, compact 02 and 03.
pub fn small_stage(file: &str, intake_id: &str) -> Option<String> {
    let body = match file {
        "01-outcome.md" => {
            return stage(file, intake_id).map(|t| format!("---\nprofile: small\n---\n\n{t}"))
        }
        "02-behavior.md" => BEHAVIOR_SMALL,
        "03-solution.md" => SOLUTION_SMALL,
        _ => return stage(file, intake_id),
    };
    Some(body.replace("{{intake_id}}", intake_id))
}

const OUTCOME_TINY: &str = include_str!("../../templates/intake/01-outcome-tiny.md");
const BEHAVIOR_TINY: &str = include_str!("../../templates/intake/02-behavior-tiny.md");
const SOLUTION_TINY: &str = include_str!("../../templates/intake/03-solution-tiny.md");
const BREAKDOWN_TINY: &str = include_str!("../../templates/intake/04-breakdown-tiny.md");

/// Template for a stage file of a tiny intake (`zforge intake new
/// --tiny`): a short 01-outcome marked `profile: tiny`, and 02/03/04 as
/// standard text pointing at the one task, TASK-001 ([`is_standard`]).
pub fn tiny_stage(file: &str, intake_id: &str) -> Option<String> {
    let body = match file {
        "01-outcome.md" => OUTCOME_TINY,
        _ => standard_template(super::lint::PROFILE_TINY, file)?,
    };
    Some(body.replace("{{intake_id}}", intake_id))
}

/// The raw template (placeholders left) of a stage zforge writes as
/// standard text for an intake of `profile`: 02/03/04 of a tiny or a fixup
/// intake.
fn standard_template(profile: &str, file: &str) -> Option<&'static str> {
    match (profile, file) {
        (super::lint::PROFILE_TINY, "02-behavior.md") => Some(BEHAVIOR_TINY),
        (super::lint::PROFILE_TINY, "03-solution.md") => Some(SOLUTION_TINY),
        (super::lint::PROFILE_TINY, "04-breakdown.md") => Some(BREAKDOWN_TINY),
        (super::lint::PROFILE_FIXUP, "02-behavior.md") => Some(BEHAVIOR_FIXUP),
        (super::lint::PROFILE_FIXUP, "03-solution.md") => Some(SOLUTION_FIXUP),
        (super::lint::PROFILE_FIXUP, "04-breakdown.md") => Some(BREAKDOWN_FIXUP),
        _ => None,
    }
}

/// Whether `text` of stage `file`, in an intake of `profile` (see
/// [`super::lint::profile`]), is still the standard text zforge wrote
/// for it — nothing of its own for the user to decide. Guidance comments,
/// frontmatter and blank lines do not count; each line must match the
/// template's, a `{{name}}` matching whatever was filled in on that line.
pub fn is_standard(profile: &str, file: &str, text: &str) -> bool {
    let Some(template) = standard_template(profile, file) else {
        return false;
    };
    let lines = |t: &str| -> Vec<String> {
        super::lint::strip_comments(super::lint::split_frontmatter(t).1)
            .lines()
            .map(|l| l.trim_end().to_string())
            .filter(|l| !l.trim().is_empty())
            .collect()
    };
    let want = lines(template);
    let have = lines(text);
    want.len() == have.len()
        && want
            .iter()
            .zip(&have)
            .all(|(w, h)| line_pattern(w).is_match(h))
}

/// A template line as an anchored pattern: literal text, each `{{name}}`
/// any text.
fn line_pattern(line: &str) -> regex::Regex {
    let mut pattern = String::from("^");
    let mut rest = line;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        pattern.push_str(&regex::escape(&rest[..start]));
        pattern.push_str(".*");
        rest = &rest[start + end + 2..];
    }
    pattern.push_str(&regex::escape(rest));
    pattern.push('$');
    regex::Regex::new(&pattern).expect("escaped template line")
}

const OUTCOME_FIXUP: &str = include_str!("../../templates/intake/01-outcome-fixup.md");
const BEHAVIOR_FIXUP: &str = include_str!("../../templates/intake/02-behavior-fixup.md");
const SOLUTION_FIXUP: &str = include_str!("../../templates/intake/03-solution-fixup.md");
const BREAKDOWN_FIXUP: &str = include_str!("../../templates/intake/04-breakdown-fixup.md");

/// Template for a stage file of a fixup intake (`zforge intake amend
/// --fixup`), with every `{{name}}` in `vars` filled in besides
/// `{{intake_id}}`.
pub fn fixup_stage(file: &str, intake_id: &str, vars: &[(&str, &str)]) -> Option<String> {
    let body = match file {
        "01-outcome.md" => OUTCOME_FIXUP,
        "02-behavior.md" => BEHAVIOR_FIXUP,
        "03-solution.md" => SOLUTION_FIXUP,
        "04-breakdown.md" => BREAKDOWN_FIXUP,
        _ => return None,
    };
    let mut text = body.replace("{{intake_id}}", intake_id);
    for (name, value) in vars {
        text = text.replace(&format!("{{{{{name}}}}}"), value);
    }
    Some(text)
}

pub fn task(intake_id: &str, task_id: &str) -> String {
    TASK.replace("{{intake_id}}", intake_id)
        .replace("{{task_id}}", task_id)
}

/// The intake's one-page reading view, for the agent to write in the user's
/// language (`super::BRIEF_FILE`).
pub fn brief(intake_id: &str) -> String {
    BRIEF.replace("{{intake_id}}", intake_id)
}

/// Skeleton of a change request (workflow §8).
pub fn change(intake_id: &str, change_id: &str) -> String {
    CHANGE
        .replace("{{intake_id}}", intake_id)
        .replace("{{change_id}}", change_id)
}

const QUESTIONS: &str = include_str!("../../templates/intake/questions.md");

/// Skeleton of a shared questions file (`.zforge/questions/<NAME>.md`).
pub fn questions(name: &str) -> String {
    QUESTIONS.replace("{{name}}", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard text stays standard however its comments and blank lines
    /// change; any edit of its own makes it the user's.
    #[test]
    fn standard_text_is_recognised_until_it_is_edited() {
        let b = tiny_stage("02-behavior.md", "F-1").unwrap();
        assert!(is_standard("tiny", "02-behavior.md", &b));
        let no_comment = crate::intake::lint::strip_comments(&b).replace("\n\n", "\n");
        assert!(is_standard("tiny", "02-behavior.md", &no_comment));
        let edited = format!("{b}\n## Situations\n\n| # | When | Then |\n");
        assert!(!is_standard("tiny", "02-behavior.md", &edited));
        let reworded = b.replace("acceptance criteria", "criteria");
        assert!(!is_standard("tiny", "02-behavior.md", &reworded));

        let vars = [
            ("from", "E-1"),
            ("origin", "E-1"),
            ("revision", "3"),
            ("req", "REQ-004"),
            ("snapshot", "x/3.md"),
        ];
        let f = fixup_stage("02-behavior.md", "F-1", &vars).unwrap();
        assert!(is_standard("fixup", "02-behavior.md", &f));
        let changed = f.replace(
            "- No other change.",
            "- Expired links answer 410 (REQ-004).",
        );
        assert!(!is_standard("fixup", "02-behavior.md", &changed));
    }

    #[test]
    fn every_stage_has_a_template_with_no_placeholders_left() {
        for file in crate::intake::STAGES {
            let t = stage(file, "F-1").unwrap();
            assert!(t.contains("F-1") && !t.contains("{{"), "{file}");
        }
        let t = task("F-1", "TASK-001");
        assert!(t.contains("id: TASK-001") && t.contains("parent: F-1") && !t.contains("{{"));
        let b = brief("F-1");
        assert!(b.contains("F-1") && !b.contains("{{"));
        let vars = [
            ("from", "E-1"),
            ("origin", "E-1, HANDOVER-001"),
            ("revision", "2"),
            ("req", "REQ-004"),
            ("snapshot", "x/2.md"),
        ];
        for file in crate::intake::STAGES {
            let t = fixup_stage(file, "F-1", &vars).unwrap();
            assert!(t.contains("F-1") && !t.contains("{{"), "{file}: {t}");
        }
        for file in crate::intake::STAGES {
            let t = tiny_stage(file, "F-1").unwrap();
            assert!(t.contains("F-1") && !t.contains("{{"), "{file}: {t}");
            assert_eq!(
                is_standard("tiny", file, &t),
                file != "01-outcome.md",
                "{file}"
            );
            let f = fixup_stage(file, "F-1", &vars).unwrap();
            assert_eq!(
                is_standard("fixup", file, &f),
                file != "01-outcome.md",
                "{file}"
            );
            assert!(!is_standard("small", file, &t), "{file}");
        }
        let c = change("F-1", "CHANGE-RUN-001");
        assert!(c.contains("CHANGE-RUN-001") && c.contains("F-1") && !c.contains("{{"));
        let q = questions("REF-V1");
        assert!(q.contains("Q:REF-V1/Q-1") && !q.contains("{{"));
    }
}
