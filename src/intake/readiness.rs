//! Readiness before handover (workflow §6.2).
//!
//! Every check runs on the *accepted* revisions — what would be handed over
//! — never on the working files. Machine checks confirm structure,
//! references and observable conditions; they do not show the requirements
//! are complete or the design right, which is what the user's review of
//! each file is for.

use super::lint::{self, Known, Severity, TaskMeta};
use super::record;
use super::review;
use super::status::DocState;
use super::{Intake, STAGES};
use crate::config::ExecutionConfig;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One accepted revision, pinned by hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pinned {
    pub file: String,
    pub revision: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Readiness {
    pub intake: String,
    /// Tasks in the handover scope, in dependency order.
    pub tasks: Vec<String>,
    /// The scope is every task of the intake.
    pub whole: bool,
    pub checks: Vec<Check>,
    pub warnings: Vec<String>,
    /// Accepted revisions the handover would pin: stages, then tasks.
    pub files: Vec<Pinned>,
    pub ready: bool,
}

struct Builder {
    checks: Vec<Check>,
    warnings: Vec<String>,
}

impl Builder {
    fn check(&mut self, name: &'static str, problems: Vec<String>) {
        self.checks.push(Check {
            name,
            ok: problems.is_empty(),
            problems,
        });
    }
}

/// Check the intake for handing over `requested` tasks (all when empty).
pub fn check(
    intake: &Intake,
    project_root: &Path,
    requested: &[String],
    execution: &ExecutionConfig,
) -> Result<Readiness> {
    let statuses = review::statuses(intake)?;
    let all_tasks: Vec<String> = statuses.iter().filter_map(|s| task_id(&s.file)).collect();
    for t in requested {
        if !all_tasks.contains(t) {
            bail!("task {t} has no file tasks/{t}.md in intake {}", intake.id);
        }
    }
    let whole = requested.is_empty();
    let scope: BTreeSet<String> = if whole {
        all_tasks.iter().cloned().collect()
    } else {
        requested.iter().cloned().collect()
    };
    let mut b = Builder {
        checks: Vec::new(),
        warnings: Vec::new(),
    };

    // Accepted, and nothing unreviewed left in the files handed over.
    let mut pinned = Vec::new();
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let mut problems = Vec::new();
    let in_handover =
        |file: &str| STAGES.contains(&file) || task_id(file).is_some_and(|t| scope.contains(&t));
    for s in &statuses {
        if let Some(a) = &s.accepted {
            texts.insert(
                s.file.clone(),
                record::read_snapshot(intake, &s.file, a.revision)?,
            );
        }
        if !in_handover(&s.file) {
            continue;
        }
        match (&s.accepted, s.state) {
            (None, _) => problems.push(format!(
                "{} has no accepted revision ({})",
                s.file,
                s.state.as_str()
            )),
            (Some(a), DocState::Accepted) if !s.has_draft => pinned.push(Pinned {
                file: s.file.clone(),
                revision: a.revision,
                sha256: a.sha256.clone(),
            }),
            (Some(a), _) => problems.push(format!(
                "{} has changes after accepted revision {} ({}); accept or discard them first",
                s.file,
                a.revision,
                if s.has_draft {
                    "draft"
                } else {
                    s.state.as_str()
                }
            )),
        }
    }
    for stage in STAGES {
        if !statuses.iter().any(|s| s.file == stage) {
            problems.push(format!("{stage} is missing"));
        }
    }
    if scope.is_empty() {
        problems.push("there are no leaf tasks to hand over".into());
    }
    b.check("accepted revisions", problems);

    // Structure of the accepted texts, against the accepted IDs.
    let outcome = texts.get(lint::OUTCOME).cloned().unwrap_or_default();
    let metas: BTreeMap<String, TaskMeta> = texts
        .iter()
        .filter_map(|(f, t)| Some((task_id(f)?, lint::parse_task_meta(t).ok()?)))
        .collect();
    let known = Known {
        requirements: lint::defined_requirements(&outcome).into_iter().collect(),
        tasks: metas.keys().cloned().collect(),
    };
    let mut problems = Vec::new();
    for p in &pinned {
        for issue in lint::lint(&p.file, &texts[&p.file], &intake.id, &known) {
            match issue.severity {
                Severity::Error => problems.push(format!("{}: {}", issue.file, issue.message)),
                Severity::Warning => b
                    .warnings
                    .push(format!("{}: {}", issue.file, issue.message)),
            }
        }
    }
    b.check("structure", problems);

    // No important question left open.
    let problems = pinned
        .iter()
        .flat_map(|p| {
            lint::open_questions(&texts[&p.file])
                .into_iter()
                .map(move |q| format!("{}: {q}", p.file))
        })
        .collect();
    b.check("open questions", problems);

    // Every requirement has a task answering for it.
    let covered: BTreeSet<&String> = metas.values().flat_map(|m| &m.requirements).collect();
    let uncovered: Vec<String> = known
        .requirements
        .iter()
        .filter(|r| !covered.contains(r))
        .map(|r| format!("{r} has no task"))
        .collect();
    if whole {
        b.check("requirement coverage", uncovered);
    } else {
        b.warnings.extend(uncovered);
        b.check("requirement coverage", Vec::new());
    }

    // Dependencies inside the scope, without cycles.
    let (order, problems) = dependency_order(&scope, &metas);
    b.check("dependencies", problems);

    let secs = lint::sections(&lint::strip_comments(
        texts.get(lint::BREAKDOWN).map(String::as_str).unwrap_or(""),
    ));
    let integration = secs
        .iter()
        .find(|(t, _)| t == lint::INTEGRATION)
        .is_some_and(|(_, lines)| lines.iter().any(|l| !l.trim().is_empty()));
    b.check(
        "integration verification",
        if integration {
            Vec::new()
        } else {
            vec![format!(
                "04-breakdown.md: section \"{}\" is empty",
                lint::INTEGRATION
            )]
        },
    );

    b.check("git work tree", git_problems(project_root));
    b.check(
        "execution budget",
        match execution.budget_usd {
            Some(usd) if usd > 0.0 => Vec::new(),
            _ => vec!["no budget: set `execution.budget_usd` in .zforge/config.yaml".into()],
        },
    );

    b.warnings
        .extend(accepted_before_upstream(intake, &pinned, &metas)?);

    let ready = b.checks.iter().all(|c| c.ok);
    Ok(Readiness {
        intake: intake.id.clone(),
        tasks: order,
        whole,
        checks: b.checks,
        warnings: b.warnings,
        files: pinned,
        ready,
    })
}

/// Files accepted before a file they build on got a newer accepted
/// revision: the later stages and tasks may rest on the old contract.
/// Stages build on the stages before them; a task on every stage and on
/// the tasks it depends on.
fn accepted_before_upstream(
    intake: &Intake,
    pinned: &[Pinned],
    metas: &BTreeMap<String, TaskMeta>,
) -> Result<Vec<String>> {
    let log = record::read(intake)?;
    let accepted_at = |p: &Pinned| {
        log.iter()
            .rev()
            .find(|d| {
                d.file == p.file
                    && d.revision == p.revision
                    && d.decision == record::DecisionKind::Accepted
            })
            .map(|d| d.at)
    };
    let by_file: BTreeMap<&str, &Pinned> = pinned.iter().map(|p| (p.file.as_str(), p)).collect();
    let mut warnings = Vec::new();
    for p in pinned {
        let upstream: Vec<String> = match task_id(&p.file) {
            Some(t) => STAGES
                .iter()
                .map(|s| s.to_string())
                .chain(
                    metas
                        .get(&t)
                        .map(|m| m.depends_on.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|d| format!("tasks/{d}.md")),
                )
                .collect(),
            None => STAGES
                .iter()
                .take_while(|s| **s != p.file)
                .map(|s| s.to_string())
                .collect(),
        };
        let Some(mine) = accepted_at(p) else { continue };
        for up in upstream {
            let Some(u) = by_file.get(up.as_str()) else {
                continue;
            };
            if accepted_at(u).is_some_and(|t| t > mine) {
                warnings.push(format!(
                    "{} was accepted before {} revision {}; check it still holds",
                    p.file, u.file, u.revision
                ));
            }
        }
    }
    Ok(warnings)
}

fn task_id(file: &str) -> Option<String> {
    file.strip_prefix("tasks/")?
        .strip_suffix(".md")
        .map(String::from)
}

/// Scope in dependency order, and the problems: a dependency outside the
/// scope (its output would not exist yet) or a cycle.
fn dependency_order(
    scope: &BTreeSet<String>,
    metas: &BTreeMap<String, TaskMeta>,
) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    for t in scope {
        for d in metas
            .get(t)
            .map(|m| m.depends_on.as_slice())
            .unwrap_or_default()
        {
            if !scope.contains(d) {
                problems.push(format!(
                    "{t} depends on {d}, which is outside the handover scope"
                ));
            }
        }
    }
    // Kahn's algorithm over the scope; ties broken by ID for a stable order.
    let deps = |t: &String| -> Vec<String> {
        metas
            .get(t)
            .map(|m| {
                m.depends_on
                    .iter()
                    .filter(|d| scope.contains(*d))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut done: Vec<String> = Vec::new();
    let mut left: BTreeSet<String> = scope.clone();
    while let Some(next) = left
        .iter()
        .find(|t| deps(t).iter().all(|d| done.contains(d)))
        .cloned()
    {
        left.remove(&next);
        done.push(next);
    }
    if !left.is_empty() {
        let cycle: Vec<&str> = left.iter().map(String::as_str).collect();
        problems.push(format!("dependency cycle among {}", cycle.join(", ")));
    }
    (done, problems)
}

/// D3: runs need a git work tree with a commit to branch from.
fn git_problems(root: &Path) -> Vec<String> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()
            .filter(|o| o.status.success())
    };
    match git(&["rev-parse", "--is-inside-work-tree"]) {
        None => vec!["the project is not in a git work tree (runs use a worktree each)".into()],
        Some(_) if git(&["rev-parse", "--verify", "-q", "HEAD"]).is_none() => {
            vec!["the repository has no commit to use as the baseline".into()]
        }
        Some(_) => Vec::new(),
    }
}

/// Readable view for `readiness.md`. Generated; not a record of anything.
pub fn render(r: &Readiness) -> String {
    let mut out = format!(
        "# Readiness — {}\n\n<!-- Generated by `zforge readiness`; editing it changes nothing. -->\n\n**{}**\n\n",
        r.intake,
        if r.ready { "Ready to hand over" } else { "Not ready" }
    );
    let scope = if r.whole {
        "all tasks"
    } else {
        "selected tasks"
    };
    out.push_str(&format!(
        "Scope ({scope}), in dependency order: {}\n\n",
        r.tasks.join(" → ")
    ));
    out.push_str("## Checks\n\n");
    for c in &r.checks {
        out.push_str(&format!(
            "- [{}] {}\n",
            if c.ok { "x" } else { " " },
            c.name
        ));
        for p in &c.problems {
            out.push_str(&format!("  - {p}\n"));
        }
    }
    if !r.warnings.is_empty() {
        out.push_str("\n## Warnings\n\n");
        for w in &r.warnings {
            out.push_str(&format!("- {w}\n"));
        }
    }
    out.push_str("\n## Accepted revisions\n\n");
    for f in &r.files {
        out.push_str(&format!(
            "- {} rev {} `{}`\n",
            f.file,
            f.revision,
            super::hash::short(&f.sha256)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(deps: &[&str]) -> TaskMeta {
        TaskMeta {
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
            ..TaskMeta::default()
        }
    }

    #[test]
    fn orders_by_dependency_and_finds_cycles_and_missing_scope() {
        let scope: BTreeSet<String> = ["T1", "T2", "T3"].map(String::from).into();
        let metas: BTreeMap<String, TaskMeta> = [
            ("T1".to_string(), meta(&["T3"])),
            ("T2".to_string(), meta(&[])),
            ("T3".to_string(), meta(&["T2"])),
        ]
        .into();
        let (order, problems) = dependency_order(&scope, &metas);
        assert_eq!(order, ["T2", "T3", "T1"]);
        assert!(problems.is_empty());

        let cyclic: BTreeMap<String, TaskMeta> = [
            ("T1".to_string(), meta(&["T2"])),
            ("T2".to_string(), meta(&["T1"])),
        ]
        .into();
        let scope2: BTreeSet<String> = ["T1", "T2"].map(String::from).into();
        let (_, problems) = dependency_order(&scope2, &cyclic);
        assert_eq!(problems, ["dependency cycle among T1, T2"]);

        let only: BTreeSet<String> = ["T1"].map(String::from).into();
        let (_, problems) = dependency_order(&only, &metas);
        assert_eq!(
            problems,
            ["T1 depends on T3, which is outside the handover scope"]
        );
    }
}
