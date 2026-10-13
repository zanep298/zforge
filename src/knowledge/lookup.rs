//! Read accepted knowledge items without reading whole files: by ID, by
//! module (a `## ` section, or a path the item cites) or by text
//! (`zforge knowledge get`, MCP `knowledge_get`). Read-only, from the
//! *accepted* snapshots only — a draft never answers.

use super::items::{self, Cite};
use super::{reference, Knowledge, FILES};
use crate::intake::record;
use crate::intake::review as intake_review;
use anyhow::{bail, Result};
use serde::Serialize;
use std::path::PathBuf;

/// At most this many items in one answer; the rest is counted, not shown.
pub const MAX_ITEMS: usize = 200;

/// One knowledge file's accepted revision and its text.
#[derive(Debug, Clone)]
pub struct AcceptedFile {
    pub file: String,
    pub revision: u32,
    pub text: String,
}

/// Every one of [`FILES`] that has an accepted revision, in order. A
/// project that never onboarded has none — not an error.
pub fn accepted_files(k: &Knowledge) -> Result<Vec<AcceptedFile>> {
    let mut out = Vec::new();
    for file in FILES {
        if let Some(rev) = intake_review::file_status(k, file)?.accepted {
            out.push(AcceptedFile {
                file: file.to_string(),
                revision: rev.revision,
                text: record::read_snapshot(k, file, rev.revision)?,
            });
        }
    }
    Ok(out)
}

/// One accepted item, in full.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub file: String,
    pub revision: u32,
    pub section: String,
    pub text: String,
    pub evidence: Vec<Cite>,
}

/// What to look up. Every filter given must hold (AND); at least one of
/// `ids`, `module`, `grep` is required.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub ids: Vec<String>,
    /// A `## ` section name (case-insensitive, substring) or a path prefix
    /// one of the item's citations starts with.
    pub module: Option<String>,
    /// Case-insensitive text in the item's ID, section or text.
    pub grep: Option<String>,
    /// Also search `knowledge.extra` (needs `grep`).
    pub extra: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Answer {
    pub items: Vec<Found>,
    /// Matching items left out past [`MAX_ITEMS`].
    #[serde(skip_serializing_if = "is_zero")]
    pub omitted: usize,
    /// IDs asked for that the accepted knowledge does not state.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<reference::Search>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Answer `q` from the accepted knowledge (and, with `q.extra`, the
/// reference docs in `extra`).
pub fn get(k: &Knowledge, q: &Query, extra: &[(String, PathBuf)]) -> Result<Answer> {
    let grep = q.grep.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let module = q.module.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if q.ids.is_empty() && module.is_none() && grep.is_none() {
        bail!("name item IDs, --module <section|path> or --grep <text>");
    }
    if q.extra && grep.is_none() {
        bail!("--extra searches the reference docs by text: add --grep <text>");
    }
    let ids: Vec<String> = q.ids.iter().map(|i| i.trim().to_uppercase()).collect();
    let mut all = Vec::new();
    for f in accepted_files(k)? {
        for it in items::items(&f.text) {
            all.push(Found {
                id: it.id,
                file: f.file.clone(),
                revision: f.revision,
                section: it.section,
                text: it.text,
                evidence: it.evidence,
            });
        }
    }
    let mut answer = Answer {
        missing: ids
            .iter()
            .filter(|id| !all.iter().any(|f| &f.id == *id))
            .cloned()
            .collect(),
        ..Answer::default()
    };
    let grep_lc = grep.map(str::to_lowercase);
    let matching = all.into_iter().filter(|f| {
        (ids.is_empty() || ids.contains(&f.id))
            && module.map_or(true, |m| in_module(f, m))
            && grep_lc.as_deref().map_or(true, |g| {
                f.id.to_lowercase().contains(g)
                    || f.section.to_lowercase().contains(g)
                    || f.text.to_lowercase().contains(g)
            })
    });
    for f in matching {
        if answer.items.len() < MAX_ITEMS {
            answer.items.push(f);
        } else {
            answer.omitted += 1;
        }
    }
    if q.extra {
        answer.extra = Some(reference::search(extra, grep.unwrap_or_default()));
    }
    Ok(answer)
}

/// The item is under a section named like `m`, or cites a path under `m`.
fn in_module(f: &Found, m: &str) -> bool {
    let lc = m.to_lowercase();
    if f.section.to_lowercase().contains(&lc) {
        return true;
    }
    let prefix = m.trim_start_matches("./").trim_end_matches('/');
    !prefix.is_empty()
        && f.evidence.iter().any(|c| {
            c.path == prefix
                || c.path
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
}

/// The answer as text for a terminal or an agent reading CLI output.
pub fn render(a: &Answer) -> String {
    let mut out = String::new();
    for f in &a.items {
        let section = if f.section.is_empty() {
            String::new()
        } else {
            format!(" › {}", f.section)
        };
        out.push_str(&format!(
            "{} — {} rev {}{section}\n{}\nevidence: {}\n\n",
            f.id,
            f.file,
            f.revision,
            f.text,
            f.evidence
                .iter()
                .map(cite_label)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if a.items.is_empty() {
        out.push_str("no accepted knowledge item matches\n");
    }
    if a.omitted > 0 {
        out.push_str(&format!(
            "… {} more matching item(s) not shown; narrow the query\n",
            a.omitted
        ));
    }
    if !a.missing.is_empty() {
        out.push_str(&format!(
            "not in the accepted knowledge: {}\n",
            a.missing.join(", ")
        ));
    }
    if let Some(s) = &a.extra {
        out.push_str(&reference::render(s));
    }
    out
}

pub fn cite_label(c: &Cite) -> String {
    if c.start == c.end {
        format!("{}:{}", c.path, c.start)
    } else {
        format!("{}:{}-{}", c.path, c.start, c.end)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;

    pub(crate) fn repo_with_code(root: &Path) -> String {
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::create_dir_all(root.join("src/billing")).unwrap();
        std::fs::create_dir_all(root.join("src/auth")).unwrap();
        let ten = "l\n".repeat(10);
        std::fs::write(root.join("src/billing/invoice.rs"), &ten).unwrap();
        std::fs::write(root.join("src/auth/token.rs"), &ten).unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "c"]);
        git(&["rev-parse", "HEAD"])
    }

    pub(crate) const DOMAIN_BODY: &str = "## billing\n\
- DOM-001: An invoice is immutable once issued; corrections are credit notes. (src/billing/invoice.rs:1-3)\n\
- DOM-002: Totals are computed in minor units. (src/billing/invoice.rs:5)\n\n\
## auth\n\
- DOM-003: A refresh token is single-use. (src/auth/token.rs:2)\n\n\
## Open questions\n";

    pub(crate) fn accept(root: &Path, k: &Knowledge, rel: &str, text: &str) {
        std::fs::create_dir_all(&k.dir).unwrap();
        std::fs::write(k.dir.join(rel), text).unwrap();
        crate::knowledge::review(root, k, rel).unwrap();
        crate::knowledge::accept(k, rel, None).unwrap();
    }

    fn setup() -> (tempfile::TempDir, Knowledge) {
        let tmp = tempfile::tempdir().unwrap();
        let pinned = repo_with_code(tmp.path());
        let k = Knowledge {
            dir: tmp.path().join("docs/knowledge"),
        };
        accept(
            tmp.path(),
            &k,
            "domain.md",
            &format!("---\npinned: {pinned}\n---\n# Domain\n\n{DOMAIN_BODY}"),
        );
        (tmp, k)
    }

    fn q() -> Query {
        Query::default()
    }

    #[test]
    fn get_by_id_returns_the_full_item_and_names_the_missing_one() {
        let (_tmp, k) = setup();
        let a = get(
            &k,
            &Query {
                ids: vec!["dom-001".into(), "DOM-999".into()],
                ..q()
            },
            &[],
        )
        .unwrap();
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].id, "DOM-001");
        assert_eq!(a.items[0].section, "billing");
        assert!(a.items[0].text.contains("credit notes"), "{:?}", a.items[0]);
        assert_eq!(a.items[0].evidence[0].path, "src/billing/invoice.rs");
        assert_eq!(a.missing, vec!["DOM-999".to_string()]);
    }

    #[test]
    fn get_by_module_matches_a_section_or_a_cited_path() {
        let (_tmp, k) = setup();
        let by_section = get(
            &k,
            &Query {
                module: Some("Billing".into()),
                ..q()
            },
            &[],
        )
        .unwrap();
        let ids: Vec<&str> = by_section.items.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["DOM-001", "DOM-002"]);

        let by_path = get(
            &k,
            &Query {
                module: Some("src/auth/".into()),
                ..q()
            },
            &[],
        )
        .unwrap();
        let ids: Vec<&str> = by_path.items.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["DOM-003"]);

        // `src/aut` is not a path prefix of `src/auth/…`.
        let partial = get(
            &k,
            &Query {
                module: Some("src/aut".into()),
                ..q()
            },
            &[],
        )
        .unwrap();
        assert!(partial.items.is_empty());
    }

    #[test]
    fn get_by_grep_is_case_insensitive_and_combines_with_module() {
        let (_tmp, k) = setup();
        let a = get(
            &k,
            &Query {
                grep: Some("MINOR UNITS".into()),
                ..q()
            },
            &[],
        )
        .unwrap();
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].id, "DOM-002");

        let none = get(
            &k,
            &Query {
                grep: Some("minor units".into()),
                module: Some("auth".into()),
                ..q()
            },
            &[],
        )
        .unwrap();
        assert!(none.items.is_empty());
        assert!(render(&none).contains("no accepted knowledge item matches"));
    }

    #[test]
    fn a_draft_never_answers_and_an_empty_query_is_refused() {
        let (_tmp, k) = setup();
        std::fs::write(
            k.dir.join("domain.md"),
            "# Domain\n\n- DOM-050: drafted only. (src/auth/token.rs:1)\n",
        )
        .unwrap();
        let a = get(
            &k,
            &Query {
                ids: vec!["DOM-050".into()],
                ..q()
            },
            &[],
        )
        .unwrap();
        assert!(a.items.is_empty());
        assert!(get(&k, &q(), &[]).is_err());
        assert!(get(
            &k,
            &Query {
                ids: vec!["DOM-001".into()],
                extra: true,
                ..q()
            },
            &[]
        )
        .is_err());
    }
}
