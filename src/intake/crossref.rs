//! References from one intake to things outside it, checked against the
//! project rather than the intake alone:
//!
//! - **supersedes** — a requirement line in `01-outcome.md` may end with
//!   `(supersedes <INTAKE>/REQ-nnn[, <INTAKE>/REQ-nnn])`: this requirement
//!   replaces one another intake accepted. Each target must name an
//!   existing intake whose *accepted* 01-outcome defines that REQ. The
//!   commitments mark the target `superseded` once the superseding line is
//!   accepted (`knowledge::commitments`).
//! - **shared questions** — any intake file may cite a stakeholder question
//!   as `Q:<NAME>/Q-<n>` ([`super::questions`]); the file and question must
//!   exist, and readiness refuses while one cited is still open.
//!
//! [`lint`] adds these to an intake file's structural issues (so `review`
//! refuses a broken reference); [`readiness`] re-checks the accepted
//! snapshots a handover would pin, and [`pin_questions`] records what the
//! handover relied on.

use super::lint::{self, Issue, Severity};
use super::questions::{self, Pin};
use super::{record, review, Intake};
use anyhow::{bail, Result};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `<INTAKE>/REQ-nnn`, a requirement of another intake.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    pub intake: String,
    pub requirement: String,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.intake, self.requirement)
    }
}

fn supersedes_clause() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)\(\s*supersedes\s+([^()]*)\)\s*\.?\s*$").expect("static regex")
    })
}

fn target_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^([A-Za-z0-9][A-Za-z0-9._-]*)/(REQ-\d{3,})$").expect("static regex")
    })
}

/// The targets a line's trailing `(supersedes …)` names: empty without
/// one, `Err` naming what cannot be read.
pub fn supersedes(line: &str) -> std::result::Result<Vec<Target>, String> {
    let Some(c) = supersedes_clause().captures(line.trim_end()) else {
        return Ok(Vec::new());
    };
    c[1].split(',')
        .map(str::trim)
        .map(|t| {
            target_re()
                .captures(t)
                .map(|m| Target {
                    intake: m[1].to_string(),
                    requirement: m[2].to_string(),
                })
                .ok_or_else(|| {
                    format!("cannot read supersedes target {t:?}: write <INTAKE>/REQ-nnn")
                })
        })
        .collect()
}

/// Each requirement defined in an outcome text with what its line
/// supersedes (HTML comments ignored).
pub fn requirement_supersedes(
    outcome: &str,
) -> Vec<(String, std::result::Result<Vec<Target>, String>)> {
    lint::strip_comments(outcome)
        .lines()
        .filter_map(|l| {
            let id = lint::defined_requirements(l).pop()?;
            Some((id, supersedes(l)))
        })
        .collect()
}

/// The project root of an intake opened at `<root>/.zforge/intakes/<id>`.
pub fn project_root(intake: &Intake) -> Option<PathBuf> {
    intake
        .dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// Requirements defined in `id`'s accepted 01-outcome, or why there are
/// none to check against.
fn accepted_requirements(root: &Path, id: &str) -> std::result::Result<BTreeSet<String>, String> {
    let other = Intake::open(root, id).map_err(|_| format!("intake {id} does not exist"))?;
    let status = review::file_status(&other, lint::OUTCOME)
        .map_err(|e| format!("cannot read intake {id}: {e}"))?;
    let accepted = status
        .accepted
        .ok_or_else(|| format!("intake {id} has no accepted 01-outcome.md"))?;
    let text = record::read_snapshot(&other, lint::OUTCOME, accepted.revision)
        .map_err(|e| format!("cannot read intake {id}'s accepted 01-outcome.md: {e}"))?;
    Ok(lint::defined_requirements(&text).into_iter().collect())
}

/// Problems with the supersedes clauses of an outcome text.
fn supersedes_problems(root: &Path, own: &str, outcome: &str) -> Vec<String> {
    let mut cache: BTreeMap<String, std::result::Result<BTreeSet<String>, String>> =
        BTreeMap::new();
    let mut out = Vec::new();
    for (req, targets) in requirement_supersedes(outcome) {
        let targets = match targets {
            Ok(t) => t,
            Err(e) => {
                out.push(format!("{req}: {e}"));
                continue;
            }
        };
        for t in targets {
            if t.intake == own {
                out.push(format!(
                    "{req} supersedes {t}, a requirement of this intake: change it in place"
                ));
                continue;
            }
            let defined = cache
                .entry(t.intake.clone())
                .or_insert_with(|| accepted_requirements(root, &t.intake));
            match defined {
                Err(e) => out.push(format!("{req} supersedes {t}, but {e}")),
                Ok(reqs) if !reqs.contains(&t.requirement) => out.push(format!(
                    "{req} supersedes {t}, which {}'s accepted 01-outcome.md does not define",
                    t.intake
                )),
                Ok(_) => {}
            }
        }
    }
    out
}

/// Problems with the question citations of a text: missing file or
/// question (`references`), and questions still open (`open`).
fn question_problems(
    root: &Path,
    text: &str,
    files: &mut BTreeMap<String, Option<questions::File>>,
) -> (Vec<String>, Vec<String>) {
    let (mut references, mut open) = (Vec::new(), Vec::new());
    for (name, id) in questions::citations(text) {
        match questions::check_citation(files, root, &name, &id) {
            Err(e) => references.push(e),
            Ok(false) => {
                let q = files[&name]
                    .as_ref()
                    .and_then(|f| f.parsed.get(&id))
                    .map(|q| q.question.clone())
                    .unwrap_or_default();
                open.push(format!("cites Q:{name}/{id}, still open: {q}"))
            }
            Ok(true) => {}
        }
    }
    (references, open)
}

fn error(file: &str, message: String) -> Issue {
    Issue {
        file: file.to_string(),
        severity: Severity::Error,
        message,
    }
}

/// Cross-intake issues of one intake file: errors for a supersedes target
/// that does not resolve and for a citation of a missing questions file
/// or question. An open question is not a lint issue — readiness blocks
/// on it, and its answer arrives from outside.
pub fn lint(intake: &Intake, rel: &str, text: &str) -> Vec<Issue> {
    let Some(root) = project_root(intake) else {
        return Vec::new();
    };
    let mut issues = Vec::new();
    if rel == lint::OUTCOME {
        issues.extend(
            supersedes_problems(&root, &intake.id, text)
                .into_iter()
                .map(|m| error(rel, m)),
        );
    }
    let (references, _) = question_problems(&root, text, &mut BTreeMap::new());
    issues.extend(references.into_iter().map(|m| error(rel, m)));
    issues
}

/// Readiness on the accepted texts a handover would pin, as `(file,
/// text)`: unresolved references (supersedes targets, question citations)
/// and cited questions still open, each prefixed with its file.
pub fn readiness<'a>(
    intake: &Intake,
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> (Vec<String>, Vec<String>) {
    let Some(root) = project_root(intake) else {
        return (Vec::new(), Vec::new());
    };
    let mut cache = BTreeMap::new();
    let (mut references, mut open) = (Vec::new(), Vec::new());
    for (file, text) in files {
        if file == lint::OUTCOME {
            references.extend(
                supersedes_problems(&root, &intake.id, text)
                    .into_iter()
                    .map(|m| format!("{file}: {m}")),
            );
        }
        let (r, o) = question_problems(&root, text, &mut cache);
        references.extend(r.into_iter().map(|m| format!("{file}: {m}")));
        open.extend(o.into_iter().map(|m| format!("{file}: {m}")));
    }
    (references, open)
}

/// The questions files the handed-over texts cite, each with its hash as
/// read now — refused if, in that same read, a cited question is missing
/// or open, so the pinned hash is of answers the contract can rely on.
pub fn pin_questions<'a>(
    intake: &Intake,
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Vec<Pin>> {
    let Some(root) = project_root(intake) else {
        return Ok(Vec::new());
    };
    let mut cited: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (_, text) in files {
        for (name, id) in questions::citations(text) {
            cited.entry(name).or_default().insert(id);
        }
    }
    let mut pins = Vec::new();
    for (name, ids) in cited {
        let Some(f) = questions::read(&root, &name)? else {
            bail!(".zforge/questions/{name}.md does not exist");
        };
        for id in &ids {
            match f.parsed.get(id) {
                None => bail!("Q:{name}/{id} is not defined"),
                Some(q) if !q.answered => bail!("Q:{name}/{id} is still open"),
                Some(_) => {}
            }
        }
        pins.push(Pin {
            name,
            sha256: f.sha256,
            cited: ids.into_iter().collect(),
        });
    }
    Ok(pins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_supersedes_clauses() {
        assert_eq!(supersedes("- REQ-001: a thing").unwrap(), []);
        assert_eq!(
            supersedes("- REQ-001: a thing (supersedes PHC-1/REQ-001, REF.Q/REQ-012)").unwrap(),
            [
                Target {
                    intake: "PHC-1".into(),
                    requirement: "REQ-001".into()
                },
                Target {
                    intake: "REF.Q".into(),
                    requirement: "REQ-012".into()
                }
            ]
        );
        assert!(
            supersedes("- REQ-001: x (Supersedes A/REQ-002).")
                .unwrap()
                .len()
                == 1
        );
        assert!(supersedes("- REQ-001: x (supersedes the old flow)").is_err());
        assert!(supersedes("- REQ-001: x (supersedes A/REQ-1)").is_err());
        assert_eq!(
            supersedes("- REQ-001: x (supersedes A/REQ-002) and more").unwrap(),
            [],
            "only a trailing clause counts"
        );
    }

    fn accept(i: &Intake, file: &str, text: &str) {
        std::fs::write(i.dir.join(file), text).unwrap();
        review::review(i, file).unwrap();
        review::accept(i, file, None).unwrap();
    }

    fn errors(issues: &[Issue]) -> Vec<String> {
        issues.iter().map(|i| i.message.clone()).collect()
    }

    #[test]
    fn lint_resolves_supersedes_against_accepted_outcomes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let old = review::create(root, "OLD").unwrap();
        accept(
            &old,
            lint::OUTCOME,
            "# OLD\n\n## Requirements\n\n- REQ-001: old rule\n\n## Open questions\n",
        );
        // Drafted but never accepted: nothing to supersede yet.
        let draft = review::create(root, "DRAFT").unwrap();
        std::fs::write(draft.dir.join(lint::OUTCOME), "- REQ-001: x\n").unwrap();
        let new = review::create(root, "NEW").unwrap();

        let ok = "- REQ-001: new rule (supersedes OLD/REQ-001)\n";
        assert_eq!(errors(&lint(&new, lint::OUTCOME, ok)), Vec::<String>::new());

        let bad = "- REQ-001: a (supersedes OLD/REQ-009)\n\
                   - REQ-002: b (supersedes GONE/REQ-001)\n\
                   - REQ-003: c (supersedes DRAFT/REQ-001)\n\
                   - REQ-004: d (supersedes NEW/REQ-001)\n\
                   - REQ-005: e (supersedes the old one)\n";
        let e = errors(&lint(&new, lint::OUTCOME, bad));
        assert_eq!(e.len(), 5, "{e:?}");
        assert!(e[0].contains("OLD/REQ-009, which OLD's accepted 01-outcome.md does not define"));
        assert!(e[1].contains("intake GONE does not exist"));
        assert!(e[2].contains("DRAFT has no accepted 01-outcome.md"));
        assert!(e[3].contains("a requirement of this intake"));
        assert!(e[4].contains("cannot read supersedes target"));
        // Only 01-outcome supersedes.
        assert!(lint(&new, "02-behavior.md", bad).is_empty());
    }

    #[test]
    fn lint_and_readiness_check_question_citations() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let i = review::create(root, "F").unwrap();
        let text = "Rule per Q:REF/Q-1.\n";
        let e = errors(&lint(&i, "02-behavior.md", text));
        assert!(
            e[0].contains(".zforge/questions/REF.md does not exist"),
            "{e:?}"
        );

        let path = questions::create(root, "REF").unwrap();
        std::fs::write(&path, "- [ ] Q-1: archived?\n").unwrap();
        assert!(lint(&i, "02-behavior.md", text).is_empty());
        let e = errors(&lint(&i, "02-behavior.md", "Q:REF/Q-2\n"));
        assert!(e[0].contains("does not define"), "{e:?}");

        let (refs, open) = readiness(&i, [("02-behavior.md", text)]);
        assert!(refs.is_empty());
        assert_eq!(
            open,
            ["02-behavior.md: cites Q:REF/Q-1, still open: archived?"]
        );
        assert!(pin_questions(&i, [("02-behavior.md", text)]).is_err());

        std::fs::write(&path, "- [x] Q-1: archived? => no\n").unwrap();
        let (refs, open) = readiness(&i, [("02-behavior.md", text)]);
        assert!(refs.is_empty() && open.is_empty());
        let pins = pin_questions(&i, [("02-behavior.md", text)]).unwrap();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].name, "REF");
        assert_eq!(pins[0].cited, ["Q-1"]);
        assert_eq!(
            pins[0].sha256,
            crate::intake::hash::sha256("- [x] Q-1: archived? => no\n")
        );
    }
}
