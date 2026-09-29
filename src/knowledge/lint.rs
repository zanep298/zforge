//! Evidence, ID and `covers` lint for knowledge files (ONBOARD REQ-004,
//! TASK-002). Run by `Knowledge::lint`, which `intake::review::review`
//! calls before a file is sent for review — every error here refuses the
//! review (Output).
//!
//! Evidence is read with `git show <pinned>:<path>` (Constraints): the
//! working tree is never consulted, so a file edited after `pinned` does
//! not affect the check (AC-06). Commands run with `Knowledge.dir` as the
//! working directory; git resolves both the commit and root-relative
//! citation paths the same way regardless of the caller's cwd inside the
//! repository, so no separate lookup of the repository root is needed.

use super::items::{self, Cite, Item};
use super::Knowledge;
use crate::intake::lint::{Heading, Issue, Severity};
use crate::intake::record::{self, DecisionKind};
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn err(rel: &str, message: String) -> Issue {
    Issue {
        file: rel.to_string(),
        severity: Severity::Error,
        message,
    }
}

fn warn(rel: &str, message: String) -> Issue {
    Issue {
        file: rel.to_string(),
        severity: Severity::Warning,
        message,
    }
}

/// Item, evidence, ID and `covers` issues in `rel`'s `text` (Output).
pub fn lint(k: &Knowledge, rel: &str, text: &str) -> Vec<Issue> {
    let parsed = items::parse(text);
    let expected_prefix = items::expected_prefix(rel);
    let mut issues = Vec::new();

    // A list item outside "Open questions" that is not an item, or has no
    // evidence (Output, AC-02).
    for u in &parsed.unparsed {
        issues.push(match &u.id {
            Some(id) => err(
                rel,
                format!(
                    "{id} (line {}): statement has no evidence — end it with `(path:line, ...)`",
                    u.line
                ),
            ),
            None => err(
                rel,
                format!(
                    "line {}: list item is not a knowledge item — start it with `- {}-NNN: text (path:line)`",
                    u.line,
                    expected_prefix.unwrap_or("ID")
                ),
            ),
        });
    }

    // An ID with the wrong prefix, or used twice (Output, AC-04).
    let mut first_seen: HashMap<&str, usize> = HashMap::new();
    for it in &parsed.items {
        if let Some(prefix) = expected_prefix {
            let actual = it.id.split('-').next().unwrap_or("");
            if actual != prefix {
                issues.push(err(
                    rel,
                    format!(
                        "{} (line {}): {rel} items use `{prefix}-`, not `{actual}-`",
                        it.id, it.line
                    ),
                ));
            }
        }
        if let Some(&first) = first_seen.get(it.id.as_str()) {
            issues.push(err(
                rel,
                format!(
                    "{} is used twice (line {first} and line {})",
                    it.id, it.line
                ),
            ));
        } else {
            first_seen.insert(&it.id, it.line);
        }
    }

    // Evidence at `pinned`, only when there is something to check against
    // it (Output; existing knowledge fixtures with no items stay valid).
    if !parsed.items.is_empty() {
        match items::pinned(text) {
            None => issues.push(err(
                rel,
                "missing `pinned` commit in frontmatter — items need one to check evidence against"
                    .into(),
            )),
            Some(pinned) => {
                if !commit_exists(&k.dir, &pinned) {
                    issues.push(err(
                        rel,
                        format!("`pinned: {pinned}` is not a known commit"),
                    ));
                } else {
                    for it in &parsed.items {
                        for cite in &it.evidence {
                            if let Some(issue) = check_cite(&k.dir, rel, it, &pinned, cite) {
                                issues.push(issue);
                            }
                        }
                    }
                }
            }
        }
    }

    issues.extend(retired_id_issues(k, rel, &parsed.items));
    issues.extend(covers_warnings(k, rel, text));
    issues
}

fn commit_exists(dir: &Path, sha: &str) -> bool {
    crate::run::git::run(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{sha}^{{commit}}"),
        ],
    )
    .map(|o| o.status.success())
    .unwrap_or(false)
}

/// `git show <pinned>:<path>`'s raw output, or `None` when the path does
/// not exist at that commit.
fn show_at(dir: &Path, pinned: &str, path: &str) -> Option<String> {
    let out = crate::run::git::run(dir, &["show", &format!("{pinned}:{path}")]).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn cite_label(cite: &Cite) -> String {
    if cite.start == cite.end {
        format!("{}:{}", cite.path, cite.start)
    } else {
        format!("{}:{}-{}", cite.path, cite.start, cite.end)
    }
}

fn check_cite(dir: &Path, rel: &str, item: &Item, pinned: &str, cite: &Cite) -> Option<Issue> {
    let Some(content) = show_at(dir, pinned, &cite.path) else {
        return Some(err(
            rel,
            format!(
                "{} (line {}): {} does not exist at {pinned}",
                item.id, item.line, cite.path
            ),
        ));
    };
    if cite.start == 0 {
        return Some(err(
            rel,
            format!(
                "{} (line {}): {} — line numbers start at 1",
                item.id,
                item.line,
                cite_label(cite)
            ),
        ));
    }
    if cite.start > cite.end {
        return Some(err(
            rel,
            format!(
                "{} (line {}): {} — start is after end",
                item.id,
                item.line,
                cite_label(cite)
            ),
        ));
    }
    let total = content.lines().count() as u32;
    if cite.end > total {
        return Some(err(
            rel,
            format!("{}: {} — file has {total} lines", item.id, cite_label(cite)),
        ));
    }
    None
}

/// An ID present in an earlier accepted revision, dropped from a later
/// accepted one, and back again (Output, AC-05; business rule 3: "a
/// removed item's ID is not reused").
fn retired_id_issues(k: &Knowledge, rel: &str, items: &[Item]) -> Vec<Issue> {
    let Ok(log) = record::read(k) else {
        return Vec::new();
    };
    let mut history: Vec<HashSet<String>> = Vec::new();
    for d in log
        .iter()
        .filter(|d| d.file == rel && d.decision == DecisionKind::Accepted)
    {
        let Ok(text) = record::read_snapshot(k, rel, d.revision) else {
            continue;
        };
        history.push(items::items(&text).into_iter().map(|it| it.id).collect());
    }
    let Some(latest) = history.last() else {
        return Vec::new();
    };
    let mut ever = HashSet::new();
    for ids in &history {
        ever.extend(ids.iter().cloned());
    }
    let retired: HashSet<&String> = ever.iter().filter(|id| !latest.contains(*id)).collect();
    items
        .iter()
        .filter(|it| retired.contains(&it.id))
        .map(|it| {
            err(
                rel,
                format!(
                    "{} (line {}): was dropped from an accepted revision and its ID may not be reused",
                    it.id, it.line
                ),
            )
        })
        .collect()
}

/// `covers` in `domain.md`'s frontmatter should name every module
/// `baseline.md` lists, when the probe (ONBOARD TASK-003) has written one
/// (Output). `baseline.md`'s own format is TASK-003's to define; until it
/// lands this reads a `## Modules` section of `- <module> …` items, taking
/// each line's leading token as the module name — absent that section (or
/// the file), this is silently skipped, matching "if baseline.md exists".
fn covers_warnings(k: &Knowledge, rel: &str, text: &str) -> Vec<Issue> {
    if rel != "domain.md" {
        return Vec::new();
    }
    let Ok(baseline) = std::fs::read_to_string(k.dir.join("baseline.md")) else {
        return Vec::new();
    };
    let listed = baseline_modules(&baseline);
    if listed.is_empty() {
        return Vec::new();
    }
    let covered: HashSet<String> = items::covers(text).into_iter().collect();
    listed
        .into_iter()
        .filter(|m| !covered.contains(m))
        .map(|m| {
            warn(
                rel,
                format!("`covers` does not list {m}, which baseline.md lists"),
            )
        })
        .collect()
}

const MODULES: Heading = Heading {
    name: "Modules",
    legacy: "Modules",
};

fn baseline_modules(text: &str) -> Vec<String> {
    let secs = crate::intake::lint::sections(text);
    let Some(lines) = crate::intake::lint::section(&secs, MODULES) else {
        return Vec::new();
    };
    lines
        .iter()
        .filter_map(|l| {
            let t = l.trim_start();
            let t = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
            t.split_whitespace().next().map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "user.name", "t"]);
    }

    /// Write and commit `rel` with `content`; the commit it lands on.
    fn commit_file(root: &Path, rel: &str, content: &str) -> String {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "evidence"]);
        String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    fn knowledge(root: &Path) -> Knowledge {
        let dir = root.join("docs/knowledge");
        std::fs::create_dir_all(&dir).unwrap();
        Knowledge { dir }
    }

    fn ten_lines() -> String {
        (1..=10).map(|n| format!("line{n}\n")).collect()
    }

    fn errors(issues: &[Issue]) -> Vec<&str> {
        issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.message.as_str())
            .collect()
    }

    /// AC-01: a file whose items all cite existing lines at `pinned`
    /// passes review (no errors).
    #[test]
    fn an_item_citing_real_lines_at_pinned_has_no_errors() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let sha = commit_file(tmp.path(), "src/x.rs", &ten_lines());
        let text = format!(
            "---\npinned: {sha}\n---\n## m\n- DOM-001: a thing happens. (src/x.rs:3)\n\n## Open questions\n"
        );
        let k = knowledge(tmp.path());
        assert!(errors(&lint(&k, "domain.md", &text)).is_empty());
    }

    /// AC-02: a statement without evidence is an error naming its line.
    #[test]
    fn a_statement_without_evidence_is_an_error_naming_its_line() {
        let text = "## m\n- DOM-001: a thing happens with no citation\n\n## Open questions\n";
        let k = Knowledge {
            dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        };
        let issues = lint(&k, "domain.md", text);
        let e = errors(&issues);
        assert!(e.iter().any(|m| m.contains("line 2")), "{e:?}");
        assert!(e.iter().any(|m| m.contains("no evidence")), "{e:?}");
    }

    /// AC-03: a citation past the file's end is refused naming the file's
    /// line count; a path absent at `pinned` is refused too.
    #[test]
    fn a_citation_past_the_end_or_to_a_missing_file_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let sha = commit_file(tmp.path(), "internal/token/rotate.go", &"x\n".repeat(143));
        let text = format!(
            "---\npinned: {sha}\n---\n## m\n- DOM-007: too far. (internal/token/rotate.go:200)\n- DOM-008: missing file. (nowhere.go:1)\n\n## Open questions\n"
        );
        let k = knowledge(tmp.path());
        let issues = lint(&k, "domain.md", &text);
        let e = errors(&issues);
        assert!(e.iter().any(|m| m.contains("file has 143 lines")), "{e:?}");
        assert!(
            e.iter()
                .any(|m| m.contains("nowhere.go") && m.contains("does not exist")),
            "{e:?}"
        );
    }

    /// AC-04: a duplicate ID, or `RULE-` in `domain.md`, is refused.
    #[test]
    fn duplicate_ids_and_the_wrong_file_prefix_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let sha = commit_file(tmp.path(), "src/x.rs", &ten_lines());
        let text = format!(
            "---\npinned: {sha}\n---\n## m\n- DOM-001: first. (src/x.rs:1)\n- DOM-001: again. (src/x.rs:2)\n- RULE-001: misplaced. (src/x.rs:3)\n\n## Open questions\n"
        );
        let k = knowledge(tmp.path());
        let issues = lint(&k, "domain.md", &text);
        let e = errors(&issues);
        assert!(
            e.iter()
                .any(|m| m.contains("DOM-001") && m.contains("used twice")),
            "{e:?}"
        );
        assert!(
            e.iter()
                .any(|m| m.contains("RULE-001") && m.contains("not") && m.contains("RULE-")),
            "{e:?}"
        );
    }

    /// AC-05: an ID dropped in accepted revision 2 and written again in
    /// revision 3 is refused.
    #[test]
    fn an_id_dropped_from_an_accepted_revision_cannot_come_back() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let sha = commit_file(tmp.path(), "src/x.rs", &ten_lines());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();

        let rev1 = format!(
            "---\npinned: {sha}\n---\n## m\n- DOM-001: first. (src/x.rs:1)\n\n## Open questions\n"
        );
        record::write_snapshot(&k, "domain.md", 1, &rev1).unwrap();
        record::append(
            &k,
            &record::Decision {
                at: chrono::Utc::now(),
                file: "domain.md".into(),
                revision: 1,
                sha256: crate::intake::hash::sha256(&rev1),
                decision: DecisionKind::Accepted,
                channel: "cli-tty".into(),
                by: None,
                note: String::new(),
            },
        )
        .unwrap();

        let rev2 =
            format!("---\npinned: {sha}\n---\n## m\n- DOM-002: only this now. (src/x.rs:2)\n\n## Open questions\n");
        record::write_snapshot(&k, "domain.md", 2, &rev2).unwrap();
        record::append(
            &k,
            &record::Decision {
                at: chrono::Utc::now(),
                file: "domain.md".into(),
                revision: 2,
                sha256: crate::intake::hash::sha256(&rev2),
                decision: DecisionKind::Accepted,
                channel: "cli-tty".into(),
                by: None,
                note: String::new(),
            },
        )
        .unwrap();

        let rev3 = format!(
            "---\npinned: {sha}\n---\n## m\n- DOM-002: still here. (src/x.rs:2)\n- DOM-001: back again. (src/x.rs:1)\n\n## Open questions\n"
        );
        let issues = lint(&k, "domain.md", &rev3);
        let e = errors(&issues);
        assert!(
            e.iter()
                .any(|m| m.contains("DOM-001") && m.contains("may not be reused")),
            "{e:?}"
        );
        assert!(!e.iter().any(|m| m.contains("DOM-002")), "{e:?}");
    }

    /// AC-06: evidence is read at `pinned`, never the working tree — a
    /// cited file changed afterwards does not affect the check.
    #[test]
    fn a_working_tree_edit_after_pinned_does_not_affect_the_check() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let sha = commit_file(tmp.path(), "src/x.rs", &ten_lines());
        // Now shrink the working copy well below the cited line — the
        // commit is untouched, so the check must still pass.
        std::fs::write(tmp.path().join("src/x.rs"), "only one line\n").unwrap();

        let text = format!(
            "---\npinned: {sha}\n---\n## m\n- DOM-001: a thing happens. (src/x.rs:9)\n\n## Open questions\n"
        );
        let k = knowledge(tmp.path());
        assert!(errors(&lint(&k, "domain.md", &text)).is_empty());
    }

    /// A file with no items needs no `pinned` — keeps existing knowledge
    /// fixtures (drafted before TASK-002) valid.
    #[test]
    fn a_file_with_no_items_does_not_require_pinned() {
        let text = "# Domain\n\nA thing happens. (src/x.rs:1)\n\n## Open questions\n";
        let k = Knowledge {
            dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        };
        assert!(errors(&lint(&k, "domain.md", text)).is_empty());
    }

    /// Items need `pinned` to be checked against.
    #[test]
    fn items_without_a_pinned_commit_are_refused() {
        let text = "## m\n- DOM-001: a thing. (src/x.rs:1)\n\n## Open questions\n";
        let k = Knowledge {
            dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        };
        let issues = lint(&k, "domain.md", text);
        let e = errors(&issues);
        assert!(e.iter().any(|m| m.contains("pinned")), "{e:?}");
    }

    /// `covers` missing a module `baseline.md` lists is a warning, not an
    /// error, and only fires when `baseline.md` exists.
    #[test]
    fn covers_warns_only_against_an_existing_baseline() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let sha = commit_file(tmp.path(), "src/x.rs", &ten_lines());
        let k = knowledge(tmp.path());
        std::fs::create_dir_all(&k.dir).unwrap();
        let text = format!(
            "---\npinned: {sha}\ncovers: [internal/token]\n---\n## internal/token\n- DOM-001: x. (src/x.rs:1)\n\n## Open questions\n"
        );

        // No baseline.md yet: no warning either way.
        assert!(lint(&k, "domain.md", &text).is_empty());

        std::fs::write(
            k.dir.join("baseline.md"),
            "# Baseline\n\n## Modules\n\n- internal/token (12 files)\n- internal/audit (4 files)\n",
        )
        .unwrap();
        let issues = lint(&k, "domain.md", &text);
        assert!(errors(&issues).is_empty());
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Warning && i.message.contains("internal/audit")),
            "{issues:?}"
        );
    }
}
