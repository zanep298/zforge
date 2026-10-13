//! What a file rests on, item by item (D6 as amended 12/10/2026).
//!
//! A later stage or a task does not rest on the whole of an earlier stage,
//! only on some of its items:
//!
//! - `01-outcome.md`: each requirement — its `REQ-nnn` line with its
//!   sub-items;
//! - `02-behavior.md`: each row of the situations table, keyed by its `#`
//!   and tied to the requirements in its REQ column, and the business rules
//!   as one item tied to the requirements they name;
//! - `03-solution.md`: each binding decision `D-n`;
//! - `04-breakdown.md`: each task row, keyed by its `TASK-nnn`.
//!
//! The scope constraints — "Must not change" and "Out of scope" in
//! 01-outcome, "Existing behavior to preserve" in 02-behavior — bind
//! everything below them. Other text (summary, problem, flow, prose,
//! detail) binds nothing by itself.
//!
//! [`view`] is what one downstream file sees of one upstream stage: the
//! items it rests on, and the constraints. Two views of the same stage
//! that differ mean the downstream file rests on something that changed
//! ([`changed`]); the hash of the views is what a run's contract key
//! covers (`run::reuse`). A stage that cannot be read as items — written
//! before these conventions, or in another shape — is seen whole, which is
//! the rule D6 had before.

use super::lint::{self, Heading};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub const MUST_NOT_CHANGE: Heading = Heading::new("Must not change", "Phải giữ nguyên");
pub const OUT_OF_SCOPE: Heading = Heading::new("Out of scope", "Ngoài phạm vi");
pub const SITUATIONS: Heading = Heading::new("Situations", "Tình huống");
pub const BUSINESS_RULES: Heading = Heading::new("Business rules", "Quy tắc nghiệp vụ");
pub const PRESERVE: Heading =
    Heading::new("Existing behavior to preserve", "Hành vi cũ cần bảo toàn");

/// The key of a stage seen whole, when it cannot be read as items.
pub const WHOLE: &str = "(whole file)";
/// The key of the list of requirement IDs a later stage must cover.
pub const REQUIREMENT_LIST: &str = "(requirement list)";

/// One item of a stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its exact text (trimmed lines).
    pub text: String,
    /// The requirements it belongs to.
    pub reqs: BTreeSet<String>,
}

/// A stage read as items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// Item key (`REQ-001`, `situation 3`, `business rules`, `D-1`,
    /// `TASK-001`) → item.
    pub items: BTreeMap<String, Item>,
    /// Scope constraints: section title → its text.
    pub constraints: BTreeMap<String, String>,
}

/// What rests on a stage.
#[derive(Debug, Clone)]
pub enum Scope {
    /// A later stage: it rests on the items it names, by their ID or by a
    /// requirement they belong to, on every item tied to no requirement,
    /// and on the list of requirements (it must cover a new one).
    Stage { mentions: BTreeSet<String> },
    /// A task: its requirements, the 02 rows and rules of those
    /// requirements (and those tied to none), the decisions it cites, and
    /// its own row of the breakdown.
    Task {
        id: String,
        reqs: BTreeSet<String>,
        decisions: BTreeSet<String>,
    },
}

impl Scope {
    /// The scope of a later stage, from its text.
    pub fn stage(text: &str) -> Self {
        let clean = lint::strip_comments(lint::split_frontmatter(text).1);
        let mut mentions = lint::requirement_refs(&clean);
        mentions.extend(decision_refs(&clean));
        mentions.extend(lint::task_refs(&clean));
        Scope::Stage { mentions }
    }

    /// The scope of task `id`, from its text: the requirements its
    /// frontmatter lists or its body names, and the decisions it cites.
    pub fn task(id: &str, text: &str) -> Self {
        let (_, body) = lint::split_frontmatter(text);
        let clean = lint::strip_comments(body);
        let mut reqs = lint::requirement_refs(&clean);
        if let Ok(meta) = lint::parse_task_meta(text) {
            reqs.extend(meta.requirements);
        }
        Scope::Task {
            id: id.to_string(),
            reqs,
            decisions: decision_refs(&clean),
        }
    }
}

fn re(pattern: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static regex"))
}

fn decision_id() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"\bD-\d+\b", &R)
}

/// Decision IDs (`D-1`) named in `text`.
pub fn decision_refs(text: &str) -> BTreeSet<String> {
    decision_id()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn req_start() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"^(#{1,6}\s+|[-*]\s+)(?:\*\*)?(REQ-\d{3,})\b", &R)
}

/// Read `stage` (a file name of [`super::STAGES`]) as items; `None` when its
/// text does not have them in the expected shape.
pub fn parse(stage: &str, text: &str) -> Option<Parsed> {
    let (_, body) = lint::split_frontmatter(text);
    let clean = lint::strip_comments(body);
    let secs = lint::sections(&clean);
    let mut p = Parsed::default();
    let constraint = |p: &mut Parsed, h: Heading| {
        if let Some(lines) = lint::section(&secs, h) {
            let t = joined(lines);
            if !t.is_empty() {
                p.constraints.insert(h.name.to_string(), t);
            }
        }
    };
    match stage {
        "01-outcome.md" => {
            p.items = requirements(&clean);
            constraint(&mut p, MUST_NOT_CHANGE);
            constraint(&mut p, OUT_OF_SCOPE);
        }
        "02-behavior.md" => {
            let rows = table_rows(lint::section(&secs, SITUATIONS)?);
            for row in rows {
                let key = row
                    .cell("#")
                    .or_else(|| row.cells.first().map(String::as_str))
                    .unwrap_or("")
                    .trim_matches(|c: char| c == '*' || c.is_whitespace())
                    .to_string();
                if key.is_empty() {
                    continue;
                }
                let reqs = lint::requirement_refs(row.cell("REQ").unwrap_or(&row.line));
                p.items.insert(
                    format!("situation {key}"),
                    Item {
                        text: row.line.clone(),
                        reqs,
                    },
                );
            }
            if let Some(lines) = lint::section(&secs, BUSINESS_RULES) {
                let text = joined(lines);
                if !text.is_empty() {
                    p.items.insert(
                        "business rules".into(),
                        Item {
                            reqs: lint::requirement_refs(&text),
                            text,
                        },
                    );
                }
            }
            if !p.items.keys().any(|k| k.starts_with("situation ")) {
                return None;
            }
            constraint(&mut p, PRESERVE);
        }
        "03-solution.md" => {
            let lines = lint::section(&secs, lint::BINDING_DECISIONS)?;
            for line in lines {
                let t = line.trim();
                let lead = t
                    .trim_start_matches(['|', '-', '*', ' '])
                    .trim_start_matches("**");
                let Some(m) = decision_id().find(lead).filter(|m| m.start() == 0) else {
                    continue;
                };
                p.items.insert(
                    m.as_str().to_string(),
                    Item {
                        text: t.to_string(),
                        reqs: lint::requirement_refs(t),
                    },
                );
            }
            // A section in another shape ("Status is an enum." as prose)
            // cannot be read as decisions; "None." can.
            let words = joined(lines).split_whitespace().count();
            if p.items.is_empty() && words > 4 {
                return None;
            }
        }
        "04-breakdown.md" => {
            for row in table_rows(&clean.lines().map(String::from).collect::<Vec<_>>()) {
                let first = row.cells.first().map(String::as_str).unwrap_or("");
                let first = first.trim().trim_start_matches("**");
                if let Some(m) = lint::task_refs(first).into_iter().next() {
                    if first.starts_with(&m) {
                        p.items.insert(
                            m,
                            Item {
                                reqs: lint::requirement_refs(&row.line),
                                text: row.line.clone(),
                            },
                        );
                    }
                }
            }
        }
        _ => return None,
    }
    // A solution may bind nothing; the other stages always have items.
    (!p.items.is_empty() || stage == "03-solution.md").then_some(p)
}

/// Requirements with their sub-items: the `- REQ-nnn` line and the
/// indented lines under it; for a `### REQ-nnn` heading, everything up to
/// the next heading.
fn requirements(clean: &str) -> BTreeMap<String, Item> {
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    let mut current: Option<(String, bool, Vec<String>)> = None;
    let mut in_fence = false;
    let flush = |items: &mut BTreeMap<String, Item>, cur: Option<(String, bool, Vec<String>)>| {
        if let Some((id, _, lines)) = cur {
            let text = joined(&lines);
            let entry = items.entry(id.clone()).or_insert_with(|| Item {
                text: String::new(),
                reqs: BTreeSet::from([id]),
            });
            if !entry.text.is_empty() {
                entry.text.push('\n');
            }
            entry.text.push_str(&text);
        }
    };
    for line in clean.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence {
            if let Some(c) = req_start().captures(line) {
                flush(&mut items, current.take());
                let heading = c[1].starts_with('#');
                current = Some((c[2].to_string(), heading, vec![line.to_string()]));
                continue;
            }
        }
        let Some((_, heading, lines)) = current.as_mut() else {
            continue;
        };
        let ends = if *heading {
            !in_fence && line.starts_with('#')
        } else {
            !in_fence && !line.trim().is_empty() && !line.starts_with([' ', '\t'])
        };
        if ends {
            flush(&mut items, current.take());
        } else {
            lines.push(line.to_string());
        }
    }
    flush(&mut items, current.take());
    items
}

/// Non-empty lines, trimmed at the end, joined.
fn joined(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

struct Row {
    line: String,
    cells: Vec<String>,
    header: Vec<String>,
}

impl Row {
    fn cell(&self, name: &str) -> Option<&str> {
        let i = self.header.iter().position(|h| {
            let h = h.trim().trim_matches('*').to_lowercase();
            h == name.to_lowercase() || (name == "REQ" && h.starts_with("req"))
        })?;
        self.cells.get(i).map(String::as_str)
    }
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn is_separator(line: &str) -> bool {
    let c = cells(line);
    !c.is_empty()
        && c.iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

/// The body rows of every Markdown table in `lines`, each with its header.
fn table_rows(lines: &[String]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut header: Option<Vec<String>> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if !line.starts_with('|') {
            header = None;
            i += 1;
            continue;
        }
        if header.is_none() {
            if lines.get(i + 1).is_some_and(|n| is_separator(n)) {
                header = Some(cells(line));
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        rows.push(Row {
            line: line.to_string(),
            cells: cells(line),
            header: header.clone().unwrap_or_default(),
        });
        i += 1;
    }
    rows
}

/// What `scope` sees of `stage` with text `text`: item key → text. A stage
/// that cannot be read as items is seen whole ([`WHOLE`]).
pub fn view(stage: &str, text: &str, scope: &Scope) -> BTreeMap<String, String> {
    let Some(p) = parse(stage, text) else {
        return BTreeMap::from([(WHOLE.to_string(), text.to_string())]);
    };
    let mut out: BTreeMap<String, String> = p
        .constraints
        .iter()
        .map(|(t, x)| (format!("\"{t}\""), x.clone()))
        .collect();
    let absent = || "(absent)".to_string();
    match scope {
        Scope::Stage { mentions } => {
            for (k, item) in &p.items {
                let named = mentions.contains(k)
                    || item.reqs.iter().any(|r| mentions.contains(r))
                    || item.reqs.is_empty();
                if named {
                    out.insert(k.clone(), item.text.clone());
                }
            }
            if stage == lint::OUTCOME {
                let ids: Vec<&str> = p.items.keys().map(String::as_str).collect();
                out.insert(REQUIREMENT_LIST.into(), ids.join(" "));
            }
        }
        Scope::Task {
            id,
            reqs,
            decisions,
        } => match stage {
            "01-outcome.md" => {
                for r in reqs {
                    let t = p.items.get(r).map(|i| i.text.clone());
                    out.insert(r.clone(), t.unwrap_or_else(absent));
                }
            }
            "02-behavior.md" => {
                for (k, item) in &p.items {
                    if item.reqs.is_empty() || item.reqs.iter().any(|r| reqs.contains(r)) {
                        out.insert(k.clone(), item.text.clone());
                    }
                }
            }
            "03-solution.md" => {
                for d in decisions {
                    let t = p.items.get(d).map(|i| i.text.clone());
                    out.insert(d.clone(), t.unwrap_or_else(absent));
                }
            }
            _ => {
                let t = p.items.get(id).map(|i| i.text.clone());
                out.insert(id.clone(), t.unwrap_or_else(absent));
            }
        },
    }
    out
}

/// Keys whose text differs between two views (added and removed too).
pub fn changed(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> Vec<String> {
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    keys.into_iter()
        .filter(|k| old.get(*k) != new.get(*k))
        .cloned()
        .collect()
}

/// A view as one text, for hashing.
pub fn digest(view: &BTreeMap<String, String>) -> String {
    view.iter()
        .map(|(k, v)| format!("[{k}]\n{v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Items named for a person: `REQ-005`, `situation 3`, `"Out of scope"`,
/// or what a whole-file key means.
pub fn describe(keys: &[String]) -> String {
    keys.iter()
        .map(|k| match k.as_str() {
            WHOLE => "the file (not readable as items)".to_string(),
            REQUIREMENT_LIST => "the list of requirements (one added or removed)".to_string(),
            _ => k.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const OUTCOME: &str = "# F — Outcome\n\n## Summary\nTwo things.\n\n## Requirements\n\n\
        - REQ-001: add returns the sum\n  - negative numbers too\n\
        - REQ-002: sub returns the difference\n\n\
        ## Must not change\n\n- the CLI flags\n\n## Out of scope\n\n- floats\n\n## Open questions\n";

    #[test]
    fn reads_requirements_with_their_sub_items_and_the_constraints() {
        let p = parse("01-outcome.md", OUTCOME).unwrap();
        assert_eq!(
            p.items["REQ-001"].text,
            "- REQ-001: add returns the sum\n  - negative numbers too"
        );
        assert_eq!(
            p.items["REQ-002"].text,
            "- REQ-002: sub returns the difference"
        );
        assert_eq!(p.constraints["Out of scope"], "- floats");
        assert!(parse("01-outcome.md", "# F\n\nNo requirement.\n").is_none());
    }

    #[test]
    fn reads_situation_rows_decisions_and_task_rows() {
        let behavior = "## Situations\n\n| # | Kind | When | Then | REQ |\n|---|---|---|---|---|\n\
            | 1 | normal | add 2 3 | 5 | REQ-001 |\n| 2 | normal | sub 3 2 | 1 | REQ-002 |\n\n\
            ## Business rules\n\n- sums wrap (REQ-001)\n";
        let p = parse("02-behavior.md", behavior).unwrap();
        assert_eq!(
            p.items["situation 2"].reqs,
            BTreeSet::from(["REQ-002".into()])
        );
        assert_eq!(
            p.items["business rules"].reqs,
            BTreeSet::from(["REQ-001".into()])
        );
        assert!(parse("02-behavior.md", "## Situations\nREQ-001: it adds.\n").is_none());

        let solution =
            "## Binding decisions\n\n| # | Decision | Why | Rejected |\n|---|---|---|---|\n\
            | D-1 | shell arithmetic | REQ-001 | bc |\n| **D-2** | one file | simple | two |\n";
        let p = parse("03-solution.md", solution).unwrap();
        assert_eq!(p.items.keys().collect::<Vec<_>>(), ["D-1", "D-2"]);
        assert!(parse("03-solution.md", "## Binding decisions\n\nNone.\n").is_some());
        assert!(parse("03-solution.md", "## Flow\nDoes it.\n").is_none());
        assert!(parse(
            "03-solution.md",
            "## Binding decisions\nStatus is an enum, not text.\n"
        )
        .is_none());

        let breakdown = "## Tasks and dependencies\n\n| Task | Serves | Depends on | Output |\n\
            |---|---|---|---|\n| TASK-001 Add | REQ-001 | — | add |\n| TASK-002 Sub | REQ-002 | TASK-001 | sub |\n";
        let p = parse("04-breakdown.md", breakdown).unwrap();
        assert_eq!(p.items.keys().collect::<Vec<_>>(), ["TASK-001", "TASK-002"]);
    }

    #[test]
    fn a_task_sees_only_its_requirements_and_a_stage_what_it_names() {
        let task = "---\nid: TASK-001\nparent: F\nrequirements: [REQ-001]\ndepends_on: []\n---\n# T\nKeeps D-1.\n";
        let scope = Scope::task("TASK-001", task);
        let changed_req2 = OUTCOME.replace("difference", "delta");
        let before = view("01-outcome.md", OUTCOME, &scope);
        assert_eq!(before, view("01-outcome.md", &changed_req2, &scope));
        let changed_req1 = OUTCOME.replace("negative numbers too", "and zero");
        assert_eq!(
            changed(&before, &view("01-outcome.md", &changed_req1, &scope)),
            ["REQ-001"]
        );
        // Prose binds nothing; a scope constraint binds everything.
        let prose = OUTCOME.replace("Two things.", "Two small things.");
        assert_eq!(before, view("01-outcome.md", &prose, &scope));
        let scope_change = OUTCOME.replace("- floats", "- floats\n- big numbers");
        assert_eq!(
            changed(&before, &view("01-outcome.md", &scope_change, &scope)),
            ["\"Out of scope\""]
        );

        // A stage naming only REQ-001 does not rest on REQ-002's text, but
        // does on the list of requirements.
        let stage = Scope::stage("| 1 | add | REQ-001 |");
        let before = view("01-outcome.md", OUTCOME, &stage);
        assert_eq!(before, view("01-outcome.md", &changed_req2, &stage));
        let added = OUTCOME.replace(
            "- REQ-002: sub",
            "- REQ-003: mul returns the product\n- REQ-002: sub",
        );
        assert_eq!(
            changed(&before, &view("01-outcome.md", &added, &stage)),
            [REQUIREMENT_LIST]
        );
        // Unreadable: seen whole.
        let v = view("02-behavior.md", "## Situations\nprose\n", &stage);
        assert_eq!(v.keys().collect::<Vec<_>>(), [WHOLE]);
    }
}
