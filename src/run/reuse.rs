//! Reusing a task's output across handovers (MOC-C TASK-006, REQ-006).
//!
//! After an amendment the user hands over again. A task whose contract did
//! not change need not be paid for twice: its verified output from an
//! earlier handover of the same intake stands, provided nothing it was built
//! from differs. "Nothing" is exact — there is no "close enough":
//!
//! - its [`contract_key`] is equal: the baseline commit, the task file and
//!   the task file of every dependency, direct or not, pinned with the same
//!   SHA-256, and the exact text of every stage item the task rests on
//!   (D6 as amended 12/10/2026: its requirements, their behavior rows, the
//!   decisions it cites, its breakdown row, the scope constraints; a stage
//!   that cannot be read as items, whole); and
//! - it started from exactly the outputs its dependencies have now in the new
//!   handover (so a dependency that ran again, even to the same contract,
//!   means running this task again).
//!
//! A reuse is recorded as a run of its own whose single event, `reused`,
//! names the run it reuses — so the new handover's state, knowledge and
//! integration all come from records as usual. Integration runs are never
//! reused: they call no agent and cost nothing to repeat.

use super::contract::{self, Handover};
use super::record::{self, Run, RunEvent, RunKind, RunMeta, RunState, RunStatus};
use super::{start, worktree};
use crate::intake::{items, STAGES};
use anyhow::Result;
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The definition of [`contract_key`]: a key made under another one
/// never equals a key made under this one.
const KEY_VERSION: &str = "contract-key v2: items";

/// Everything `task`'s work in handover `h` depended on, as one hash: the
/// baseline commit, the task file and those of every task it depends on,
/// directly or not, and of each stage only what the task rests on
/// ([`rests_on`]) — so an edit to a requirement another task serves leaves
/// this key, and this task's reuse, alone.
pub fn contract_key(h: &Handover, task: &str) -> Result<String> {
    let pinned = |file: &str| -> Result<String> {
        h.files
            .iter()
            .find(|f| f.file == file)
            .map(|f| f.sha256.clone())
            .ok_or_else(|| anyhow::anyhow!("{} does not pin {file}", h.manifest.id))
    };
    let mut lines = vec![
        KEY_VERSION.to_string(),
        format!("baseline {}", h.manifest.baseline.commit),
    ];
    for (stage, view) in rests_on(h, task)? {
        let digest = crate::intake::hash::sha256(&items::digest(&view));
        lines.push(format!("stage {stage} {digest}"));
    }
    let own = format!("tasks/{task}.md");
    lines.push(format!("task {own} {}", pinned(&own)?));
    for dep in transitive_deps(h, task)? {
        let file = format!("tasks/{dep}.md");
        lines.push(format!("dep {file} {}", pinned(&file)?));
    }
    Ok(crate::intake::hash::sha256(&lines.join("\n")))
}

/// What `task` rests on in each pinned stage (`intake::items::view`): its
/// requirements, their behavior rows, the decisions it cites, its row of
/// the breakdown and the scope constraints — or a whole stage that cannot
/// be read as items.
pub fn rests_on(h: &Handover, task: &str) -> Result<Vec<(&'static str, BTreeMap<String, String>)>> {
    rests_on_files(&h.files, &h.manifest.id, task)
}

/// [`rests_on`] over the files handover `id` pins.
pub fn rests_on_files(
    files: &[contract::ContractFile],
    id: &str,
    task: &str,
) -> Result<Vec<(&'static str, BTreeMap<String, String>)>> {
    let text = |file: &str| -> Result<&str> {
        files
            .iter()
            .find(|f| f.file == file)
            .map(|f| f.text.as_str())
            .ok_or_else(|| anyhow::anyhow!("{id} does not pin {file}"))
    };
    let scope = items::Scope::task(task, text(&format!("tasks/{task}.md"))?);
    STAGES
        .iter()
        .map(|stage| Ok((*stage, items::view(stage, text(stage)?, &scope))))
        .collect()
}

/// Every task `task` depends on, directly or not, sorted.
fn transitive_deps(h: &Handover, task: &str) -> Result<BTreeSet<String>> {
    let mut seen = BTreeSet::new();
    let mut todo = h.depends_on(task)?;
    while let Some(d) = todo.pop() {
        if seen.insert(d.clone()) {
            todo.extend(h.depends_on(&d)?);
        }
    }
    Ok(seen)
}

/// A verified run of `task` in another handover of `h`'s intake that this
/// handover can reuse, newest first. `runs` are all runs of the project.
pub fn find(
    project_root: &Path,
    h: &Handover,
    task: &str,
    runs: &[(RunMeta, RunState)],
) -> Result<Option<(RunMeta, RunState)>> {
    let key = contract_key(h, task)?;
    // The outputs its dependencies have in this handover.
    let here: Vec<(RunMeta, RunState)> = runs
        .iter()
        .filter(|(m, _)| m.intake == h.intake && m.handover == h.manifest.id)
        .cloned()
        .collect();
    let mut expected = BTreeMap::new();
    for dep in h.depends_on(task)? {
        match start::output_of(&dep, &h.manifest.id, &here) {
            Ok(src) => expected.insert(dep, src.commit),
            Err(_) => return Ok(None),
        };
    }

    let mut keys: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (meta, state) in runs.iter().rev() {
        let candidate = meta.intake == h.intake
            && meta.handover != h.manifest.id
            && meta.kind == RunKind::Task
            && meta.task == task
            && state.status == RunStatus::Verified
            && state.output.is_some();
        if !candidate {
            continue;
        }
        let theirs = keys.entry(meta.handover.clone()).or_insert_with(|| {
            // A handover whose snapshots no longer verify offers nothing.
            contract::load_handover(project_root, &format!("{}/{}", meta.intake, meta.handover))
                .ok()
                .and_then(|old| contract_key(&old, task).ok())
        });
        if theirs.as_deref() != Some(key.as_str()) {
            continue;
        }
        let started_from: BTreeMap<String, String> = meta
            .start
            .iter()
            .flat_map(|s| s.from.iter())
            .map(|s| (s.task.clone(), s.commit.clone()))
            .collect();
        if started_from == expected {
            return Ok(Some((meta.clone(), state.clone())));
        }
    }
    Ok(None)
}

/// Record that `task` in `h` reuses `from` (found by [`find`]).
pub fn record(
    project_root: &Path,
    h: &Handover,
    task: &str,
    from: &(RunMeta, RunState),
) -> Result<Run> {
    let (old, state) = from;
    let (Some(candidate), Some(commit)) = (state.last_candidate.clone(), state.output.clone())
    else {
        anyhow::bail!("{} has no sealed output to reuse", old.id);
    };
    let _claim =
        super::feature_ops::claim(project_root, &h.intake, &h.manifest.id, task, RunKind::Task)?;
    let run = record::create(project_root, |id| RunMeta {
        id: id.to_string(),
        created_at: Utc::now(),
        intake: h.intake.clone(),
        handover: h.manifest.id.clone(),
        task: task.to_string(),
        manifest_sha256: h.manifest_sha256.clone(),
        // Never created: the output lives on the reused run's branch.
        worktree: worktree::path_for(project_root, id),
        branch: old.branch.clone(),
        baseline_commit: h.manifest.baseline.commit.clone(),
        budget_usd: 0.0,
        max_iterations: 0,
        retry_of: None,
        start: old.start.clone(),
        kind: RunKind::Task,
        checks: None,
    })?;
    run.append(&RunEvent::Reused {
        at: Utc::now(),
        from_run: old.id.clone(),
        from_handover: old.handover.clone(),
        candidate,
        commit,
    })?;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intake::handover::{Baseline, Manifest, Policy};
    use crate::intake::readiness::Pinned;
    use crate::run::contract::ContractFile;

    fn handover(files: &[(&str, String)], baseline: &str) -> Handover {
        let tasks = vec!["A".to_string(), "B".into(), "C".into(), "D".into()];
        Handover {
            intake: "F".into(),
            manifest: Manifest {
                id: "HANDOVER-001".into(),
                intake: "F".into(),
                created_at: Utc::now(),
                channel: "tty".into(),
                by: None,
                session: None,
                tasks,
                files: Vec::<Pinned>::new(),
                knowledge: Vec::new(),
                known_failures: Vec::new(),
                questions: Vec::new(),
                baseline: Baseline {
                    branch: "main".into(),
                    commit: baseline.into(),
                    candidate: None,
                },
                policy: Policy {
                    max_iterations: 1,
                    budget_usd: 1.0,
                },
                delivery: String::new(),
            },
            manifest_sha256: "m".into(),
            knowledge: Vec::new(),
            reference_docs: Vec::new(),
            files: files
                .iter()
                .map(|(file, text)| ContractFile {
                    file: (*file).into(),
                    revision: 1,
                    sha256: crate::intake::hash::sha256(text),
                    text: text.clone(),
                })
                .collect(),
        }
    }

    const OUTCOME: &str = "# F\n\n## Requirements\n\n- REQ-001: add\n- REQ-002: sub\n\n\
        ## Out of scope\n\n- floats\n";

    /// A ← B ← C, D alone; A and C serve REQ-001, B and D REQ-002. The
    /// outcome is readable as items, the other stages are not. `edit`
    /// changes one file's text.
    fn files(edit: &dyn Fn(&str, String) -> String) -> Vec<(&'static str, String)> {
        let task = |req: &str, deps: &str| {
            format!("---\nid: X\nparent: F\nrequirements: [{req}]\ndepends_on: [{deps}]\n---\n")
        };
        [
            ("01-outcome.md", OUTCOME.to_string()),
            ("02-behavior.md", "prose".into()),
            ("03-solution.md", "prose".into()),
            ("04-breakdown.md", "prose".into()),
            ("tasks/A.md", task("REQ-001", "")),
            ("tasks/B.md", task("REQ-002", "A")),
            ("tasks/C.md", task("REQ-001", "B")),
            ("tasks/D.md", task("REQ-002", "")),
        ]
        .into_iter()
        .map(|(f, t)| (f, edit(f, t)))
        .collect()
    }

    fn key_with(edit: &dyn Fn(&str, String) -> String, baseline: &str, task: &str) -> String {
        contract_key(&handover(&files(edit), baseline), task).unwrap()
    }

    fn key(change: &str, baseline: &str, task: &str) -> String {
        key_with(
            &|f, t| {
                if f == change {
                    format!("{t}\nchanged")
                } else {
                    t
                }
            },
            baseline,
            task,
        )
    }

    #[test]
    fn the_key_covers_what_the_work_depended_on() {
        let same = |change: &str, task: &str| key(change, "base", task) == key("", "base", task);
        // A task's own file, and those of its dependencies, direct or not.
        assert!(!same("tasks/C.md", "C"));
        assert!(!same("tasks/B.md", "C"));
        assert!(!same("tasks/A.md", "C"));
        // Not those of tasks it does not depend on.
        assert!(same("tasks/D.md", "C"));
        assert!(same("tasks/C.md", "A"));
        // A stage that cannot be read as items, whole, for every task; and
        // the baseline.
        for stage in &STAGES[1..] {
            assert!(!same(stage, "D"), "{stage}");
        }
        assert_ne!(key("", "base", "D"), key("", "other", "D"));
    }

    /// D6 as amended 12/10/2026: of a readable stage, only the items a
    /// task rests on.
    #[test]
    fn the_key_covers_only_the_items_a_task_rests_on() {
        let edit = |from: &'static str, to: &'static str| {
            move |f: &str, t: String| {
                if f == "01-outcome.md" {
                    t.replace(from, to)
                } else {
                    t
                }
            }
        };
        let base = |task: &str| key("", "base", task);
        let req2 = edit("- REQ-002: sub", "- REQ-002: subtract");
        assert_eq!(key_with(&req2, "base", "A"), base("A"));
        assert_eq!(key_with(&req2, "base", "C"), base("C"));
        assert_ne!(key_with(&req2, "base", "B"), base("B"));
        assert_ne!(key_with(&req2, "base", "D"), base("D"));
        // A new requirement is no task's yet; prose binds nothing; the
        // scope binds every task.
        let added = edit("- REQ-002: sub", "- REQ-003: mul\n- REQ-002: sub");
        assert_eq!(key_with(&added, "base", "A"), base("A"));
        let prose = edit("# F", "# F, the calculator");
        assert_eq!(key_with(&prose, "base", "B"), base("B"));
        let scope = edit("- floats", "- floats\n- big numbers");
        for t in ["A", "B", "C", "D"] {
            assert_ne!(key_with(&scope, "base", t), base(t), "{t}");
        }
    }
}
