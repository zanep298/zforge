//! Readiness before handover (workflow §6.2).
//!
//! Every check runs on the *accepted* revisions — what would be handed over
//! — never on the working files. Machine checks confirm structure,
//! references and observable conditions; they do not show the requirements
//! are complete or the design right, which is what the user's review of
//! each file is for.

use super::items;
use super::lint::{self, Known, Severity, TaskMeta};
use super::record;
use super::review;
use super::status::DocState;
use super::{Intake, STAGES};
use crate::config::Config;
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
    config: &Config,
) -> Result<Readiness> {
    let execution = &config.execution;
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
        ..Known::default()
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
    let integration = lint::section(&secs, lint::INTEGRATION)
        .is_some_and(|lines| lines.iter().any(|l| !l.trim().is_empty()));
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

    b.check(
        "test command",
        match config.project.test_command.trim().is_empty() {
            false => Vec::new(),
            true => vec![
                "no test command: set `project.test_command`; a run cannot verify without one"
                    .into(),
            ],
        },
    );

    // D6: a file accepted before something it rests on changed may rest
    // on the old version; the user confirms it again before it is handed
    // over. Only the items it rests on count (amended 12/10/2026).
    b.check(
        "up to date with upstream",
        accepted_before_upstream(intake, &pinned, &metas, &texts)?
            .into_iter()
            .map(|(file, why)| {
                format!(
                    "{why}; review it again (`zforge intake review {} {file}`) and accept it if it still holds",
                    intake.id
                )
            })
            .collect(),
    );

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

/// Files resting on something that changed since they were accepted
/// (D6, amended 12/10/2026), each with why: the upstream item, the file
/// it is in and its newest accepted revision.
///
/// What a file rests on: a stage, the stages before it; a task, every
/// stage and the tasks it depends on — but only the items of a stage it
/// rests on (`items::view`): a later stage the requirements, rows and
/// decisions it names, and the list of requirements; a task its
/// requirements, their behavior rows, the decisions it cites and its row
/// of the breakdown. The scope constraints bind everything; other text
/// binds nothing. The upstream revision compared is the one in force when
/// the file was accepted (the last accepted before it, in the decision
/// log) against the newest accepted one. A stage that cannot be read as
/// items is compared whole; a task depended on, always whole.
fn accepted_before_upstream(
    intake: &Intake,
    pinned: &[Pinned],
    metas: &BTreeMap<String, TaskMeta>,
    texts: &BTreeMap<String, String>,
) -> Result<Vec<(String, String)>> {
    let log = record::read(intake)?;
    let accepted = |d: &record::Decision| d.decision == record::DecisionKind::Accepted;
    let by_file: BTreeMap<&str, &Pinned> = pinned.iter().map(|p| (p.file.as_str(), p)).collect();
    let mut out = Vec::new();
    for p in pinned {
        let Some(mine) = log
            .iter()
            .rposition(|d| d.file == p.file && d.revision == p.revision && accepted(d))
        else {
            continue;
        };
        let Some(text) = texts.get(&p.file) else {
            continue;
        };
        // The revision of `up` in force when `p` was accepted.
        let in_force = |up: &str| {
            log[..mine]
                .iter()
                .rev()
                .find(|d| d.file == up && accepted(d))
                .map(|d| d.revision)
        };
        let task = task_id(&p.file);
        let scope = match &task {
            Some(t) => items::Scope::task(t, text),
            None => items::Scope::stage(text),
        };
        let stages: Vec<&str> = match &task {
            Some(_) => STAGES.to_vec(),
            None => STAGES
                .iter()
                .copied()
                .take_while(|s| *s != p.file)
                .collect(),
        };
        let deps: Vec<String> = task
            .as_ref()
            .and_then(|t| metas.get(t))
            .map(|m| {
                m.depends_on
                    .iter()
                    .map(|d| format!("tasks/{d}.md"))
                    .collect()
            })
            .unwrap_or_default();
        for up in stages.iter().map(|s| s.to_string()).chain(deps) {
            let (Some(u), Some(new)) = (by_file.get(up.as_str()), texts.get(&up)) else {
                continue;
            };
            let old_rev = in_force(&up);
            if old_rev == Some(u.revision) {
                continue;
            }
            let Some(old_rev) = old_rev else {
                out.push((
                    p.file.clone(),
                    format!(
                        "{} was accepted before {} had an accepted revision (now revision {})",
                        p.file, up, u.revision
                    ),
                ));
                continue;
            };
            let old = record::read_snapshot(intake, &up, old_rev)?;
            if up.starts_with("tasks/") {
                if old != *new {
                    out.push((
                        p.file.clone(),
                        format!(
                            "{} rests on {up} (depends_on), changed in revision {}",
                            p.file, u.revision
                        ),
                    ));
                }
                continue;
            }
            let changed = items::changed(
                &items::view(&up, &old, &scope),
                &items::view(&up, new, &scope),
            );
            if changed.iter().any(|k| k == items::WHOLE) {
                out.push((
                    p.file.clone(),
                    format!(
                        "{} rests on {up}, changed in revision {} (not readable as items, so compared whole)",
                        p.file, u.revision
                    ),
                ));
            } else if !changed.is_empty() {
                out.push((
                    p.file.clone(),
                    format!(
                        "{} rests on {} of {up}, changed in revision {}",
                        p.file,
                        items::describe(&changed),
                        u.revision
                    ),
                ));
            }
        }
    }
    Ok(out)
}

/// Accepted files of the intake resting on something that changed since
/// they were accepted, with why ([`accepted_before_upstream`]). A file
/// here may be reviewed again even though it did not change
/// (`review::review`).
pub fn stale_reasons(intake: &Intake) -> Result<BTreeMap<String, Vec<String>>> {
    let mut pinned = Vec::new();
    let mut metas = BTreeMap::new();
    let mut texts = BTreeMap::new();
    for s in review::statuses(intake)? {
        let Some(a) = &s.accepted else { continue };
        let text = record::read_snapshot(intake, &s.file, a.revision)?;
        if let Some(t) = task_id(&s.file) {
            if let Ok(m) = lint::parse_task_meta(&text) {
                metas.insert(t, m);
            }
        }
        texts.insert(s.file.clone(), text);
        pinned.push(Pinned {
            file: s.file.clone(),
            revision: a.revision,
            sha256: a.sha256.clone(),
        });
    }
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (file, why) in accepted_before_upstream(intake, &pinned, &metas, &texts)? {
        out.entry(file).or_default().push(why);
    }
    Ok(out)
}

/// The files of [`stale_reasons`].
pub fn stale(intake: &Intake) -> Result<BTreeSet<String>> {
    Ok(stale_reasons(intake)?.into_keys().collect())
}

impl Readiness {
    /// Add a check made outside [`check`] — [`runtime`] — to the report.
    pub fn with(mut self, c: Check) -> Self {
        self.ready &= c.ok;
        self.checks.push(c);
        self
    }

    /// Add the project's own readiness (ONBOARD REQ-010): its knowledge
    /// onboarded, no stale citation, baseline green or its failures known.
    /// A gap warns by default and does not affect [`Readiness::ready`];
    /// `onboarding.required: true` turns every gap into an ordinary
    /// failing check instead, so it blocks a handover like any other.
    ///
    /// Knowledge too large for `knowledge.prompt_limit` is always only a
    /// warning ([`crate::knowledge::fit_warning`]): every task still gets
    /// the part most relevant to it.
    pub fn with_project(self, config: &Config) -> Self {
        let gaps = crate::onboard::readiness_gaps(config);
        let mut r = if config.onboarding.required {
            let ok = gaps.is_empty();
            self.with(Check {
                name: "project onboarding",
                ok,
                problems: gaps,
            })
        } else {
            let mut r = self;
            r.warnings.extend(gaps);
            r
        };
        r.warnings.extend(crate::knowledge::fit_warning(config));
        r
    }
}

/// The runner a run needs, as this machine has it: registered for the
/// project, and its command found. Kept apart from [`check`], which reads
/// only the intake and the config, because it depends on the machine.
pub fn runtime(project_root: &Path) -> Check {
    let runner = crate::run::execute::RUNNER;
    let problems = match crate::registry::io::load() {
        Err(e) => vec![format!("cannot read the agent registry: {e:#}")],
        Ok(reg) => match reg.resolved_agent(runner, project_root) {
            None => vec![format!(
                "runner `{runner}` is not in the agent registry; run `zforge init --agent {runner}`"
            )],
            Some(spec) if which::which(&spec.command).is_err() => vec![format!(
                "runner `{runner}` runs `{}`, which is not installed or not on PATH",
                spec.command
            )],
            Some(_) => Vec::new(),
        },
    };
    Check {
        name: "runner",
        ok: problems.is_empty(),
        problems,
    }
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

    fn init_repo(dir: &Path) {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
    }

    fn commit_all(dir: &Path) {
        for args in [
            vec!["add", "-A"],
            vec!["commit", "-q", "--allow-empty", "-m", "c"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(dir)
                .output()
                .unwrap();
        }
    }

    fn config_at(root: &Path, onboarding_required: bool) -> Config {
        let extra = if onboarding_required {
            "onboarding:\n  required: true\n"
        } else {
            ""
        };
        let yaml = format!(
            "project:\n  name: p\n  language: shell\n  test_command: \"true\"\n\
             execution:\n  budget_usd: 1.0\n{extra}"
        );
        let mut cfg: Config = serde_yaml::from_str(&yaml).unwrap();
        cfg.config_file = root.join(".zforge").join("config.yaml");
        cfg
    }

    /// A minimal intake, accepted top to bottom, with one task serving its
    /// one requirement — just enough for `check` to be ready on its own.
    fn accept_intake(root: &Path) -> Intake {
        let i = crate::intake::review::create(root, "F").unwrap();
        std::fs::write(
            i.dir.join("01-outcome.md"),
            "# F — Outcome\n\n## Requirements\n\n- REQ-001: does a thing\n\n## Open questions\n",
        )
        .unwrap();
        std::fs::write(
            i.dir.join("02-behavior.md"),
            "# F — Behavior\n\n## Situations\n\nREQ-001: it happens.\n\n## Open questions\n",
        )
        .unwrap();
        std::fs::write(
            i.dir.join("03-solution.md"),
            "# F — Solution\n\n## Flow\n\nDoes the thing.\n\n## Open questions\n",
        )
        .unwrap();
        std::fs::write(
            i.dir.join("04-breakdown.md"),
            "# F — Breakdown\n\n## Tasks\n\nTASK-001.\n\n\
             ## Integration verification\n\nsh test.sh\n\n## Open questions\n",
        )
        .unwrap();
        std::fs::write(
            i.dir.join("tasks/TASK-001.md"),
            "---\nid: TASK-001\nparent: F\nrequirements: [REQ-001]\ndepends_on: []\n---\n\n\
             # TASK-001\n\n## Goal\nDo it.\n\n## Input\nx.\n\n## Output\ny.\n\n\
             ## Constraints\nz.\n\n## Autonomy\nw.\n\n\
             ## Acceptance and verification\n- AC-01: ok\n\n## Delivery\nLocal.\n\n\
             ## Amend the contract when\nNever.\n\n## Open questions\n",
        )
        .unwrap();
        for f in i.files() {
            crate::intake::review::review(&i, &f).unwrap();
            crate::intake::review::accept(&i, &f, None).unwrap();
        }
        i
    }

    /// AC-03: a project that never onboarded still hands over — the gap
    /// is a warning, and readiness stays ready.
    #[test]
    fn with_project_warns_by_default_when_not_onboarded() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        commit_all(root);
        let cfg = config_at(root, false);
        let i = accept_intake(root);

        let r = check(&i, root, &[], &cfg).unwrap().with_project(&cfg);
        assert!(r.ready, "{:?}", r.checks);
        assert!(
            r.warnings.iter().any(|w| w.contains("domain.md")),
            "{:?}",
            r.warnings
        );
    }

    /// AC-04: `onboarding.required: true` turns the same gap into a
    /// failing check, so readiness — and a handover — refuses.
    #[test]
    fn with_project_blocks_when_onboarding_required() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        commit_all(root);
        let cfg = config_at(root, true);
        let i = accept_intake(root);

        let r = check(&i, root, &[], &cfg).unwrap().with_project(&cfg);
        assert!(!r.ready, "{:?}", r.checks);
        let project = r
            .checks
            .iter()
            .find(|c| c.name == "project onboarding")
            .unwrap();
        assert!(!project.ok, "{:?}", project.problems);
        assert!(!project.problems.is_empty());
    }

    /// Accepted conventions and rules over `knowledge.prompt_limit` are a
    /// warning naming both sizes — never a failing check, even when
    /// onboarding is required.
    #[test]
    fn with_project_warns_when_conventions_and_rules_exceed_the_prompt_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        std::fs::create_dir_all(root.join("a/handler")).unwrap();
        std::fs::write(root.join("a/handler/h.go"), "package handler\n").unwrap();
        commit_all(root);
        let sha = crate::run::git::head(root).unwrap();
        let mut cfg = config_at(root, true);
        let k = crate::knowledge::Knowledge::open(&cfg);
        std::fs::create_dir_all(&k.dir).unwrap();
        std::fs::write(
            k.dir.join("conventions.md"),
            format!(
                "---\npinned: {sha}\n---\n## Structure\n\
                 - CONV-001: handlers live in handler/. (a/handler/h.go:1)\n\n\
                 ## Open questions\n"
            ),
        )
        .unwrap();
        crate::intake::review::review(&k, "conventions.md").unwrap();
        crate::intake::review::accept(&k, "conventions.md", None).unwrap();
        let i = accept_intake(root);

        let fits = check(&i, root, &[], &cfg).unwrap().with_project(&cfg);
        assert!(
            !fits.warnings.iter().any(|w| w.contains("prompt_limit")),
            "{:?}",
            fits.warnings
        );

        cfg.knowledge.prompt_limit = 10;
        let r = check(&i, root, &[], &cfg).unwrap().with_project(&cfg);
        let w = r
            .warnings
            .iter()
            .find(|w| w.contains("knowledge.prompt_limit"))
            .unwrap_or_else(|| panic!("{:?}", r.warnings));
        assert!(w.contains("(10)") && w.contains("bytes"), "{w}");
        let project = r
            .checks
            .iter()
            .find(|c| c.name == "project onboarding")
            .unwrap();
        assert!(
            !project.problems.iter().any(|p| p.contains("prompt_limit")),
            "{:?}",
            project.problems
        );
    }

    // ─── D6 item by item (amended 12/10/2026) ───────────────────────────

    const OUTCOME_ITEMS: &str = "# F — Outcome\n\n## Summary\nAdd and sub.\n\n## Requirements\n\n\
        - REQ-001: add returns the sum\n  - negative numbers too\n\
        - REQ-002: sub returns the difference\n\n## Must not change\n\n- the CLI flags\n\n\
        ## Out of scope\n\n- floats\n\n## Open questions\n";
    const BEHAVIOR_ITEMS: &str = "# F — Behavior\n\n## Situations\n\n| # | Kind | When | Then | REQ |\n\
        |---|---|---|---|---|\n| 1 | normal | add 2 3 | 5 | REQ-001 |\n| 2 | normal | sub 3 2 | 1 | REQ-002 |\n\n\
        ## Open questions\n";
    const SOLUTION_ITEMS: &str = "# F — Solution\n\n## Flow\n\nShell arithmetic.\n\n## Binding decisions\n\n\
        | # | Decision | Why | Rejected alternative |\n|---|---|---|---|\n\
        | D-1 | `$(( ))` arithmetic | REQ-001: no dependency | bc |\n| D-2 | one file lib.sh | simple | two files |\n\n\
        ## Open questions\n";
    const BREAKDOWN_ITEMS: &str = "# F — Breakdown\n\n## Tasks and dependencies\n\n\
        | Task | Serves | Depends on | Output |\n|---|---|---|---|\n\
        | TASK-001 Add | REQ-001 | — | add |\n| TASK-002 Sub | REQ-002 | TASK-001 | sub |\n\n\
        ## Integration verification\n\nsh test.sh\n\n## Open questions\n";

    fn task_items(id: &str, req: &str, deps: &str, cites: &str) -> String {
        format!(
            "---\nid: {id}\nparent: F\nrequirements: [{req}]\ndepends_on: [{deps}]\n---\n\n\
             # {id}\n\n## Goal\nDo it.\n\n## Input\nx.\n\n## Output\ny.\n\n\
             ## Constraints\n{cites}\n\n## Autonomy\nw.\n\n\
             ## Acceptance and verification\n- AC-01: ok\n\n## Delivery\nLocal.\n\n\
             ## Amend the contract when\nNever.\n\n## Open questions\n"
        )
    }

    /// Stages written as items; TASK-001 serves REQ-001 and keeps D-1,
    /// TASK-002 serves REQ-002 and builds on TASK-001.
    fn items_intake(root: &Path) -> Intake {
        let i = crate::intake::review::create(root, "F").unwrap();
        for (f, t) in [
            ("01-outcome.md", OUTCOME_ITEMS.to_string()),
            ("02-behavior.md", BEHAVIOR_ITEMS.to_string()),
            ("03-solution.md", SOLUTION_ITEMS.to_string()),
            ("04-breakdown.md", BREAKDOWN_ITEMS.to_string()),
            (
                "tasks/TASK-001.md",
                task_items("TASK-001", "REQ-001", "", "Keeps D-1."),
            ),
            (
                "tasks/TASK-002.md",
                task_items("TASK-002", "REQ-002", "TASK-001", "None."),
            ),
        ] {
            std::fs::write(i.dir.join(f), t).unwrap();
        }
        for f in i.files() {
            crate::intake::review::review(&i, &f).unwrap();
            crate::intake::review::accept(&i, &f, None).unwrap();
        }
        assert!(stale(&i).unwrap().is_empty());
        i
    }

    fn change(i: &Intake, file: &str, from: &str, to: &str) {
        let path = i.dir.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
        crate::intake::review::review(i, file).unwrap();
        crate::intake::review::accept(i, file, None).unwrap();
    }

    fn stale_files(i: &Intake) -> Vec<String> {
        stale(i).unwrap().into_iter().collect()
    }

    #[test]
    fn a_changed_requirement_stales_only_what_rests_on_it() {
        let tmp = tempfile::tempdir().unwrap();
        let i = items_intake(tmp.path());
        change(
            &i,
            "01-outcome.md",
            "the difference",
            "the difference, never negative",
        );
        // 03-solution names only REQ-001; TASK-001 serves REQ-001.
        assert_eq!(
            stale_files(&i),
            ["02-behavior.md", "04-breakdown.md", "tasks/TASK-002.md"]
        );
        let why = stale_reasons(&i).unwrap();
        assert_eq!(
            why["tasks/TASK-002.md"],
            ["tasks/TASK-002.md rests on REQ-002 of 01-outcome.md, changed in revision 2"]
        );
    }

    #[test]
    fn prose_binds_nothing_and_the_scope_binds_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let i = items_intake(tmp.path());
        change(&i, "01-outcome.md", "Add and sub.", "Add and subtract.");
        change(
            &i,
            "03-solution.md",
            "Shell arithmetic.",
            "Plain shell arithmetic.",
        );
        assert!(stale_files(&i).is_empty());

        change(&i, "01-outcome.md", "- floats", "- floats\n- big numbers");
        assert_eq!(stale_files(&i).len(), 5, "{:?}", stale_files(&i));
        let why = stale_reasons(&i).unwrap();
        assert!(
            why["tasks/TASK-001.md"][0].contains("rests on \"Out of scope\" of 01-outcome.md"),
            "{why:?}"
        );
    }

    #[test]
    fn a_new_requirement_stales_the_stages_not_the_tasks() {
        let tmp = tempfile::tempdir().unwrap();
        let i = items_intake(tmp.path());
        change(
            &i,
            "01-outcome.md",
            "- REQ-002:",
            "- REQ-003: mul returns the product\n- REQ-002:",
        );
        assert_eq!(
            stale_files(&i),
            ["02-behavior.md", "03-solution.md", "04-breakdown.md"]
        );
    }

    #[test]
    fn decisions_rows_and_dependencies_stale_who_cites_them() {
        let tmp = tempfile::tempdir().unwrap();
        let i = items_intake(tmp.path());
        // D-1 is cited by TASK-001 only; the breakdown names REQ-001.
        change(&i, "03-solution.md", "| bc |", "| bc, awk |");
        assert_eq!(stale_files(&i), ["04-breakdown.md", "tasks/TASK-001.md"]);
        for f in stale_files(&i) {
            crate::intake::review::review(&i, &f).unwrap();
            crate::intake::review::accept(&i, &f, None).unwrap();
        }
        assert!(stale_files(&i).is_empty());

        // A row of the breakdown: only its task.
        change(&i, "04-breakdown.md", "| sub |", "| `sub` |");
        assert_eq!(stale_files(&i), ["tasks/TASK-002.md"]);
        crate::intake::review::review(&i, "tasks/TASK-002.md").unwrap();
        crate::intake::review::accept(&i, "tasks/TASK-002.md", None).unwrap();

        // A task depended on, whole.
        change(&i, "tasks/TASK-001.md", "Do it.", "Do it well.");
        assert_eq!(stale_files(&i), ["tasks/TASK-002.md"]);
        assert_eq!(
            stale_reasons(&i).unwrap()["tasks/TASK-002.md"],
            ["tasks/TASK-002.md rests on tasks/TASK-001.md (depends_on), changed in revision 3"]
        );
    }

    #[test]
    fn a_stage_not_readable_as_items_is_compared_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let i = items_intake(tmp.path());
        let path = i.dir.join("02-behavior.md");
        std::fs::write(
            &path,
            "# F — Behavior\n\n## Situations\n\nREQ-001: add 2 3 = 5.\n\n## Open questions\n",
        )
        .unwrap();
        crate::intake::review::review(&i, "02-behavior.md").unwrap();
        crate::intake::review::accept(&i, "02-behavior.md", None).unwrap();
        assert_eq!(
            stale_files(&i),
            [
                "03-solution.md",
                "04-breakdown.md",
                "tasks/TASK-001.md",
                "tasks/TASK-002.md"
            ]
        );
        assert!(stale_reasons(&i).unwrap()["03-solution.md"][0].contains("not readable as items"));
    }
}
