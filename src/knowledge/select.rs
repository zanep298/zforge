//! What a task's agents get from the project's accepted knowledge (ONBOARD
//! REQ-008, TASK-007 shared interfaces): every `rules.md` and
//! `conventions.md` item, then the `domain.md` items the task's contract
//! and its accepted stages cite by ID or by the module of a path they name
//! — within a byte budget. What does not fit is named by ID, with the
//! absolute path of its accepted snapshot so the agent can still read the
//! rest (03-solution "Choosing what a task gets").
//!
//! Selection reads only the pinned snapshots a run's contract already
//! verified (`run::contract::Handover`); it never touches the working
//! files, so it can never change what a run was given after the fact.

use super::items::{self, Item};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One pinned knowledge file, as a run's contract loads it.
#[derive(Debug, Clone, Copy)]
pub struct PinnedFile<'a> {
    pub file: &'a str,
    pub revision: u32,
    pub text: &'a str,
    /// Absolute path of the accepted snapshot, named when an item of this
    /// file does not fit the limit (AC-03).
    pub snapshot_path: &'a Path,
}

/// What a task's prompt gets from the project's knowledge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Rendered for the prompt; empty when nothing was pinned or selected.
    pub text: String,
    /// IDs of the items actually included in `text`.
    pub items: Vec<String>,
    /// IDs that did not fit the byte limit.
    pub omitted: Vec<String>,
    /// Absolute snapshot paths of the files an omitted item came from.
    pub snapshots: Vec<PathBuf>,
}

fn cited_ids_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b(?:DOM|CONV|RULE)-\d+\b").expect("static regex"))
}

/// Backticked spans that look like a path: contain `/` and no whitespace.
fn backticked_paths(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else {
            break;
        };
        let span = &after[..end];
        if span.contains('/') && !span.contains(char::is_whitespace) {
            out.push(span);
        }
        rest = &after[end + 1..];
    }
    out
}

/// Whether `module` (a `domain.md` section, e.g. `internal/token`) is the
/// directory of one of `paths` — equal to it, or a `/`-bounded prefix.
fn module_matches(module: &str, paths: &[&str]) -> bool {
    let module = module.trim_end_matches('/');
    if module.is_empty() {
        return false;
    }
    paths
        .iter()
        .any(|p| *p == module || p.starts_with(&format!("{module}/")))
}

/// `pin`'s items, each stamped with its file name.
fn file_items(pin: &PinnedFile) -> Vec<Item> {
    items::items(pin.text)
        .into_iter()
        .map(|mut it| {
            it.file = pin.file.to_string();
            it
        })
        .collect()
}

/// One item as a prompt line: `- ID: text (evidence)`.
fn render_item(it: &Item) -> String {
    let evidence = it
        .evidence
        .iter()
        .map(|c| {
            if c.start == c.end {
                format!("{}:{}", c.path, c.start)
            } else {
                format!("{}:{}-{}", c.path, c.start, c.end)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("- {}: {} ({})", it.id, it.text, evidence)
}

/// What `task` gets from `files` — the knowledge snapshots a handover
/// pinned — read for `search_text`, the task's contract and its accepted
/// stages concatenated: item IDs it cites, and the backticked paths it
/// names.
pub fn for_task(files: &[PinnedFile], search_text: &str, limit: usize) -> Selection {
    if files.is_empty() {
        return Selection::default();
    }
    let cited_ids: BTreeSet<String> = cited_ids_re()
        .find_iter(search_text)
        .map(|m| m.as_str().to_string())
        .collect();
    let cited_paths = backticked_paths(search_text);

    let by_name = |name: &str| files.iter().find(|f| f.file == name);
    let mut ordered: Vec<(PinnedFile, Item)> = Vec::new();

    // 1. every rules.md item, then every conventions.md item.
    for name in ["rules.md", "conventions.md"] {
        if let Some(pin) = by_name(name) {
            ordered.extend(file_items(pin).into_iter().map(|it| (*pin, it)));
        }
    }
    // 2 & 3. domain.md items the contract cites by ID, then the sections
    // whose module a cited path names — domain's own order, no repeats.
    if let Some(pin) = by_name("domain.md") {
        let domain_items = file_items(pin);
        let mut seen = BTreeSet::new();
        for it in &domain_items {
            if cited_ids.contains(&it.id) {
                seen.insert(it.id.clone());
                ordered.push((*pin, it.clone()));
            }
        }
        for it in &domain_items {
            if !seen.contains(&it.id) && module_matches(&it.section, &cited_paths) {
                seen.insert(it.id.clone());
                ordered.push((*pin, it.clone()));
            }
        }
    }

    build(&ordered, files, limit)
}

fn build(ordered: &[(PinnedFile, Item)], files: &[PinnedFile], limit: usize) -> Selection {
    let mut text = String::new();
    let mut items = Vec::new();
    let mut omitted = Vec::new();
    let mut omitted_files: BTreeSet<String> = BTreeSet::new();
    let mut current_file: Option<String> = None;
    let mut current_section: Option<String> = None;
    let mut budget = limit;

    for (pin, it) in ordered {
        let mut block = String::new();
        let file_changed = current_file.as_deref() != Some(pin.file);
        if file_changed {
            if current_file.is_some() {
                block.push('\n');
            }
            block.push_str(&format!("### {} (revision {})\n", pin.file, pin.revision));
        }
        if pin.file == "domain.md"
            && !it.section.is_empty()
            && (file_changed || current_section.as_deref() != Some(it.section.as_str()))
        {
            block.push_str(&format!("## {}\n", it.section));
        }
        block.push_str(&render_item(it));
        block.push('\n');

        if block.len() <= budget {
            budget -= block.len();
            text.push_str(&block);
            items.push(it.id.clone());
            current_file = Some(pin.file.to_string());
            if pin.file == "domain.md" {
                current_section = Some(it.section.clone());
            }
        } else {
            omitted.push(it.id.clone());
            omitted_files.insert(pin.file.to_string());
        }
    }

    let snapshots: Vec<PathBuf> = files
        .iter()
        .filter(|f| omitted_files.contains(f.file))
        .map(|f| f.snapshot_path.to_path_buf())
        .collect();

    if !omitted.is_empty() {
        text.push_str(&format!(
            "\nNot shown — over the knowledge prompt limit: {}. Read the rest at:\n{}\n",
            omitted.join(", "),
            snapshots
                .iter()
                .map(|p| format!("- {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }

    Selection {
        text,
        items,
        omitted,
        snapshots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> String {
        "## general\n- RULE-001: never log secrets. (a/log.rs:10)\n\
         - RULE-002: version the API. (a/api.rs:5-8)\n"
            .into()
    }

    fn conventions() -> String {
        "## general\n- CONV-001: errors wrap with %w. (a/err.rs:20)\n".into()
    }

    fn domain() -> String {
        "## internal/token\n\
         - DOM-001: a refresh token is single-use. (internal/token/refresh.rs:88)\n\
         ## internal/audit\n\
         - DOM-002: every export is logged. (internal/audit/writer.rs:12)\n"
            .into()
    }

    fn pins<'a>(
        rules: &'a str,
        conventions: &'a str,
        domain: &'a str,
        snaps: &'a [PathBuf; 3],
    ) -> Vec<PinnedFile<'a>> {
        vec![
            PinnedFile {
                file: "rules.md",
                revision: 3,
                text: rules,
                snapshot_path: &snaps[0],
            },
            PinnedFile {
                file: "conventions.md",
                revision: 2,
                text: conventions,
                snapshot_path: &snaps[1],
            },
            PinnedFile {
                file: "domain.md",
                revision: 5,
                text: domain,
                snapshot_path: &snaps[2],
            },
        ]
    }

    fn test_snaps() -> [PathBuf; 3] {
        [
            PathBuf::from("/store/rules.md"),
            PathBuf::from("/store/conventions.md"),
            PathBuf::from("/store/domain.md"),
        ]
    }

    /// AC-01: every rules and conventions item is in; a domain item the
    /// contract cites by ID is in; one that is neither cited nor in a
    /// named module is not.
    #[test]
    fn every_rules_and_conventions_item_and_cited_domain_items_are_included() {
        let (r, c, d) = (rules(), conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let search = "Constraints\n\nKnowledge: DOM-001\n";
        let s = for_task(&files, search, 10_000);
        assert_eq!(s.items, ["RULE-001", "RULE-002", "CONV-001", "DOM-001"]);
        assert!(s.omitted.is_empty());
        assert!(s.text.contains("RULE-001") && s.text.contains("CONV-001"));
        assert!(s.text.contains("DOM-001"));
        assert!(
            !s.text.contains("DOM-002"),
            "not cited, not named: {}",
            s.text
        );
    }

    /// AC-02: a domain section whose module appears in a backticked path
    /// the contract names is included in full; a section that is neither
    /// cited nor named is not.
    #[test]
    fn a_domain_section_named_by_path_is_included_whole() {
        let (r, c, d) = (rules(), conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let search = "See `internal/audit/writer.rs` for the export path.";
        let s = for_task(&files, search, 10_000);
        assert!(s.items.contains(&"DOM-002".to_string()));
        assert!(!s.items.contains(&"DOM-001".to_string()));
    }

    /// AC-03: over the limit, the omitted IDs and the snapshot path are
    /// named.
    #[test]
    fn over_the_limit_names_what_did_not_fit_and_its_snapshot() {
        let (r, c, d) = (rules(), conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        // Room for RULE-001's block (header + line) but not RULE-002's.
        let header_and_first = format!(
            "### rules.md (revision 3)\n{}\n",
            render_item(&{
                let mut it = items::items(&r).remove(0);
                it.file = "rules.md".into();
                it
            })
        );
        let limit = header_and_first.len();
        let s = for_task(&files, "", limit);
        assert_eq!(s.items, ["RULE-001"]);
        assert!(s.omitted.contains(&"RULE-002".to_string()));
        // conventions.md/domain.md items never fit either at this limit.
        assert!(s.omitted.contains(&"CONV-001".to_string()));
        // domain.md is never cited by the empty search text, so nothing of
        // it is even considered — only rules.md and conventions.md, which
        // could not fit, are named.
        assert_eq!(s.snapshots, [snaps[0].clone(), snaps[1].clone()]);
        assert!(s.text.contains("Not shown"), "{}", s.text);
        assert!(s.text.contains("RULE-002"), "{}", s.text);
    }

    /// AC-05 (selection level): no pinned files select nothing.
    #[test]
    fn no_files_selects_nothing() {
        let s = for_task(&[], "Knowledge: DOM-001", 10_000);
        assert_eq!(s, Selection::default());
        assert!(s.text.is_empty());
    }

    #[test]
    fn module_matching_requires_a_directory_boundary() {
        // `internal/audit2/x.rs` must not match module `internal/audit`.
        assert!(!module_matches("internal/audit", &["internal/audit2/x.rs"]));
        assert!(module_matches("internal/audit", &["internal/audit/x.rs"]));
        assert!(module_matches("internal/audit", &["internal/audit"]));
    }
}
