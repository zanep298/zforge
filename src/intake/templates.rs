//! Starting content for intake files. Guidance lives in HTML comments,
//! which the linter ignores, so an untouched template reads as empty.

const OUTCOME: &str = include_str!("../../templates/intake/01-outcome.md");
const BEHAVIOR: &str = include_str!("../../templates/intake/02-behavior.md");
const SOLUTION: &str = include_str!("../../templates/intake/03-solution.md");
const BREAKDOWN: &str = include_str!("../../templates/intake/04-breakdown.md");
const TASK: &str = include_str!("../../templates/intake/task.md");

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

pub fn task(intake_id: &str, task_id: &str) -> String {
    TASK.replace("{{intake_id}}", intake_id)
        .replace("{{task_id}}", task_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_has_a_template_with_no_placeholders_left() {
        for file in crate::intake::STAGES {
            let t = stage(file, "F-1").unwrap();
            assert!(t.contains("F-1") && !t.contains("{{"), "{file}");
        }
        let t = task("F-1", "TASK-001");
        assert!(t.contains("id: TASK-001") && t.contains("parent: F-1") && !t.contains("{{"));
    }
}
