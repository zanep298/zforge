//! Structural checks for intake Markdown (D2). They confirm what a machine
//! can — IDs, references, required sections, task frontmatter — and say
//! nothing about whether the content is right; that is the review's job.
//!
//! Conventions:
//! - a requirement is defined in `01-outcome.md` as a list item or heading
//!   starting with its ID: `- REQ-001: …`;
//! - an acceptance criterion is defined in a task's "Acceptance và kiểm
//!   chứng" section: `- AC-01: …`;
//! - an open question is an unchecked item `- [ ]` under "Câu hỏi còn mở";
//! - HTML comments are guidance and are ignored.

use regex::Regex;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::OnceLock;

pub const OUTCOME: &str = "01-outcome.md";
pub const BREAKDOWN: &str = "04-breakdown.md";
pub const OPEN_QUESTIONS: &str = "Câu hỏi còn mở";
pub const INTEGRATION: &str = "Kiểm chứng tích hợp";
pub const ACCEPTANCE: &str = "Acceptance và kiểm chứng";
pub const CHANGES_PREFIX: &str = "changes/";

/// Sections a change request carries (workflow §8).
pub const CHANGE_SECTIONS: [&str; 5] = [
    "Hợp đồng đang áp dụng",
    "Bằng chứng",
    "Đề xuất",
    "Tác động",
    "Cần quyết định",
];

/// Sections every leaf task contract has (workflow §5.6, example §11).
pub const TASK_SECTIONS: [&str; 8] = [
    "Mục tiêu",
    "Input",
    "Output",
    "Ràng buộc",
    "Tự chủ",
    ACCEPTANCE,
    "Bàn giao",
    "Cần amendment khi",
];

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Issue {
    pub file: String,
    pub severity: Severity,
    pub message: String,
}

impl Issue {
    fn error(file: &str, message: impl Into<String>) -> Self {
        Self {
            file: file.to_string(),
            severity: Severity::Error,
            message: message.into(),
        }
    }
    fn warning(file: &str, message: impl Into<String>) -> Self {
        Self {
            file: file.to_string(),
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

/// What other files define, for checking references.
#[derive(Debug, Default, Clone)]
pub struct Known {
    pub requirements: BTreeSet<String>,
    pub tasks: BTreeSet<String>,
}

/// Leaf task frontmatter.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TaskMeta {
    pub id: String,
    pub parent: String,
    pub requirements: Vec<String>,
    pub depends_on: Vec<String>,
    /// Protected test files the contract lets the task change (globs).
    pub tests_may_change: Vec<String>,
}

fn re(pattern: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static regex"))
}

fn req_ref() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"\bREQ-\d{3,}\b", &R)
}

fn req_def() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"^\s*(?:#{1,6}\s+|[-*]\s+)(?:\*\*)?(REQ-\d{3,})\b", &R)
}

fn ac_def() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"^\s*[-*]\s+(?:\*\*)?(AC-\d{2,})\b", &R)
}

fn task_ref() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"\bTASK-\d{3,}\b", &R)
}

/// Text with HTML comments removed.
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// (frontmatter, body) of a Markdown file.
pub fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let normalized = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"));
    let Some(rest) = normalized else {
        return (None, text);
    };
    match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            let body = after.split_once('\n').map_or("", |(_, b)| b);
            (Some(&rest[..end]), body)
        }
        None => (None, text),
    }
}

/// Body lines of each `## ` section, by title.
pub fn sections(body: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in body.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            out.push((title.trim().to_string(), Vec::new()));
        } else if let Some((_, lines)) = out.last_mut() {
            lines.push(line.to_string());
        }
    }
    out
}

fn section<'a>(secs: &'a [(String, Vec<String>)], title: &str) -> Option<&'a [String]> {
    secs.iter()
        .find(|(t, _)| t == title)
        .map(|(_, l)| l.as_slice())
}

fn has_content(lines: &[String]) -> bool {
    lines.iter().any(|l| !l.trim().is_empty())
}

/// Requirements defined in `01-outcome.md` text, in order (duplicates kept).
pub fn defined_requirements(outcome: &str) -> Vec<String> {
    strip_comments(outcome)
        .lines()
        .filter_map(|l| req_def().captures(l).map(|c| c[1].to_string()))
        .collect()
}

/// Unchecked `- [ ]` items under "Câu hỏi còn mở".
pub fn open_questions(text: &str) -> Vec<String> {
    let (_, body) = split_frontmatter(text);
    let clean = strip_comments(body);
    let secs = sections(&clean);
    section(&secs, OPEN_QUESTIONS)
        .unwrap_or_default()
        .iter()
        .filter_map(|l| {
            let t = l.trim_start();
            t.strip_prefix("- [ ]")
                .or_else(|| t.strip_prefix("* [ ]"))
                .map(|q| q.trim().to_string())
        })
        .collect()
}

/// Acceptance criteria defined in a task, in order.
pub fn acceptance_criteria(task_text: &str) -> Vec<String> {
    let (_, body) = split_frontmatter(task_text);
    let clean = strip_comments(body);
    let secs = sections(&clean);
    section(&secs, ACCEPTANCE)
        .unwrap_or_default()
        .iter()
        .filter_map(|l| ac_def().captures(l).map(|c| c[1].to_string()))
        .collect()
}

pub fn parse_task_meta(text: &str) -> Result<TaskMeta, String> {
    let (fm, _) = split_frontmatter(text);
    let fm = fm.ok_or("missing frontmatter (id, parent, requirements, depends_on)")?;
    let v: serde_yaml::Value =
        serde_yaml::from_str(fm).map_err(|e| format!("frontmatter is not valid YAML: {e}"))?;
    let string = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let list = |k: &str| -> Result<Vec<String>, String> {
        match v.get(k) {
            None | Some(serde_yaml::Value::Null) => Ok(Vec::new()),
            Some(serde_yaml::Value::Sequence(items)) => items
                .iter()
                .map(|i| {
                    i.as_str()
                        .map(str::to_string)
                        .ok_or(format!("`{k}` must list strings"))
                })
                .collect(),
            Some(_) => Err(format!("`{k}` must be a list")),
        }
    };
    Ok(TaskMeta {
        id: string("id"),
        parent: string("parent"),
        requirements: list("requirements")?,
        depends_on: list("depends_on")?,
        tests_may_change: list("tests_may_change")?,
    })
}

/// Structural issues in one file. `rel` is its intake-relative path.
pub fn lint(rel: &str, text: &str, intake_id: &str, known: &Known) -> Vec<Issue> {
    let mut issues = Vec::new();
    let (_, body) = split_frontmatter(text);
    let clean = strip_comments(body);

    if clean
        .lines()
        .all(|l| l.trim().is_empty() || l.starts_with('#'))
    {
        issues.push(Issue::error(rel, "has no content yet (only headings)"));
    }

    if rel == OUTCOME {
        let defs = defined_requirements(text);
        if defs.is_empty() {
            issues.push(Issue::error(rel, "defines no requirement (`- REQ-001: …`)"));
        }
        let mut seen = BTreeSet::new();
        for d in &defs {
            if !seen.insert(d) {
                issues.push(Issue::error(rel, format!("{d} is defined more than once")));
            }
        }
    } else {
        let unknown: BTreeSet<&str> = req_ref()
            .find_iter(&clean)
            .map(|m| m.as_str())
            .filter(|r| !known.requirements.contains(*r))
            .collect();
        for r in unknown {
            issues.push(Issue::error(
                rel,
                format!("refers to {r}, which 01-outcome.md does not define"),
            ));
        }
    }

    if rel == BREAKDOWN {
        let secs = sections(&clean);
        if !section(&secs, INTEGRATION).is_some_and(has_content) {
            issues.push(Issue::warning(
                rel,
                format!("section \"{INTEGRATION}\" is empty: say how the whole is verified"),
            ));
        }
        let missing: BTreeSet<&str> = task_ref()
            .find_iter(&clean)
            .map(|m| m.as_str())
            .filter(|t| !known.tasks.contains(*t))
            .collect();
        for t in missing {
            issues.push(Issue::warning(
                rel,
                format!("mentions {t}, which has no file in tasks/"),
            ));
        }
    }

    if rel.starts_with(CHANGES_PREFIX) {
        let secs = sections(&clean);
        for title in CHANGE_SECTIONS {
            match section(&secs, title) {
                None => issues.push(Issue::error(rel, format!("missing section \"{title}\""))),
                Some(lines) if !has_content(lines) => {
                    issues.push(Issue::error(rel, format!("section \"{title}\" is empty")))
                }
                Some(_) => {}
            }
        }
        return issues;
    }

    if let Some(stem) = rel
        .strip_prefix("tasks/")
        .and_then(|n| n.strip_suffix(".md"))
    {
        lint_task(rel, stem, text, &clean, intake_id, known, &mut issues);
    }

    if !sections(&clean).iter().any(|(t, _)| t == OPEN_QUESTIONS) {
        issues.push(Issue::warning(
            rel,
            format!("has no \"{OPEN_QUESTIONS}\" section"),
        ));
    }
    issues
}

fn lint_task(
    rel: &str,
    stem: &str,
    text: &str,
    clean_body: &str,
    intake_id: &str,
    known: &Known,
    issues: &mut Vec<Issue>,
) {
    match parse_task_meta(text) {
        Err(e) => issues.push(Issue::error(rel, e)),
        Ok(meta) => {
            if meta.id != stem {
                issues.push(Issue::error(
                    rel,
                    format!("frontmatter id {:?} does not match the file name", meta.id),
                ));
            }
            if meta.parent != intake_id {
                issues.push(Issue::error(
                    rel,
                    format!("frontmatter parent {:?} is not {intake_id}", meta.parent),
                ));
            }
            if meta.requirements.is_empty() {
                issues.push(Issue::error(
                    rel,
                    "links to no requirement: a task must serve an accepted REQ",
                ));
            }
            for r in meta
                .requirements
                .iter()
                .filter(|r| !known.requirements.contains(*r))
            {
                issues.push(Issue::error(
                    rel,
                    format!("requirement {r} is not defined in 01-outcome.md"),
                ));
            }
            for d in &meta.depends_on {
                if d == stem {
                    issues.push(Issue::error(rel, "depends on itself"));
                } else if !known.tasks.contains(d) {
                    issues.push(Issue::error(
                        rel,
                        format!("depends on {d}, which has no file in tasks/"),
                    ));
                }
            }
        }
    }
    let secs = sections(clean_body);
    for title in TASK_SECTIONS {
        match section(&secs, title) {
            None => issues.push(Issue::error(rel, format!("missing section \"{title}\""))),
            Some(lines) if !has_content(lines) => {
                issues.push(Issue::error(rel, format!("section \"{title}\" is empty")))
            }
            Some(_) => {}
        }
    }
    let acs = acceptance_criteria(text);
    if acs.is_empty() && section(&secs, ACCEPTANCE).is_some() {
        issues.push(Issue::error(
            rel,
            "defines no acceptance criterion (`- AC-01: …`)",
        ));
    }
    let mut seen = BTreeSet::new();
    for ac in &acs {
        if !seen.insert(ac) {
            issues.push(Issue::error(rel, format!("{ac} is defined more than once")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTCOME_OK: &str = "# F — Outcome\n\n## Yêu cầu\n\n- REQ-001: lọc theo trạng thái\n- **REQ-002**: giữ thứ tự\n\n## Câu hỏi còn mở\n\n- [x] đã xong\n- [ ] còn phân quyền?\n";

    const TASK_OK: &str = "---\nid: TASK-001\nparent: F\nrequirements: [REQ-001]\ndepends_on: []\n---\n\n# TASK-001 — Lọc\n\n## Mục tiêu\nLọc.\n## Input\nAPI.\n## Output\nDanh sách.\n## Ràng buộc\nGiữ shape.\n## Tự chủ\nTự chọn hàm.\n## Acceptance và kiểm chứng\n- AC-01: open\n- AC-02: done\n## Bàn giao\nLocal.\n## Cần amendment khi\nĐổi shape.\n## Câu hỏi còn mở\n";

    fn known() -> Known {
        Known {
            requirements: ["REQ-001", "REQ-002"].map(String::from).into(),
            tasks: ["TASK-001"].map(String::from).into(),
        }
    }

    fn errors(issues: &[Issue]) -> Vec<&str> {
        issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.message.as_str())
            .collect()
    }

    #[test]
    fn reads_requirements_questions_and_criteria() {
        assert_eq!(defined_requirements(OUTCOME_OK), ["REQ-001", "REQ-002"]);
        assert_eq!(open_questions(OUTCOME_OK), ["còn phân quyền?"]);
        assert_eq!(acceptance_criteria(TASK_OK), ["AC-01", "AC-02"]);
        assert_eq!(
            parse_task_meta(TASK_OK).unwrap(),
            TaskMeta {
                id: "TASK-001".into(),
                parent: "F".into(),
                requirements: vec!["REQ-001".into()],
                depends_on: vec![],
                tests_may_change: vec![],
            }
        );
    }

    #[test]
    fn guidance_in_comments_is_not_content() {
        let t = crate::intake::templates::stage(OUTCOME, "F").unwrap();
        assert!(
            defined_requirements(&t).is_empty(),
            "the example REQ is in a comment"
        );
        let e = lint(OUTCOME, &t, "F", &Known::default());
        assert!(
            errors(&e).contains(&"has no content yet (only headings)"),
            "{e:?}"
        );
        assert!(errors(&e)
            .iter()
            .any(|m| m.starts_with("defines no requirement")));
    }

    #[test]
    fn a_complete_task_and_outcome_are_clean() {
        assert!(errors(&lint("tasks/TASK-001.md", TASK_OK, "F", &known())).is_empty());
        assert!(errors(&lint(OUTCOME, OUTCOME_OK, "F", &known())).is_empty());
    }

    #[test]
    fn task_contract_problems_are_errors() {
        let bad = TASK_OK
            .replace("id: TASK-001", "id: TASK-009")
            .replace("parent: F", "parent: G")
            .replace("[REQ-001]", "[REQ-777]")
            .replace("depends_on: []", "depends_on: [TASK-002]")
            .replace("## Tự chủ\nTự chọn hàm.\n", "")
            .replace("- AC-02: done", "- AC-01: again");
        let e = lint("tasks/TASK-001.md", &bad, "F", &known());
        let m = errors(&e);
        for want in [
            "frontmatter id \"TASK-009\" does not match the file name",
            "frontmatter parent \"G\" is not F",
            "requirement REQ-777 is not defined in 01-outcome.md",
            "depends on TASK-002, which has no file in tasks/",
            "missing section \"Tự chủ\"",
            "AC-01 is defined more than once",
        ] {
            assert!(m.contains(&want), "missing {want:?} in {m:?}");
        }
    }

    /// MOC-B TASK-006 AC-02.
    #[test]
    fn change_requests_need_the_sections_of_section_8() {
        let template = crate::intake::templates::change("F", "CHANGE-RUN-001");
        let e = lint("changes/CHANGE-RUN-001.md", &template, "F", &known());
        let m = errors(&e);
        assert!(m.contains(&"section \"Bằng chứng\" is empty"), "{m:?}");
        assert_eq!(
            m.len(),
            CHANGE_SECTIONS.len() + 1,
            "every section, plus the empty file: {m:?}"
        );

        let filled = CHANGE_SECTIONS
            .iter()
            .map(|t| format!("## {t}\nnội dung\n"))
            .collect::<String>();
        assert!(errors(&lint("changes/CHANGE-RUN-001.md", &filled, "F", &known())).is_empty());

        let missing = filled.replace("## Tác động\nnội dung\n", "");
        let issues = lint("changes/CHANGE-RUN-001.md", &missing, "F", &known());
        assert_eq!(errors(&issues), ["missing section \"Tác động\""]);
    }

    #[test]
    fn stage_files_are_checked_against_known_ids() {
        let e = lint(
            OUTCOME,
            &format!("{OUTCOME_OK}- REQ-001: again\n"),
            "F",
            &known(),
        );
        assert!(errors(&e).contains(&"REQ-001 is defined more than once"));

        let behavior = "# B\n\n## Tình huống\nREQ-003 khi lọc\n\n## Câu hỏi còn mở\n";
        let e = lint("02-behavior.md", behavior, "F", &known());
        assert!(errors(&e).contains(&"refers to REQ-003, which 01-outcome.md does not define"));

        let breakdown = "# B\n\n## Task và dependency\nTASK-001, TASK-002\n\n## Kiểm chứng tích hợp\n\n## Câu hỏi còn mở\n";
        let e = lint(BREAKDOWN, breakdown, "F", &known());
        let warnings: Vec<&str> = e.iter().map(|i| i.message.as_str()).collect();
        assert!(warnings
            .iter()
            .any(|m| m.starts_with("section \"Kiểm chứng tích hợp\" is empty")));
        assert!(warnings.contains(&"mentions TASK-002, which has no file in tasks/"));
    }
}
