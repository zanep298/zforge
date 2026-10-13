//! Reference docs outside the reviewed knowledge (`knowledge.extra`): a
//! team's cross-service docs folder, say. Never reviewed, linted or
//! pinned — zforge only lists them (`INDEX.md`), searches them
//! (`knowledge get --grep … --extra`) and names them to a run.
//!
//! Every read is bounded: a search walks at most [`MAX_FILES`] markdown
//! files of at most [`MAX_FILE_BYTES`] each and returns at most
//! [`MAX_HITS`] sections, each cut at [`MAX_SECTION_BYTES`].

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = 5_000;
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_HITS: usize = 20;
pub const MAX_SECTION_BYTES: usize = 3_000;

/// One heading section of a reference doc that matched.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hit {
    /// The path as configured, joined with the file's path inside it.
    pub path: String,
    /// The section's heading line without its `#`s; empty before the first.
    pub heading: String,
    /// 1-based line the section starts on.
    pub line: usize,
    pub text: String,
    /// `text` was cut at [`MAX_SECTION_BYTES`].
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Search {
    pub hits: Vec<Hit>,
    /// More sections matched than [`MAX_HITS`], or more files existed than
    /// [`MAX_FILES`]: narrow the text.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// Configured paths that do not exist.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// Markdown files under `dir` (or `dir` itself when it is one), sorted,
/// skipping hidden entries and `node_modules`; at most `limit`.
pub fn markdown_files(dir: &Path, limit: usize) -> (Vec<PathBuf>, bool) {
    let mut out = Vec::new();
    if dir.is_file() {
        if is_markdown(dir) {
            out.push(dir.to_path_buf());
        }
        return (out, false);
    }
    let mut stack = vec![dir.to_path_buf()];
    let mut truncated = false;
    while let Some(d) = stack.pop() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&d)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && name != "node_modules"
            })
            .map(|e| e.path())
            .collect();
        entries.sort();
        // Directories are pushed in reverse so they pop in order.
        for p in entries.iter().rev().filter(|p| p.is_dir()) {
            stack.push(p.clone());
        }
        for p in entries.iter().filter(|p| p.is_file() && is_markdown(p)) {
            if out.len() >= limit {
                truncated = true;
                break;
            }
            out.push(p.clone());
        }
        if truncated {
            break;
        }
    }
    out.sort();
    (out, truncated)
}

fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// `shown` joined with `file`'s path relative to `base`.
pub fn display(shown: &str, base: &Path, file: &Path) -> String {
    match file.strip_prefix(base) {
        Ok(rel) if !rel.as_os_str().is_empty() => format!(
            "{}/{}",
            shown.trim_end_matches('/'),
            rel.to_string_lossy().replace('\\', "/")
        ),
        _ => shown.to_string(),
    }
}

/// A markdown file's heading sections: `(heading, start line, text)`.
/// A `#` line inside a fenced block is not a heading.
fn sections(text: &str) -> Vec<(String, usize, String)> {
    let mut out: Vec<(String, usize, String)> = Vec::new();
    let mut cur = (String::new(), 1usize, String::new());
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && line.starts_with('#') {
            if !cur.2.trim().is_empty() || !cur.0.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur = (
                line.trim_start_matches('#').trim().to_string(),
                i + 1,
                String::new(),
            );
        }
        cur.2.push_str(line);
        cur.2.push('\n');
    }
    if !cur.2.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// The first `#` heading of a markdown file, if any (cheap: the first
/// 64 KB only).
pub fn first_heading(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut buf)
        .ok()?;
    let text = String::from_utf8_lossy(&buf);
    sections(&text)
        .into_iter()
        .map(|(h, _, _)| h)
        .find(|h| !h.is_empty())
}

fn cut(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.trim_end().to_string(), false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}\n…", text[..end].trim_end()), true)
}

/// Every heading section of the markdown under `dirs` containing `needle`
/// (case-insensitive), bounded as the module says.
pub fn search(dirs: &[(String, PathBuf)], needle: &str) -> Search {
    let mut s = Search::default();
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return s;
    }
    s.missing = dirs
        .iter()
        .filter(|(_, dir)| !dir.exists())
        .map(|(shown, _)| shown.clone())
        .collect();
    let mut budget = MAX_FILES;
    for (shown, dir) in dirs.iter().filter(|(_, dir)| dir.exists()) {
        let (files, truncated) = markdown_files(dir, budget);
        s.truncated |= truncated;
        budget = budget.saturating_sub(files.len());
        for f in files {
            if std::fs::metadata(&f).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            if !text.to_lowercase().contains(&needle) {
                continue;
            }
            for (heading, line, body) in sections(&text) {
                if !body.to_lowercase().contains(&needle) {
                    continue;
                }
                if s.hits.len() >= MAX_HITS {
                    s.truncated = true;
                    return s;
                }
                let (text, was_cut) = cut(&body, MAX_SECTION_BYTES);
                s.hits.push(Hit {
                    path: display(shown, dir, &f),
                    heading,
                    line,
                    text,
                    cut: was_cut,
                });
            }
        }
    }
    s
}

pub fn render(s: &Search) -> String {
    let mut out = String::new();
    if s.hits.is_empty() {
        out.push_str("no reference doc section matches\n");
    }
    for h in &s.hits {
        out.push_str(&format!(
            "--- {}:{} — {}\n{}\n\n",
            h.path,
            h.line,
            if h.heading.is_empty() {
                "(top)"
            } else {
                &h.heading
            },
            h.text
        ));
    }
    if s.truncated {
        out.push_str(&format!(
            "… more matches than shown (at most {MAX_HITS} sections); narrow the text\n"
        ));
    }
    for m in &s.missing {
        out.push_str(&format!("reference path not found: {m}\n"));
    }
    out
}

/// The "Reference docs" section of `INDEX.md`: per configured path, its
/// `README.md` entry point first, then its other top-level markdown files
/// with their first heading, then its subdirectories with how many
/// markdown files each holds. Empty when nothing is configured.
pub fn index_section(dirs: &[(String, PathBuf)]) -> String {
    if dirs.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Reference docs\n\nNot reviewed and not pinned — read only what applies. \
         Search: `zforge knowledge get --grep <text> --extra`.\n",
    );
    for (shown, dir) in dirs {
        out.push_str(&format!("\n### {shown}\n\n"));
        if !dir.exists() {
            out.push_str("- not found\n");
            continue;
        }
        if dir.is_file() {
            out.push_str(&doc_line(shown, dir, dir));
            continue;
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .collect();
        entries.sort();
        let is_readme = |p: &PathBuf| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("readme.md"))
        };
        let files: Vec<&PathBuf> = entries
            .iter()
            .filter(|p| p.is_file() && is_markdown(p))
            .collect();
        for f in files.iter().filter(|p| is_readme(p)) {
            let mut line = doc_line(shown, dir, f);
            line.insert_str(line.len() - 1, " (entry point)");
            out.push_str(&line);
        }
        for f in files.iter().filter(|p| !is_readme(p)) {
            out.push_str(&doc_line(shown, dir, f));
        }
        for d in entries
            .iter()
            .filter(|p| p.is_dir() && !p.ends_with("node_modules"))
        {
            let (inside, more) = markdown_files(d, 1_000);
            if inside.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "- {}/ — {}{} markdown files\n",
                display(shown, dir, d),
                inside.len(),
                if more { "+" } else { "" }
            ));
        }
    }
    out
}

fn doc_line(shown: &str, base: &Path, f: &Path) -> String {
    match first_heading(f) {
        Some(h) => format!("- {} — {h}\n", display(shown, base, f)),
        None => format!("- {}\n", display(shown, base, f)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs(root: &Path) -> PathBuf {
        let d = root.join("documents");
        std::fs::create_dir_all(d.join("services")).unwrap();
        std::fs::create_dir_all(d.join("flows")).unwrap();
        std::fs::write(
            d.join("README.md"),
            "# Team docs\n\nStart here.\n\n## Layout\n\nservices/, flows/\n",
        )
        .unwrap();
        std::fs::write(d.join("glossary.md"), "# Glossary\n\nLedger: the book.\n").unwrap();
        std::fs::write(
            d.join("services/billing.md"),
            "# Billing service\n\nOwns invoices.\n\n## Ledger posting\n\nEvery invoice posts to the ledger.\n\n```\n# not a heading ledger\n```\n\n## Retries\n\nNothing about it.\n",
        )
        .unwrap();
        std::fs::write(d.join("flows/checkout.md"), "# Checkout\n\nPays.\n").unwrap();
        std::fs::write(d.join("flows/notes.txt"), "ledger in a text file\n").unwrap();
        d
    }

    #[test]
    fn search_returns_matching_sections_with_their_path_and_heading() {
        let tmp = tempfile::tempdir().unwrap();
        let d = docs(tmp.path());
        let s = search(&[("../documents".into(), d)], "LEDGER");
        let found: Vec<(&str, &str)> = s
            .hits
            .iter()
            .map(|h| (h.path.as_str(), h.heading.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("../documents/glossary.md", "Glossary"),
                ("../documents/services/billing.md", "Ledger posting"),
            ],
            "{s:?}"
        );
        assert!(s.hits[1].text.contains("posts to the ledger"));
        assert!(
            s.hits[1].text.contains("not a heading"),
            "fence stays inside"
        );
        assert!(!s.truncated);
    }

    #[test]
    fn search_is_bounded_and_names_a_missing_path() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("many");
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..(MAX_HITS + 5) {
            std::fs::write(
                d.join(format!("{i:03}.md")),
                format!("# Doc {i}\n\n{}\n", "needle ".repeat(1_000)),
            )
            .unwrap();
        }
        let s = search(
            &[("many".into(), d), ("gone".into(), tmp.path().join("nope"))],
            "needle",
        );
        assert_eq!(s.hits.len(), MAX_HITS);
        assert!(s.truncated);
        assert!(s
            .hits
            .iter()
            .all(|h| h.cut && h.text.len() <= MAX_SECTION_BYTES + 4));
        assert_eq!(s.missing, vec!["gone".to_string()]);
    }

    #[test]
    fn index_section_lists_the_entry_point_top_level_docs_and_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        let d = docs(tmp.path());
        let text = index_section(&[
            ("../documents".into(), d),
            ("../missing".into(), tmp.path().join("missing")),
        ]);
        let readme = text
            .find("- ../documents/README.md — Team docs (entry point)\n")
            .expect(&text);
        let glossary = text
            .find("- ../documents/glossary.md — Glossary\n")
            .expect(&text);
        assert!(readme < glossary, "{text}");
        assert!(
            text.contains("- ../documents/flows/ — 1 markdown files\n"),
            "{text}"
        );
        assert!(
            text.contains("- ../documents/services/ — 1 markdown files\n"),
            "{text}"
        );
        assert!(text.contains("### ../missing\n\n- not found\n"), "{text}");
        assert!(index_section(&[]).is_empty());
    }
}
