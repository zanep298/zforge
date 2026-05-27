use crate::cli::artifact_metadata;
use crate::cli::dispatch_helper::run_phase_for_task;
use crate::cli::flow_guard;
use crate::config;
use crate::fs::{reader, writer};
use crate::prompt::{build_context_for_phase, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use colored::Colorize;

pub fn run(task_id: &str, done: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    flow_guard::ensure_phase_in_flow(&ts, State::Reviewed, "review")?;
    ts.require(State::Verified)?;

    if done {
        if !reader::artifact_exists(&tasks_dir, task_id, "review-summary.md") {
            anyhow::bail!(
                "review-summary.md not found or empty. Generate content before marking done."
            );
        }

        // Extract patterns from review-summary.md
        let summary_path = tasks_dir.join(task_id).join("review-summary.md");
        let summary = std::fs::read_to_string(&summary_path)?;
        artifact_metadata::set_llm_metadata(&config, &ts, "review", &summary_path, &summary)?;
        let (patterns_count, anti_count) = extract_and_update_memory(&config, &summary)?;

        ts.advance(State::Reviewed, "review complete")?;
        ts.save(&tasks_dir)?;

        println!("{} review-summary.md approved", "✓".green());
        if patterns_count > 0 {
            println!(
                "{} Extracted {} patterns → .zforge/memory/patterns.md",
                "✓".green(),
                patterns_count
            );
        }
        if anti_count > 0 {
            println!(
                "{} Extracted {} anti-pattern(s) → .zforge/memory/anti-patterns.md",
                "✓".green(),
                anti_count
            );
        }
        println!("{} State: Verified → Reviewed", "✓".green());
        println!();
        println!("{} {} complete!", "🎉".bold(), task_id);
        return Ok(());
    }

    let mut ctx = build_context_for_phase(&config, task_id, PromptPhase::Review)?;
    ctx.output_file = format!(".zforge/tasks/{}/review-summary.md", task_id);
    ctx.next_command = format!("zf review {} --done", task_id);

    run_phase_for_task(&config, &ts, "review", &ctx)?;

    Ok(())
}

fn extract_and_update_memory(
    config: &crate::config::Config,
    summary: &str,
) -> Result<(usize, usize)> {
    let memory_dir = config.memory_dir();
    let (patterns_lines, anti_lines) = parse_memory_sections(summary);

    let patterns_count =
        writer::append_unique_lines(&memory_dir.join("patterns.md"), &patterns_lines)?;
    let anti_count =
        writer::append_unique_lines(&memory_dir.join("anti-patterns.md"), &anti_lines)?;

    Ok((patterns_count, anti_count))
}

/// Section headings the review template emits. The review-summary.md parser
/// matches these substrings to locate the pattern/anti-pattern bullet blocks.
/// Keep these in sync with `templates/review.tmpl`.
pub(crate) const PATTERNS_HEADING: &str = "New approved patterns";
pub(crate) const ANTIPATTERNS_HEADING: &str = "Anti-patterns discovered";

/// Pure parser: pulls bullet lines from the patterns and anti-patterns
/// sections of a review summary. A subsequent `## ` heading closes the
/// active section. Returned lines preserve their original indentation so
/// `append_unique_lines` can dedup against existing memory files verbatim.
pub(crate) fn parse_memory_sections(summary: &str) -> (Vec<String>, Vec<String>) {
    let mut patterns_lines: Vec<String> = Vec::new();
    let mut anti_lines: Vec<String> = Vec::new();

    let mut in_patterns = false;
    let mut in_anti = false;

    for line in summary.lines() {
        if line.contains(PATTERNS_HEADING) {
            in_patterns = true;
            in_anti = false;
            continue;
        }
        if line.contains(ANTIPATTERNS_HEADING) {
            in_anti = true;
            in_patterns = false;
            continue;
        }
        if line.starts_with("## ") && (in_patterns || in_anti) {
            in_patterns = false;
            in_anti = false;
        }

        if in_patterns && line.trim_start().starts_with("- ") {
            patterns_lines.push(line.to_string());
        }
        if in_anti && line.trim_start().starts_with("- ") {
            anti_lines.push(line.to_string());
        }
    }

    (patterns_lines, anti_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_patterns_section() {
        let summary = "\
## Summary
text

## New approved patterns
- use Result: for fallible ops
- prefer iterators: over manual loops

## Anti-patterns discovered
- avoid unwrap: fail loudly with context
";
        let (p, a) = parse_memory_sections(summary);
        assert_eq!(
            p,
            vec![
                "- use Result: for fallible ops",
                "- prefer iterators: over manual loops",
            ]
        );
        assert_eq!(a, vec!["- avoid unwrap: fail loudly with context"]);
    }

    #[test]
    fn next_heading_closes_active_section() {
        let summary = "\
## New approved patterns
- pattern A: desc

## Notes
- not a pattern: ignore me

## Anti-patterns discovered
- anti A: bad
";
        let (p, a) = parse_memory_sections(summary);
        assert_eq!(p, vec!["- pattern A: desc"]);
        assert_eq!(a, vec!["- anti A: bad"]);
    }

    #[test]
    fn ignores_non_bullet_lines() {
        let summary = "\
## New approved patterns
A paragraph that is not a bullet.
- real bullet: keep this
";
        let (p, _) = parse_memory_sections(summary);
        assert_eq!(p, vec!["- real bullet: keep this"]);
    }

    #[test]
    fn empty_summary_returns_empty_vecs() {
        let (p, a) = parse_memory_sections("");
        assert!(p.is_empty());
        assert!(a.is_empty());
    }

    #[test]
    fn missing_sections_return_empty_vecs() {
        let summary = "## Other\n- bullet: nope\n";
        let (p, a) = parse_memory_sections(summary);
        assert!(p.is_empty());
        assert!(a.is_empty());
    }

    // Compile-time guarantee that the bundled review prompt keeps emitting the
    // exact section headings the parser scans for. If a future edit to
    // review.tmpl renames either heading, this test fails at the same commit
    // — extraction would otherwise silently stop populating memory files.
    #[test]
    fn review_template_emits_expected_headings() {
        const REVIEW_TMPL: &str = include_str!("../../templates/review.tmpl");
        assert!(
            REVIEW_TMPL.contains(PATTERNS_HEADING),
            "templates/review.tmpl no longer contains heading {PATTERNS_HEADING:?}; \
             update PATTERNS_HEADING in src/cli/review.rs or restore the heading text"
        );
        assert!(
            REVIEW_TMPL.contains(ANTIPATTERNS_HEADING),
            "templates/review.tmpl no longer contains heading {ANTIPATTERNS_HEADING:?}; \
             update ANTIPATTERNS_HEADING in src/cli/review.rs or restore the heading text"
        );
    }
}
