//! What a task's agents get from the project's accepted knowledge (ONBOARD
//! REQ-008, TASK-007 shared interfaces), within a byte budget
//! (`knowledge.prompt_limit`). Items are taken by relevance to the task —
//! its contract and accepted stages concatenated — in tiers:
//!
//! 1. items the task cites by ID (`CONV-nn`, `RULE-nn`, `DOM-nn`);
//! 2. `conventions.md`, then `rules.md` items *related* to a path the task
//!    names: a path in the item's text (placeholders such as `<name>`
//!    match any segment) or one of its `path:line` citations lies in the
//!    same directory, a containing one, or a sibling;
//! 3. *general* conventions, then general rules — those not under a
//!    module section (a `## ` heading that is a path);
//! 4. `domain.md` items whose module section the task names;
//! 5. the remaining (module-specific, unrelated) conventions and rules.
//!
//! Conventions come before rules in every tier: how the code is laid out
//! and written matters most to the code agent. The prompt shows what was
//! taken grouped by file — conventions, rules, domain — each in its own
//! order. What does not fit is indexed one line per item (ID and the start
//! of its text) as far as [`OMITTED_INDEX_BUDGET`] allows, followed by the
//! absolute paths of the accepted snapshots and an instruction to read the
//! conventions that apply before writing code (03-solution "Choosing what
//! a task gets").
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

/// Extra bytes, beyond the limit, for the one-line index of what did not
/// fit — the agent still learns what exists and where to read it.
pub const OMITTED_INDEX_BUDGET: usize = 4_096;

/// How much of an omitted item's text its index line shows.
const OMITTED_TEXT_CHARS: usize = 80;

/// The files every task may draw on in full, in prompt order: conventions
/// before rules.
const ALWAYS_FILES: [&str; 2] = ["conventions.md", "rules.md"];

/// Whether a `## ` section names a module (a path), as `domain.md`'s do.
fn is_module_section(section: &str) -> bool {
    section.contains('/')
}

/// Path-like tokens in `text`: a run without whitespace, backticks,
/// quotes, brackets or commas that contains `/`, with a trailing
/// `:line`/`:start-end` and punctuation removed. URLs are not paths.
fn path_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '`' | '(' | ')' | '[' | ']' | ',' | ';' | '"' | '\'' | '|'
            )
    }) {
        if !raw.contains('/') || raw.contains("://") {
            continue;
        }
        let mut t = raw.trim_end_matches(['.', ':', '!', '?']);
        if let Some((path, rest)) = t.rsplit_once(':') {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '-') {
                t = path;
            }
        }
        let t = t.trim_start_matches("./").trim_matches('*');
        if t.is_empty() || t.starts_with('/') {
            continue;
        }
        out.push(t.to_string());
    }
    out
}

/// The directory a path names, as segments: a path whose last segment
/// has an extension is a file, so its parent; otherwise the path itself.
fn dir_segments(path: &str) -> Vec<&str> {
    let mut segs: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    if !path.ends_with('/') && segs.last().is_some_and(|l| l.contains('.')) {
        segs.pop();
    }
    segs
}

/// A placeholder segment (`<name>`, `{module}`, `*`) matches any segment.
fn seg_eq(a: &str, b: &str) -> bool {
    let wild = |s: &str| {
        s == "*"
            || (s.starts_with('<') && s.ends_with('>'))
            || (s.starts_with('{') && s.ends_with('}'))
    };
    a == b || wild(a) || wild(b)
}

fn prefix_of(short: &[&str], long: &[&str]) -> bool {
    short.len() <= long.len() && short.iter().zip(long).all(|(a, b)| seg_eq(a, b))
}

/// Whether two directories are the same module or close to it: equal, one
/// containing the other (the container at least two levels deep, so a
/// top-level `internal/` relates to nothing), or siblings under a parent
/// at least two levels deep (`…/feature/handler` and `…/feature/service`).
fn dirs_related(a: &[&str], b: &[&str]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if prefix_of(short, long) && (short.len() == long.len() || short.len() >= 2) {
        return true;
    }
    a.len() == b.len() && a.len() >= 3 && prefix_of(&a[..a.len() - 1], &b[..b.len() - 1])
}

/// Whether `it` names — in its text, its citations or its module section —
/// a directory related to one the task names (`task_dirs`).
fn related(it: &Item, task_dirs: &[Vec<&str>]) -> bool {
    if task_dirs.is_empty() {
        return false;
    }
    let text_paths = path_tokens(&it.text);
    let mut dirs: Vec<Vec<&str>> = text_paths.iter().map(|p| dir_segments(p)).collect();
    dirs.extend(it.evidence.iter().map(|c| dir_segments(&c.path)));
    if is_module_section(&it.section) {
        dirs.push(dir_segments(&it.section));
    }
    dirs.iter()
        .any(|d| task_dirs.iter().any(|t| dirs_related(d, t)))
}

/// What `task` gets from `files` — the knowledge snapshots a handover
/// pinned — read for `search_text`, the task's contract and its accepted
/// stages concatenated: item IDs it cites, and the paths it names.
pub fn for_task(files: &[PinnedFile], search_text: &str, limit: usize) -> Selection {
    if files.is_empty() {
        return Selection::default();
    }
    let cited_ids: BTreeSet<String> = cited_ids_re()
        .find_iter(search_text)
        .map(|m| m.as_str().to_string())
        .collect();
    let mut task_paths: Vec<String> = path_tokens(search_text);
    task_paths.extend(
        backticked_paths(search_text)
            .into_iter()
            .map(str::to_string),
    );
    task_paths.sort();
    task_paths.dedup();
    let task_path_refs: Vec<&str> = task_paths.iter().map(String::as_str).collect();
    let task_dirs: Vec<Vec<&str>> = task_paths
        .iter()
        .map(|p| dir_segments(p))
        .filter(|d| !d.is_empty())
        .collect();

    let by_name = |name: &str| files.iter().find(|f| f.file == name);
    // Every candidate, in prompt order (file, then the file's own order).
    let mut candidates: Vec<(PinnedFile, Item)> = Vec::new();
    for name in ALWAYS_FILES {
        if let Some(pin) = by_name(name) {
            candidates.extend(file_items(pin).into_iter().map(|it| (*pin, it)));
        }
    }
    if let Some(pin) = by_name("domain.md") {
        // Only what the task cites by ID or whose module it names.
        candidates.extend(
            file_items(pin)
                .into_iter()
                .filter(|it| {
                    cited_ids.contains(&it.id) || module_matches(&it.section, &task_path_refs)
                })
                .map(|it| (*pin, it)),
        );
    }

    let file_rank = |f: &str| match f {
        "conventions.md" => 0,
        "rules.md" => 1,
        _ => 2,
    };
    let tier = |pin: &PinnedFile, it: &Item| -> usize {
        let f = file_rank(pin.file);
        if cited_ids.contains(&it.id) {
            0
        } else if f == 2 {
            5
        } else if related(it, &task_dirs) {
            1 + f
        } else if !is_module_section(&it.section) {
            3 + f
        } else {
            6 + f
        }
    };
    let mut priority: Vec<(usize, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(i, (pin, it))| (tier(pin, it), i))
        .collect();
    // Within a tier, prompt order: conventions before rules, file order.
    priority.sort();
    let priority: Vec<usize> = priority.into_iter().map(|(_, i)| i).collect();

    build(&candidates, &priority, files, limit)
}

/// One file's header in the prompt.
fn file_header(pin: &PinnedFile) -> String {
    format!("### {} (revision {})\n", pin.file, pin.revision)
}

fn section_header(section: &str) -> String {
    format!("## {section}\n")
}

/// Whether `pin`'s items are shown under their `## ` sections.
fn shows_sections(pin: &PinnedFile) -> bool {
    pin.file == "domain.md"
}

/// Take `candidates` in `priority` order while they fit `limit` — a
/// file's header, and a domain section's, counted the first time
/// something under it is taken — then render what was taken in prompt
/// order, grouped by file.
fn build(
    candidates: &[(PinnedFile, Item)],
    priority: &[usize],
    files: &[PinnedFile],
    limit: usize,
) -> Selection {
    let mut taken = vec![false; candidates.len()];
    let mut files_open: BTreeSet<&str> = BTreeSet::new();
    let mut sections_open: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut budget = limit;
    let mut omitted_order = Vec::new();

    for &i in priority {
        let (pin, it) = &candidates[i];
        let mut cost = render_item(it).len() + 1;
        let new_file = !files_open.contains(pin.file);
        if new_file {
            // The blank line before a file header is counted for every
            // file, so the rendered text never exceeds the limit.
            cost += file_header(pin).len() + 1;
        }
        let new_section = shows_sections(pin)
            && !it.section.is_empty()
            && !sections_open.contains(&(pin.file, it.section.as_str()));
        if new_section {
            cost += section_header(&it.section).len();
        }
        if cost <= budget {
            budget -= cost;
            taken[i] = true;
            files_open.insert(pin.file);
            if new_section {
                sections_open.insert((pin.file, it.section.as_str()));
            }
        } else {
            omitted_order.push(i);
        }
    }

    let mut text = String::new();
    let mut items = Vec::new();
    let mut current_file: Option<&str> = None;
    let mut current_section: Option<&str> = None;
    for (i, (pin, it)) in candidates.iter().enumerate() {
        if !taken[i] {
            continue;
        }
        if current_file != Some(pin.file) {
            if current_file.is_some() {
                text.push('\n');
            }
            text.push_str(&file_header(pin));
            current_file = Some(pin.file);
            current_section = None;
        }
        if shows_sections(pin)
            && !it.section.is_empty()
            && current_section != Some(it.section.as_str())
        {
            text.push_str(&section_header(&it.section));
            current_section = Some(it.section.as_str());
        }
        text.push_str(&render_item(it));
        text.push('\n');
        items.push(it.id.clone());
    }

    let omitted: Vec<String> = omitted_order
        .iter()
        .map(|&i| candidates[i].1.id.clone())
        .collect();
    let omitted_files: BTreeSet<&str> = omitted_order
        .iter()
        .map(|&i| candidates[i].0.file)
        .collect();
    // Snapshot paths in prompt order: conventions, rules, domain.
    let mut snapshot_files: Vec<&PinnedFile> = files
        .iter()
        .filter(|f| omitted_files.contains(f.file))
        .collect();
    snapshot_files.sort_by_key(|f| {
        ALWAYS_FILES
            .iter()
            .position(|n| *n == f.file)
            .unwrap_or(ALWAYS_FILES.len())
    });
    let snapshots: Vec<PathBuf> = snapshot_files
        .iter()
        .map(|f| f.snapshot_path.to_path_buf())
        .collect();

    if !omitted.is_empty() {
        text.push_str(&omitted_block(candidates, &omitted_order, &snapshots));
    }

    Selection {
        text,
        items,
        omitted,
        snapshots,
    }
}

/// `text` on one line, cut to [`OMITTED_TEXT_CHARS`] characters — at the
/// last word boundary when there is one in its second half.
fn short_text(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= OMITTED_TEXT_CHARS {
        return one_line;
    }
    let mut cut: String = one_line.chars().take(OMITTED_TEXT_CHARS).collect();
    if let Some(space) = cut.rfind(' ').filter(|&i| i >= cut.len() / 2) {
        cut.truncate(space);
    }
    format!("{}…", cut.trim_end())
}

/// What did not fit: an index, most relevant first, as far as
/// [`OMITTED_INDEX_BUDGET`] allows; then where to read all of it.
fn omitted_block(
    candidates: &[(PinnedFile, Item)],
    omitted_order: &[usize],
    snapshots: &[PathBuf],
) -> String {
    let mut index = String::new();
    let mut listed = 0;
    for &i in omitted_order {
        let it = &candidates[i].1;
        let line = format!("- {} — {}\n", it.id, short_text(&it.text));
        if index.len() + line.len() > OMITTED_INDEX_BUDGET {
            break;
        }
        index.push_str(&line);
        listed += 1;
    }
    let mut out = format!(
        "\nNot shown — over the knowledge prompt limit ({} item(s)), most relevant first:\n{index}",
        omitted_order.len()
    );
    let rest = omitted_order.len() - listed;
    if rest > 0 {
        out.push_str(&format!("- … and {rest} more\n"));
    }
    out.push_str(
        "\nBefore writing code, read the conventions that apply to the files you change \
         — and any item above that bears on them — in the accepted files:\n",
    );
    for p in snapshots {
        out.push_str(&format!("- {}\n", p.display()));
    }
    out
}

/// Bytes the always-on knowledge — every item of `conventions.md` and
/// `rules.md` — takes in a prompt, rendered as [`for_task`] renders it.
/// `files` are `(file name, accepted text)`; other files are ignored.
pub fn always_on_bytes(files: &[(&str, &str)]) -> usize {
    let mut total = 0;
    for (name, text) in files {
        if !ALWAYS_FILES.contains(name) {
            continue;
        }
        let its = items::items(text);
        if its.is_empty() {
            continue;
        }
        total += format!("### {name} (revision 0)\n").len() + 1;
        total += its
            .iter()
            .map(|it| render_item(it).len() + 1)
            .sum::<usize>();
    }
    total
}

/// A warning when the conventions and rules alone exceed the prompt limit:
/// a task then gets only the part most relevant to it, the rest by
/// reference. A warning, never a blocking check.
pub fn fit_warning(bytes: usize, limit: usize) -> Option<String> {
    (bytes > limit).then(|| {
        format!(
            "accepted conventions.md + rules.md take {bytes} bytes, over knowledge.prompt_limit \
             ({limit}): each task gets only the items most relevant to it, the rest by reference; \
             trim conventions/rules or raise `knowledge.prompt_limit` in .zforge/config.yaml"
        )
    })
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

    /// AC-01: every rules and conventions item is in (conventions first);
    /// a domain item the contract cites by ID is in; one that is neither
    /// cited nor in a named module is not.
    #[test]
    fn every_rules_and_conventions_item_and_cited_domain_items_are_included() {
        let (r, c, d) = (rules(), conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let search = "Constraints\n\nKnowledge: DOM-001\n";
        let s = for_task(&files, search, 10_000);
        assert_eq!(s.items, ["CONV-001", "RULE-001", "RULE-002", "DOM-001"]);
        assert!(s.text.find("### conventions.md") < s.text.find("### rules.md"));
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
        // Room for conventions.md's header and CONV-001, nothing more.
        let limit = block_len("conventions.md", 2, &c, &["CONV-001"]);
        let s = for_task(&files, "", limit);
        assert_eq!(s.items, ["CONV-001"]);
        assert_eq!(s.omitted, ["RULE-001", "RULE-002"]);
        // domain.md is never cited by the empty search text, so nothing of
        // it is even considered — only rules.md, which could not fit, is
        // named.
        assert_eq!(s.snapshots, [snaps[0].clone()]);
        assert!(s.text.contains("Not shown"), "{}", s.text);
        assert!(s.text.contains("RULE-002"), "{}", s.text);
        assert!(s.text.contains("/store/rules.md"), "{}", s.text);
    }

    /// Bytes `for_task` counts for `ids` of `text` under one file header.
    fn block_len(file: &str, revision: u32, text: &str, ids: &[&str]) -> usize {
        let header = format!("### {file} (revision {revision})\n").len() + 1;
        header
            + items::items(text)
                .iter()
                .filter(|it| ids.contains(&it.id.as_str()))
                .map(|it| render_item(it).len() + 1)
                .sum::<usize>()
    }

    /// Under a limit with room for one item, a convention is chosen over a
    /// rule of the same standing.
    #[test]
    fn conventions_are_chosen_before_rules() {
        let (r, c, d) = (rules(), conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let limit = block_len("conventions.md", 2, &c, &["CONV-001"]) + 5;
        let s = for_task(&files, "", limit);
        assert_eq!(s.items, ["CONV-001"]);
        assert!(s.omitted.contains(&"RULE-001".to_string()));
    }

    fn go_conventions() -> String {
        "## Structure\n\
         - CONV-001: each module lives in `internal/module/<name>/` with dto/ handler/ \
         service/ and init.go. (internal/module/user/init.go:1)\n\
         - CONV-002: errors wrap with %w. (pkg/errs/errs.go:10)\n"
            .into()
    }

    fn go_rules() -> String {
        "## general\n\
         - RULE-001: referral rewards are paid once. \
         (internal/module/referral_program/service/reward.go:40-52)\n\
         - RULE-002: the CLI exits non-zero on error. (cmd/tool/main.go:12)\n"
            .into()
    }

    /// Items related to the paths the task names — by a placeholder path
    /// in their text, or a citation in a sibling directory — win over
    /// unrelated ones under a tight limit; a relevant rule even comes
    /// before a general convention.
    #[test]
    fn relevant_items_win_over_unrelated_ones() {
        let (r, c, d) = (go_rules(), go_conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let search = "## Output\n\n- `internal/module/referral_program/handler/claim.go`\n";
        let limit = block_len("conventions.md", 2, &c, &["CONV-001"])
            + block_len("rules.md", 3, &r, &["RULE-001"]);
        let s = for_task(&files, search, limit);
        assert_eq!(s.items, ["CONV-001", "RULE-001"], "{}", s.text);
        assert_eq!(s.omitted, ["CONV-002", "RULE-002"]);

        // Without the path, the general convention is the one that fits.
        let limit = block_len("conventions.md", 2, &c, &["CONV-002"]);
        let s = for_task(&files, "", limit);
        assert_eq!(s.items, ["CONV-002"]);
    }

    /// A cited ID comes first, whatever its file.
    #[test]
    fn a_cited_item_comes_first() {
        let (r, c, d) = (go_rules(), go_conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let limit = block_len("rules.md", 3, &r, &["RULE-002"]);
        let s = for_task(&files, "Knowledge: RULE-002", limit);
        assert_eq!(s.items, ["RULE-002"]);
    }

    /// What did not fit is indexed one line per item — ID and the start of
    /// its text — most relevant first, then the snapshot paths with the
    /// instruction to read the conventions that apply.
    #[test]
    fn omitted_items_are_indexed_then_linked() {
        let (r, c, d) = (go_rules(), go_conventions(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let search = "`internal/module/referral_program/handler/claim.go`";
        let s = for_task(&files, search, 0);
        assert!(s.items.is_empty());
        let index = s.text.split("most relevant first:\n").nth(1).unwrap();
        let lines: Vec<&str> = index.lines().take(4).collect();
        assert_eq!(
            lines,
            [
                "- CONV-001 — each module lives in `internal/module/<name>/` with dto/ handler/ \
                 service/ and…",
                "- RULE-001 — referral rewards are paid once.",
                "- CONV-002 — errors wrap with %w.",
                "- RULE-002 — the CLI exits non-zero on error.",
            ]
        );
        let read_at = s
            .text
            .find("Before writing code, read the conventions that apply to the files you change")
            .expect("instruction");
        let conv_path = s.text.find("- /store/conventions.md").unwrap();
        let rules_path = s.text.find("- /store/rules.md").unwrap();
        assert!(read_at < conv_path && conv_path < rules_path, "{}", s.text);
    }

    /// The index stops at its own budget and says how many more there are;
    /// a long item is cut short.
    #[test]
    fn the_omitted_index_is_bounded() {
        let long = "x".repeat(300);
        let mut c = String::from("## general\n");
        for n in 1..=200 {
            c.push_str(&format!("- CONV-{n:03}: {long} (a/b.rs:{n})\n"));
        }
        let (r, d) = (rules(), domain());
        let snaps = test_snaps();
        let files = pins(&r, &c, &d, &snaps);
        let s = for_task(&files, "", 0);
        assert_eq!(s.omitted.len(), 202);
        let index_lines: Vec<&str> = s
            .text
            .lines()
            .filter(|l| l.starts_with("- CONV-") || l.starts_with("- RULE-"))
            .collect();
        assert!(index_lines.len() < 202);
        let index_bytes: usize = index_lines.iter().map(|l| l.len() + 1).sum();
        assert!(index_bytes <= OMITTED_INDEX_BUDGET, "{index_bytes}");
        assert!(index_lines[0].ends_with('…'), "{}", index_lines[0]);
        assert!(index_lines[0].chars().count() < 100);
        let more = 202 - index_lines.len();
        assert!(
            s.text.contains(&format!("- … and {more} more")),
            "{}",
            s.text
        );
    }

    /// Conventions and rules larger than the limit are warned about,
    /// naming both numbers; `domain.md` does not count.
    #[test]
    fn a_warning_when_conventions_and_rules_exceed_the_limit() {
        let (r, c, d) = (rules(), conventions(), domain());
        let files = [
            ("rules.md", r.as_str()),
            ("conventions.md", c.as_str()),
            ("domain.md", d.as_str()),
        ];
        let bytes = always_on_bytes(&files);
        let expected = block_len("rules.md", 0, &r, &["RULE-001", "RULE-002"])
            + block_len("conventions.md", 0, &c, &["CONV-001"]);
        assert_eq!(bytes, expected);
        assert_eq!(fit_warning(bytes, bytes), None);
        let w = fit_warning(bytes, bytes - 1).expect("over the limit");
        assert!(w.contains(&bytes.to_string()) && w.contains(&(bytes - 1).to_string()));
        assert!(w.contains("knowledge.prompt_limit"), "{w}");
        assert_eq!(always_on_bytes(&[("domain.md", d.as_str())]), 0);
    }

    #[test]
    fn related_directories() {
        let d = dir_segments;
        // Same directory, a sibling under a module, a container, a
        // placeholder segment.
        assert!(dirs_related(&d("a/b/c/x.go"), &d("a/b/c/y.go")));
        assert!(dirs_related(&d("a/b/c/x.go"), &d("a/b/d/y.go")));
        assert!(dirs_related(&d("a/b/"), &d("a/b/c/d/y.go")));
        assert!(dirs_related(&d("a/<name>/c"), &d("a/z/c/y.go")));
        // A top-level directory alone relates to nothing below it, nor do
        // siblings directly under a top-level one.
        assert!(!dirs_related(&d("a/"), &d("a/b/c/y.go")));
        assert!(!dirs_related(&d("a/b/x.go"), &d("a/c/y.go")));
        assert!(!dirs_related(&d("x.go"), &d("y.go")));
    }

    #[test]
    fn path_tokens_strip_locations_and_skip_urls() {
        assert_eq!(
            path_tokens("see (internal/a/b.go:12-20), https://x.io/y and `./c/d/`."),
            ["internal/a/b.go", "c/d/"]
        );
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
