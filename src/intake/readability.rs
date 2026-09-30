//! Whether a file is easy to decide on: warnings only, shown when a file
//! is sent for review. Readiness does not run them — an accepted file is
//! never made unready for being long.
//!
//! The user accepts what they read, so what they read must be short and
//! say what is being decided: a `## Summary` on top of each stage, a word
//! budget for everything above an optional `## Detail` (where what only
//! the implementing agent needs goes), requirements of one sentence, and
//! diagrams as `mermaid` rather than drawn in text.

use super::lint::{section, sections, split_frontmatter, strip_comments, Heading, Issue};
use super::STAGES;

pub const SUMMARY: Heading = Heading::new("Summary", "Tóm tắt");
pub const DETAIL: Heading = Heading::new("Detail", "Chi tiết");

/// Words a summary may take.
const SUMMARY_WORDS: usize = 150;
/// Words one requirement's own line may take; conditions go in sub-items.
const REQUIREMENT_WORDS: usize = 25;
/// Characters that only a diagram drawn in text uses.
const DRAWING: &str = "│┃║▼▲►◄┌┐└┘├┤┬┴";

/// Words the user has to read before deciding: the file above `## Detail`.
fn budget(rel: &str) -> Option<usize> {
    match rel {
        "01-outcome.md" => Some(500),
        "02-behavior.md" => Some(700),
        "03-solution.md" => Some(800),
        "04-breakdown.md" => Some(400),
        _ if rel.starts_with("tasks/") => Some(300),
        _ => None,
    }
}

/// Readability warnings for the intake file `rel`.
pub fn check(rel: &str, text: &str) -> Vec<Issue> {
    let Some(budget) = budget(rel) else {
        return Vec::new();
    };
    let (_, body) = split_frontmatter(text);
    let body = strip_comments(body);
    let mut issues = Vec::new();

    if STAGES.contains(&rel) {
        match section(&sections(&body), SUMMARY) {
            None => issues.push(Issue::warning(
                rel,
                format!(
                    "has no \"{SUMMARY}\" section: open with what this file decides, the \
                     main points and what the user must decide (at most {SUMMARY_WORDS} words)"
                ),
            )),
            Some(lines) => {
                let n = words(lines.iter().map(String::as_str));
                if n > SUMMARY_WORDS {
                    issues.push(Issue::warning(
                        rel,
                        format!("\"{SUMMARY}\" is {n} words; keep it to {SUMMARY_WORDS}"),
                    ));
                }
            }
        }
    }

    let to_read = words(above_detail(&body));
    if to_read > budget {
        issues.push(Issue::warning(
            rel,
            format!(
                "{to_read} words to read before deciding (budget {budget}): tighten it, refer \
                 to IDs instead of retelling, or move what only the implementing agent needs \
                 under \"## {DETAIL}\""
            ),
        ));
    }

    if rel == super::lint::OUTCOME {
        for line in body.lines() {
            let Some(id) = requirement_id(line) else {
                continue;
            };
            let n = line.split_whitespace().count();
            if n > REQUIREMENT_WORDS {
                issues.push(Issue::warning(
                    rel,
                    format!(
                        "{id} is {n} words: state it in one sentence (at most \
                         {REQUIREMENT_WORDS} words) and put its conditions in sub-items"
                    ),
                ));
            }
        }
    }

    if draws_in_text(&body) {
        issues.push(Issue::warning(
            rel,
            "draws a diagram in text: use a ```mermaid block, one question per diagram, \
             about eight nodes",
        ));
    }
    issues
}

/// The lines above `## Detail`.
fn above_detail(body: &str) -> impl Iterator<Item = &str> {
    body.lines().take_while(|l| {
        !l.strip_prefix("## ")
            .is_some_and(|title| DETAIL.matches(title))
    })
}

/// Words in `lines`, leaving out fenced blocks (diagrams, commands,
/// examples) and tokens without a letter or digit (table rules, bullets).
fn words<'a>(lines: impl Iterator<Item = &'a str>) -> usize {
    let mut in_fence = false;
    let mut n = 0;
    for line in lines {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            n += line
                .split_whitespace()
                .filter(|w| w.chars().any(char::is_alphanumeric))
                .count();
        }
    }
    n
}

/// `REQ-001` when `line` defines it (`- REQ-001: …`).
fn requirement_id(line: &str) -> Option<&str> {
    let item = line
        .trim_start()
        .strip_prefix("- ")
        .or_else(|| line.trim_start().strip_prefix("* "))?;
    let id = item
        .trim_start_matches("**")
        .split([':', ' ', '*'])
        .next()?;
    (id.starts_with("REQ-") && id[4..].chars().all(|c| c.is_ascii_digit()) && id.len() > 4)
        .then_some(id)
}

/// A fenced block that is not `mermaid` and uses box-drawing characters.
fn draws_in_text(body: &str) -> bool {
    let mut fence: Option<bool> = None; // Some(is_mermaid) inside a block
    for line in body.lines() {
        if let Some(lang) = line.trim_start().strip_prefix("```") {
            fence = match fence {
                None => Some(lang.trim().eq_ignore_ascii_case("mermaid")),
                Some(_) => None,
            };
        } else if fence == Some(false) && line.chars().any(|c| DRAWING.contains(c)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(rel: &str, text: &str) -> Vec<String> {
        check(rel, text).into_iter().map(|i| i.message).collect()
    }

    const GOOD: &str = "# F — Outcome\n\n## Summary\nFilter tasks by status. Decide: \
        archived counts or not.\n\n## Requirements\n\n- REQ-001: Tasks can be filtered by status.\n  \
        - statuses: open, done\n\n## Open questions\n";

    #[test]
    fn a_short_file_with_a_summary_has_nothing_to_say() {
        assert_eq!(messages("01-outcome.md", GOOD), Vec::<String>::new());
        // Files with no budget are left alone.
        assert!(check("changes/CHANGE-001.md", "long ".repeat(900).as_str()).is_empty());
    }

    #[test]
    fn a_stage_without_a_summary_or_with_a_long_one_is_told() {
        let m = messages("02-behavior.md", "# B\n\n## Situations\nx\n");
        assert!(m[0].starts_with("has no \"Summary\" section"), "{m:?}");
        // Tasks open with their Goal; no summary asked.
        assert!(messages("tasks/TASK-001.md", "# T\n\n## Goal\nx\n").is_empty());

        let long = format!("# B\n\n## Summary\n{}\n", "word ".repeat(151));
        let m = messages("02-behavior.md", &long);
        assert_eq!(m, ["\"Summary\" is 151 words; keep it to 150"]);
    }

    #[test]
    fn the_budget_counts_what_is_above_detail_without_code_blocks() {
        let filler = "word ".repeat(450);
        let over = format!("# T\n\n## Goal\n{filler}\n");
        let m = messages("tasks/TASK-001.md", &over);
        assert!(
            m[0].starts_with("452 words to read before deciding (budget 300)"),
            "{m:?}"
        );

        // The same text under Detail, or in a code block, is not counted.
        let detail = format!("# T\n\n## Goal\nshort\n\n## Detail\n{filler}\n");
        assert!(messages("tasks/TASK-001.md", &detail).is_empty());
        let fenced = format!("# T\n\n## Goal\nshort\n```\n{filler}\n```\n");
        assert!(messages("tasks/TASK-001.md", &fenced).is_empty());
        // Table rules and bullets are not words.
        assert_eq!(words(["| a | b |", "|---|---|", "- x"].into_iter()), 3);
    }

    #[test]
    fn a_long_requirement_is_told_to_split() {
        let long = format!(
            "# F\n\n## Summary\ns\n\n## Requirements\n\n- REQ-002: {}\n- **REQ-003**: short\n",
            "word ".repeat(30)
        );
        let m = messages("01-outcome.md", &long);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].starts_with("REQ-002 is 32 words"), "{m:?}");
    }

    #[test]
    fn a_diagram_drawn_in_text_is_told_to_use_mermaid() {
        let ascii = "# S\n\n## Summary\ns\n\n## Flow\n```\na\n │\n ▼\nb\n```\n";
        let m = messages("03-solution.md", ascii);
        assert!(m[0].starts_with("draws a diagram in text"), "{m:?}");
        let mermaid = "# S\n\n## Summary\ns\n\n## Flow\n```mermaid\ngraph TD\n a --> b\n```\n";
        assert!(messages("03-solution.md", mermaid).is_empty());
    }
}
