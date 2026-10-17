//! Shared stakeholder questions: `.zforge/questions/<NAME>.md`.
//!
//! One release often spans several intakes, and the questions for the
//! people outside the team (a product owner, design, legal) are shared by
//! them. Such a file is *not* a reviewed file: the answers come from
//! outside, and anyone records them as they arrive. Each question is one
//! checkbox line —
//!
//! ```text
//! - [ ] Q-1: question
//! - [x] Q-2: question => answer
//! ```
//!
//! — and an intake file cites it as `Q:<NAME>/Q-<n>`. Lint refuses a
//! citation of a missing file or question; readiness refuses while a cited
//! question is open; a handover pins the hash of every questions file its
//! contract cites (`handover::Manifest::questions`).

use super::{hash, intakes_dir, lint, validate_id, Intake};
use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `<project>/.zforge/questions`.
pub fn dir(project_root: &Path) -> PathBuf {
    project_root.join(".zforge").join("questions")
}

/// `<project>/.zforge/questions/<name>.md`, after checking the name.
pub fn path(project_root: &Path, name: &str) -> Result<PathBuf> {
    validate_id(name).map_err(|_| {
        anyhow::anyhow!("invalid questions name {name:?}: use letters, digits, '.', '_' or '-'")
    })?;
    Ok(dir(project_root).join(format!("{name}.md")))
}

/// One question of a questions file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Question {
    /// `Q-1`.
    pub id: String,
    pub question: String,
    pub answered: bool,
    /// What follows `=>` on an answered line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// A parsed questions file.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Parsed {
    pub questions: Vec<Question>,
    /// Lines that look like a question but cannot be read, duplicate IDs,
    /// a ticked question without an answer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

impl Parsed {
    pub fn get(&self, id: &str) -> Option<&Question> {
        self.questions.iter().find(|q| q.id == id)
    }
}

fn question_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^\s*[-*]\s+\[([ xX])\]\s+(?:\*\*)?(Q-\d+)(?:\*\*)?\s*:\s*(.*)$")
            .expect("static regex")
    })
}

fn citation() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\bQ:([A-Za-z0-9][A-Za-z0-9._-]*)/(Q-\d+)\b").expect("static regex")
    })
}

/// Read a questions file's text. HTML comments are guidance and ignored.
pub fn parse(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut seen = BTreeSet::new();
    for line in lint::strip_comments(text).lines() {
        let t = line.trim_start();
        let Some(c) = question_line().captures(line) else {
            if (t.starts_with("- [") || t.starts_with("* [")) && t.contains("Q-") {
                out.problems.push(format!(
                    "cannot read {:?}: write `- [ ] Q-<n>: question` or `- [x] Q-<n>: question => answer`",
                    t.trim_end()
                ));
            }
            continue;
        };
        let id = c[2].to_string();
        let answered = &c[1] != " ";
        let rest = c[3].trim();
        let (question, answer) = match rest.split_once("=>") {
            Some((q, a)) => (
                q.trim(),
                Some(a.trim().to_string()).filter(|a| !a.is_empty()),
            ),
            None => (rest, None),
        };
        if !seen.insert(id.clone()) {
            out.problems.push(format!("{id} is defined more than once"));
            continue;
        }
        if answered && answer.is_none() {
            out.problems
                .push(format!("{id} is ticked but has no `=> answer`"));
        }
        out.questions.push(Question {
            id,
            question: question.to_string(),
            answered,
            answer,
        });
    }
    out
}

/// `(NAME, Q-n)` cited in `text` as `Q:<NAME>/Q-<n>`, outside HTML
/// comments, in order of first appearance.
pub fn citations(text: &str) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    citation()
        .captures_iter(&lint::strip_comments(text))
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .filter(|k| seen.insert(k.clone()))
        .collect()
}

/// Create `.zforge/questions/<name>.md` from the template.
pub fn create(project_root: &Path, name: &str) -> Result<PathBuf> {
    let path = path(project_root, name)?;
    if path.exists() {
        bail!("questions file {} already exists", path.display());
    }
    std::fs::create_dir_all(dir(project_root))?;
    crate::fs::write_atomic(&path, super::templates::questions(name).as_bytes())?;
    Ok(path)
}

/// A questions file as it is on disk now: its text, hash and questions.
#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub text: String,
    pub sha256: String,
    pub parsed: Parsed,
}

/// Read `<name>`'s questions file; `None` when it does not exist.
pub fn read(project_root: &Path, name: &str) -> Result<Option<File>> {
    let path = path(project_root, name)?;
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    Ok(Some(File {
        name: name.to_string(),
        sha256: hash::sha256(&text),
        parsed: parse(&text),
        path,
        text,
    }))
}

/// Names of every questions file, sorted.
pub fn names(project_root: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir(project_root))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let stem = n.strip_suffix(".md")?.to_string();
            validate_id(&stem).ok().map(|_| stem)
        })
        .collect();
    out.sort();
    out
}

/// Problems with one citation against what is on disk now: the file or
/// the question is missing (`Err`), or the question is open (`Ok(false)`).
pub fn check_citation(
    files: &mut BTreeMap<String, Option<File>>,
    project_root: &Path,
    name: &str,
    id: &str,
) -> std::result::Result<bool, String> {
    if !files.contains_key(name) {
        let f = read(project_root, name).unwrap_or(None);
        files.insert(name.to_string(), f);
    }
    match &files[name] {
        None => Err(format!(
            "cites Q:{name}/{id}, but .zforge/questions/{name}.md does not exist (`zforge questions new {name}`)"
        )),
        Some(f) => match f.parsed.get(id) {
            None => Err(format!(
                "cites Q:{name}/{id}, which .zforge/questions/{name}.md does not define"
            )),
            Some(q) => Ok(q.answered),
        },
    }
}

/// Where a question is cited: an intake and one of its files.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CitedBy {
    pub intake: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuestionStatus {
    #[serde(flatten)]
    pub question: Question,
    pub cited_by: Vec<CitedBy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileStatus {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub open: usize,
    pub answered: usize,
    pub questions: Vec<QuestionStatus>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    /// Citations of a question this file does not define.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub undefined_citations: Vec<String>,
}

/// Every citation in the working files of every intake: `(NAME, Q-n)` →
/// where.
pub fn all_citations(project_root: &Path) -> BTreeMap<(String, String), Vec<CitedBy>> {
    let mut out: BTreeMap<(String, String), Vec<CitedBy>> = BTreeMap::new();
    let mut ids: Vec<String> = std::fs::read_dir(intakes_dir(project_root))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    for id in ids {
        let Ok(intake) = Intake::open(project_root, &id) else {
            continue;
        };
        for f in intake.files() {
            let text = std::fs::read_to_string(intake.dir.join(&f)).unwrap_or_default();
            for key in citations(&text) {
                out.entry(key).or_default().push(CitedBy {
                    intake: id.clone(),
                    file: f.clone(),
                });
            }
        }
    }
    out
}

/// Status of `name` (or every questions file): each question open or
/// answered, and which intake files cite it.
pub fn status(project_root: &Path, name: Option<&str>) -> Result<Vec<FileStatus>> {
    let names = match name {
        Some(n) => {
            if read(project_root, n)?.is_none() {
                bail!("no questions file {n}; create it with `zforge questions new {n}`");
            }
            vec![n.to_string()]
        }
        None => names(project_root),
    };
    let cited = all_citations(project_root);
    let mut out = Vec::new();
    for n in names {
        let Some(f) = read(project_root, &n)? else {
            continue;
        };
        let questions: Vec<QuestionStatus> = f
            .parsed
            .questions
            .iter()
            .map(|q| QuestionStatus {
                question: q.clone(),
                cited_by: cited
                    .get(&(n.clone(), q.id.clone()))
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect();
        let mut undefined_citations = Vec::new();
        for ((name, id), by) in &cited {
            if *name == n && f.parsed.get(id).is_none() {
                for b in by {
                    undefined_citations.push(format!("{}/{} cites Q:{n}/{id}", b.intake, b.file));
                }
            }
        }
        out.push(FileStatus {
            open: questions.iter().filter(|q| !q.question.answered).count(),
            answered: questions.iter().filter(|q| q.question.answered).count(),
            name: n,
            path: f.path,
            sha256: f.sha256,
            questions,
            problems: f.parsed.problems,
            undefined_citations,
        });
    }
    Ok(out)
}

/// Plain-text view of [`status`].
pub fn render(files: &[FileStatus]) -> String {
    if files.is_empty() {
        return "No questions files. Create one with `zforge questions new <NAME>`.\n".into();
    }
    let mut out = String::new();
    for f in files {
        out.push_str(&format!(
            "{}  ({})  {} open, {} answered\n",
            f.name,
            f.path.display(),
            f.open,
            f.answered
        ));
        for q in &f.questions {
            let mark = if q.question.answered { "x" } else { " " };
            let answer = q
                .question
                .answer
                .as_deref()
                .map(|a| format!(" => {a}"))
                .unwrap_or_default();
            let cited = if q.cited_by.is_empty() {
                "not cited".to_string()
            } else {
                format!(
                    "cited by {}",
                    q.cited_by
                        .iter()
                        .map(|c| format!("{}/{}", c.intake, c.file))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            out.push_str(&format!(
                "  [{mark}] {}: {}{answer}  — {cited}\n",
                q.question.id, q.question.question
            ));
        }
        for p in f.problems.iter().chain(&f.undefined_citations) {
            out.push_str(&format!("  ! {p}\n"));
        }
    }
    out
}

/// The questions file a handover relied on: its hash at handover time and
/// the questions the handed-over files cite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pin {
    pub name: String,
    pub sha256: String,
    pub cited: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_and_answered_questions() {
        let text = "# Q\n\n<!-- - [ ] Q-9: example -->\n## Questions\n\n\
                    - [ ] Q-1: archived counts?\n\
                    - [x] Q-2: who sees it? => only the referrer\n\
                    * [X] **Q-3**: ticked without answer\n\
                    - [ ] Q-1: again\n\
                    - [ ] Q-x: bad\n";
        let p = parse(text);
        let ids: Vec<&str> = p.questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(ids, ["Q-1", "Q-2", "Q-3"]);
        assert!(!p.questions[0].answered);
        assert_eq!(p.questions[0].question, "archived counts?");
        assert!(p.questions[1].answered);
        assert_eq!(p.questions[1].answer.as_deref(), Some("only the referrer"));
        assert_eq!(p.questions[1].question, "who sees it?");
        assert!(p.get("Q-9").is_none(), "comments are guidance");
        assert_eq!(p.problems.len(), 3, "{:?}", p.problems);
        assert!(p.problems.iter().any(|m| m.contains("Q-3 is ticked")));
        assert!(p.problems.iter().any(|m| m.contains("Q-1 is defined more")));
        assert!(p.problems.iter().any(|m| m.contains("cannot read")));
    }

    #[test]
    fn finds_citations_outside_comments() {
        let text = "Per Q:REF-V1/Q-2 and Q:REF-V1/Q-10, again Q:REF-V1/Q-2.\n\
                    <!-- Q:OTHER/Q-1 -->\nnot Q:/Q-1 nor Q:A/Q-\n";
        assert_eq!(
            citations(text),
            [
                ("REF-V1".to_string(), "Q-2".to_string()),
                ("REF-V1".to_string(), "Q-10".to_string())
            ]
        );
    }

    #[test]
    fn create_status_and_citations() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(create(root, "../x").is_err());
        let p = create(root, "REF").unwrap();
        assert!(create(root, "REF").is_err(), "never overwritten");
        let template = parse(&std::fs::read_to_string(&p).unwrap());
        assert!(template.questions.is_empty() && template.problems.is_empty());
        std::fs::write(&p, "## Questions\n- [ ] Q-1: a?\n- [x] Q-2: b? => yes\n").unwrap();
        let i = crate::intake::review::create(root, "F").unwrap();
        std::fs::write(
            i.dir.join("01-outcome.md"),
            "- REQ-001: x, see Q:REF/Q-1 and Q:REF/Q-7\n",
        )
        .unwrap();
        let s = status(root, None).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].open, s[0].answered), (1, 1));
        assert_eq!(
            s[0].questions[0].cited_by,
            [CitedBy {
                intake: "F".into(),
                file: "01-outcome.md".into()
            }]
        );
        assert!(s[0].questions[1].cited_by.is_empty());
        assert_eq!(
            s[0].undefined_citations,
            ["F/01-outcome.md cites Q:REF/Q-7"]
        );
        assert!(render(&s).contains("[ ] Q-1: a?  — cited by F/01-outcome.md"));
        assert!(status(root, Some("NOPE")).is_err());
    }
}
