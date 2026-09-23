//! Reusing a task's output across handovers (MOC-C TASK-006, REQ-006).
//!
//! After an amendment the user hands over again. A task whose contract did
//! not change need not be paid for twice: its verified output from an
//! earlier handover of the same intake stands, provided nothing it was built
//! from differs. "Nothing" is exact — there is no "close enough":
//!
//! - its [`contract_key`] is equal: the baseline commit, the four stages, the
//!   task file and the task file of every dependency, direct or not, all
//!   pinned with the same SHA-256; and
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
use crate::intake::STAGES;
use anyhow::Result;
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Everything `task`'s work in handover `h` depended on, as one hash.
pub fn contract_key(h: &Handover, task: &str) -> Result<String> {
    let pinned = |file: &str| -> Result<String> {
        h.files
            .iter()
            .find(|f| f.file == file)
            .map(|f| f.sha256.clone())
            .ok_or_else(|| anyhow::anyhow!("{} does not pin {file}", h.manifest.id))
    };
    let mut lines = vec![format!("baseline {}", h.manifest.baseline.commit)];
    for stage in STAGES {
        lines.push(format!("stage {stage} {}", pinned(stage)?));
    }
    let own = format!("tasks/{task}.md");
    lines.push(format!("task {own} {}", pinned(&own)?));
    for dep in transitive_deps(h, task)? {
        let file = format!("tasks/{dep}.md");
        lines.push(format!("dep {file} {}", pinned(&file)?));
    }
    Ok(crate::intake::hash::sha256(&lines.join("\n")))
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

    fn handover(files: &[(&str, &str, &str)], baseline: &str) -> Handover {
        let tasks = vec!["A".to_string(), "B".into(), "C".into(), "D".into()];
        Handover {
            intake: "F".into(),
            manifest: Manifest {
                id: "HANDOVER-001".into(),
                intake: "F".into(),
                created_at: Utc::now(),
                channel: "tty".into(),
                by: None,
                tasks,
                files: Vec::<Pinned>::new(),
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
            files: files
                .iter()
                .map(|(file, sha, deps)| ContractFile {
                    file: (*file).into(),
                    revision: 1,
                    sha256: (*sha).into(),
                    text: format!("---\nid: X\nparent: F\nrequirements: [REQ-001]\ndepends_on: [{deps}]\n---\n"),
                })
                .collect(),
        }
    }

    /// A ← B ← C, D alone; every file's hash is its name unless changed.
    fn files(change: &str) -> Vec<(&'static str, String, &'static str)> {
        [
            ("01-outcome.md", ""),
            ("02-behavior.md", ""),
            ("03-solution.md", ""),
            ("04-breakdown.md", ""),
            ("tasks/A.md", ""),
            ("tasks/B.md", "A"),
            ("tasks/C.md", "B"),
            ("tasks/D.md", ""),
        ]
        .into_iter()
        .map(|(f, deps)| {
            let sha = if f == change {
                format!("{f}-changed")
            } else {
                f.to_string()
            };
            (f, sha, deps)
        })
        .collect()
    }

    fn key(change: &str, baseline: &str, task: &str) -> String {
        let f = files(change);
        let refs: Vec<(&str, &str, &str)> =
            f.iter().map(|(a, b, c)| (*a, b.as_str(), *c)).collect();
        contract_key(&handover(&refs, baseline), task).unwrap()
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
        // Every stage and the baseline, for every task.
        for stage in STAGES {
            assert!(!same(stage, "D"), "{stage}");
        }
        assert_ne!(key("", "base", "D"), key("", "other", "D"));
    }
}
