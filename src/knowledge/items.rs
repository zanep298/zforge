//! Items, IDs and evidence a knowledge file states (ONBOARD REQ-004,
//! TASK-002 shared interfaces).
//!
//! An item is a list item whose text starts with a stable ID (`DOM-`,
//! `CONV-` or `RULE-` and a number) followed by `:`; its evidence is the
//! last parenthesised, comma-separated list of `path:line` or
//! `path:start-end` citations trailing the item's text. A list item that
//! does not start this way, or has no such trailing citation list, is
//! reported separately ([`Unparsed`]) so `lint` can refuse it — but only
//! outside "Open questions", where a plain checklist item is expected.
//! A `## ` heading shown inside a fenced code block is an example, not a
//! section, matching `intake::lint::sections`.

use crate::intake::lint::{split_frontmatter, OPEN_QUESTIONS};
use regex::Regex;
use std::sync::OnceLock;

/// One `path:line` or `path:start-end` citation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Cite {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

/// One knowledge statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    /// The knowledge file this item came from; empty unless the caller
    /// stamps it (`items()` parses one file's text and does not know its
    /// own name).
    pub file: String,
    /// The `## ` section (module) the item is under.
    pub section: String,
    /// The item's text, with its trailing evidence list removed.
    pub text: String,
    pub evidence: Vec<Cite>,
    /// 1-based line the item starts on, for lint messages.
    pub line: usize,
}

/// A list item outside "Open questions" that did not parse as an [`Item`]:
/// no recognised ID, or an ID with no evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unparsed {
    pub section: String,
    pub text: String,
    pub line: usize,
    /// `Some(id)` when an ID was recognised but had no evidence list.
    pub id: Option<String>,
}

fn item_start_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^\s*[-*]\s+(?:\*\*)?((?:DOM|CONV|RULE)-\d+)(?:\*\*)?:\s*(.*)$")
            .expect("static regex")
    })
}

fn list_item_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*[-*]\s+").expect("static regex"))
}

/// Matches when the accumulated item text ends with a parenthesised,
/// comma-separated citation list — the item's *last* such list, since the
/// body is matched lazily up to it.
fn evidence_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?P<body>.*?)\s*\((?P<cites>[^()]+)\)$").expect("static regex"))
}

/// `path:line` or `path:start-end`, greedy on `path` so a colon in the
/// path itself does not split it from the line numbers.
fn cite_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(.+):(\d+)(?:-(\d+))?$").expect("static regex"))
}

/// `pinned:` from a knowledge file's frontmatter — the commit its items
/// were drafted from and are checked against.
pub fn pinned(text: &str) -> Option<String> {
    let (fm, _) = split_frontmatter(text);
    let v: serde_yaml::Value = serde_yaml::from_str(fm?).ok()?;
    v.get("pinned")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `covers:` from `domain.md`'s frontmatter — the modules it was drafted
/// to cover.
pub fn covers(text: &str) -> Vec<String> {
    let (fm, _) = split_frontmatter(text);
    let Some(fm) = fm else {
        return Vec::new();
    };
    let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(fm) else {
        return Vec::new();
    };
    match v.get("covers") {
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Parsed items and unparsed list items — everything [`parse`] found.
#[derive(Debug, Default)]
pub struct Parsed {
    pub items: Vec<Item>,
    pub unparsed: Vec<Unparsed>,
}

/// Every well-formed item in `text` (`Item::file` left empty — the caller
/// knows which file it read).
pub fn items(text: &str) -> Vec<Item> {
    parse(text).items
}

/// A line continues the previous list item: non-blank, not itself a list
/// item, not a heading, not a fence marker.
fn is_continuation(line: &str) -> bool {
    let t = line.trim_start();
    !line.trim().is_empty()
        && !list_item_re().is_match(line)
        && !t.starts_with("## ")
        && !t.starts_with("```")
}

/// Parse both items and the list items that failed to become one. Used by
/// `lint` to refuse a statement with no evidence (Output, AC-02).
pub fn parse(text: &str) -> Parsed {
    let (_, body) = split_frontmatter(text);
    let prefix_lines = text[..text.len() - body.len()].matches('\n').count();
    let lines: Vec<&str> = body.lines().collect();
    let mut parsed = Parsed::default();
    let mut section = String::new();
    let mut in_fence = false;
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            i += 1;
            continue;
        }
        if in_fence {
            i += 1;
            continue;
        }
        if let Some(title) = line.strip_prefix("## ") {
            section = title.trim().to_string();
            i += 1;
            continue;
        }
        let file_line = prefix_lines + i + 1;
        if OPEN_QUESTIONS.matches(&section) || line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(caps) = item_start_re().captures(line) {
            let id = caps[1].to_string();
            let mut acc = caps[2].trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && is_continuation(lines[j]) {
                if !acc.is_empty() {
                    acc.push(' ');
                }
                acc.push_str(lines[j].trim());
                j += 1;
            }
            match evidence_re().captures(&acc) {
                Some(ev) => {
                    let evidence = parse_cites(&ev["cites"]);
                    if evidence.is_empty() {
                        parsed.unparsed.push(Unparsed {
                            section: section.clone(),
                            text: acc,
                            line: file_line,
                            id: Some(id),
                        });
                    } else {
                        parsed.items.push(Item {
                            id,
                            file: String::new(),
                            section: section.clone(),
                            text: ev["body"].trim().to_string(),
                            evidence,
                            line: file_line,
                        });
                    }
                }
                None => parsed.unparsed.push(Unparsed {
                    section: section.clone(),
                    text: acc,
                    line: file_line,
                    id: Some(id),
                }),
            }
            i = j;
        } else if list_item_re().is_match(line) {
            let mut acc = line.trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && is_continuation(lines[j]) {
                acc.push(' ');
                acc.push_str(lines[j].trim());
                j += 1;
            }
            parsed.unparsed.push(Unparsed {
                section: section.clone(),
                text: acc,
                line: file_line,
                id: None,
            });
            i = j;
        } else {
            i += 1;
        }
    }
    parsed
}

fn parse_cites(list: &str) -> Vec<Cite> {
    list.split(',')
        .filter_map(|piece| {
            let caps = cite_re().captures(piece.trim())?;
            let start: u32 = caps[2].parse().ok()?;
            let end = caps
                .get(3)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(start);
            Some(Cite {
                path: caps[1].trim().to_string(),
                start,
                end,
            })
        })
        .collect()
}

/// `DOM-`, `CONV-` or `RULE-` — the prefix items in `rel` must use.
pub fn expected_prefix(rel: &str) -> Option<&'static str> {
    match rel {
        "domain.md" => Some("DOM"),
        "conventions.md" => Some("CONV"),
        "rules.md" => Some("RULE"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pinned_and_covers_from_frontmatter() {
        let text = "---\npinned: abc123\ncovers: [internal/token, internal/audit]\n---\n## x\n";
        assert_eq!(pinned(text).as_deref(), Some("abc123"));
        assert_eq!(covers(text), ["internal/token", "internal/audit"]);

        assert_eq!(pinned("# no frontmatter\n"), None);
        assert_eq!(covers("# no frontmatter\n"), Vec::<String>::new());
    }

    #[test]
    fn parses_a_single_line_item_with_two_cites() {
        let text =
            "## internal/token\n- DOM-004: reuse revokes the session. (a/b.rs:88, a/c.rs:141)\n";
        let its = items(text);
        assert_eq!(its.len(), 1);
        let it = &its[0];
        assert_eq!(it.id, "DOM-004");
        assert_eq!(it.section, "internal/token");
        assert_eq!(it.text, "reuse revokes the session.");
        assert_eq!(
            it.evidence,
            vec![
                Cite {
                    path: "a/b.rs".into(),
                    start: 88,
                    end: 88
                },
                Cite {
                    path: "a/c.rs".into(),
                    start: 141,
                    end: 141
                },
            ]
        );
    }

    #[test]
    fn joins_a_wrapped_item_across_lines() {
        let text = "## m\n- DOM-004: A refresh token is single-use; reuse revokes the\n  whole session family. (a/refresh.go:88-95)\n";
        let its = items(text);
        assert_eq!(its.len(), 1);
        assert_eq!(
            its[0].text,
            "A refresh token is single-use; reuse revokes the whole session family."
        );
        assert_eq!(its[0].evidence[0].start, 88);
        assert_eq!(its[0].evidence[0].end, 95);
    }

    #[test]
    fn a_list_item_without_evidence_is_unparsed_with_its_line() {
        let text = "## m\nintro\n- DOM-004: no evidence here\n";
        let p = parse(text);
        assert!(p.items.is_empty());
        assert_eq!(p.unparsed.len(), 1);
        assert_eq!(p.unparsed[0].id.as_deref(), Some("DOM-004"));
        assert_eq!(p.unparsed[0].line, 3);
    }

    #[test]
    fn a_list_item_without_a_recognised_id_is_unparsed() {
        let text = "## m\n- just a bullet, no id (a/b.rs:1)\n";
        let p = parse(text);
        assert!(p.items.is_empty());
        assert_eq!(p.unparsed.len(), 1);
        assert_eq!(p.unparsed[0].id, None);
    }

    #[test]
    fn open_questions_are_never_parsed_as_items_or_flagged() {
        let text =
            "## Open questions\n- [ ] is this a rule? (a/b.rs:1)\n- DOM-999: even this (a/b.rs:1)\n";
        let p = parse(text);
        assert!(p.items.is_empty());
        assert!(p.unparsed.is_empty());
    }

    #[test]
    fn an_item_inside_a_fenced_example_is_ignored() {
        let text =
            "## m\n```\n- DOM-001: example only (a/b.rs:1)\n```\n- DOM-002: real one (a/b.rs:2)\n";
        let its = items(text);
        assert_eq!(its.len(), 1);
        assert_eq!(its[0].id, "DOM-002");
    }

    #[test]
    fn wrong_prefixes_still_pick_up_a_recognised_kind() {
        // RULE- in domain.md: still an item structurally; the file-vs-prefix
        // check is lint's job (AC-04), not the parser's.
        let text = "## m\n- RULE-001: misplaced (a/b.rs:1)\n";
        let its = items(text);
        assert_eq!(its[0].id, "RULE-001");
    }

    #[test]
    fn expected_prefixes_match_the_three_files() {
        assert_eq!(expected_prefix("domain.md"), Some("DOM"));
        assert_eq!(expected_prefix("conventions.md"), Some("CONV"));
        assert_eq!(expected_prefix("rules.md"), Some("RULE"));
        assert_eq!(expected_prefix("other.md"), None);
    }
}
