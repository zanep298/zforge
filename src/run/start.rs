//! Where a run starts (MOC-C TASK-002; REQ-002, REQ-003).
//!
//! A task without dependencies starts from the handover's baseline commit,
//! as in Mốc B. A task with dependencies starts from their outputs — the
//! commit each dependency's verified run sealed (`output`): one dependency,
//! its commit; several, the first one's commit with the others merged in, in
//! the order the task lists them. A dependency with no verified run in the
//! same handover stops the task before anything is created.
//!
//! The merges happen in the run's own worktree, never in the user's
//! checkout. A conflict aborts the merge and fails the run without calling
//! the agent: which outputs to keep is a contract question, not the agent's.

use super::git;
use super::record::{self, RunKind, RunMeta, RunState, RunStatus, Source, Start};
use anyhow::{bail, Result};
use std::path::Path;

/// Where a run of a task depending on `deps` in `handover` of `intake`
/// starts: `None` for the baseline (no dependencies), or the dependencies'
/// outputs. Fails, naming the dependency, when one has no usable output.
pub fn plan(
    project_root: &Path,
    intake: &str,
    handover: &str,
    task: &str,
    deps: &[String],
) -> Result<Option<Start>> {
    if deps.is_empty() {
        return Ok(None);
    }
    let mut runs = Vec::new();
    for run in record::list(project_root)? {
        let meta = run.meta()?;
        if meta.intake == intake && meta.handover == handover {
            let state = run.state()?;
            runs.push((meta, state));
        }
    }
    let from = deps
        .iter()
        .map(|d| {
            output_of(d, handover, &runs).map_err(|why| anyhow::anyhow!("{task} depends on {why}"))
        })
        .collect::<Result<Vec<Source>>>()?;
    Ok(Some(Start {
        commit: from[0].commit.clone(),
        from,
    }))
}

/// The output of `task` among `runs` (all of one handover): its newest
/// verified run's sealed commit. Otherwise why there is none, as a clause
/// that follows "X depends on".
pub fn output_of(
    task: &str,
    handover: &str,
    runs: &[(RunMeta, RunState)],
) -> Result<Source, String> {
    let mine: Vec<&(RunMeta, RunState)> = runs
        .iter()
        .filter(|(m, _)| m.kind == RunKind::Task && m.task == task)
        .collect();
    let Some((latest, latest_state)) = mine.last() else {
        return Err(format!("{task}, which has not run in {handover}"));
    };
    let Some((meta, state)) = mine
        .iter()
        .rev()
        .find(|(_, s)| s.status == RunStatus::Verified)
    else {
        return Err(format!(
            "{task}, which has no verified run in {handover} (latest: {} {})",
            latest.id,
            latest_state.status.as_str()
        ));
    };
    match &state.output {
        Some(commit) => Ok(Source {
            task: task.to_string(),
            run: meta.id.clone(),
            commit: commit.clone(),
        }),
        None => Err(format!(
            "{task}, whose verified run {} has no sealed output (it predates them); run {task} again",
            meta.id
        )),
    }
}

/// Merge every source of `start` that `work_dir` does not already contain.
pub fn apply(work_dir: &Path, run_id: &str, start: &Start) -> Result<()> {
    let mut merged: Vec<&str> = Vec::new();
    for src in &start.from {
        let contained = git::run(
            work_dir,
            &["merge-base", "--is-ancestor", &src.commit, "HEAD"],
        )?
        .status
        .success();
        if !contained {
            merge(work_dir, run_id, src, &merged)?;
        }
        merged.push(&src.task);
    }
    Ok(())
}

fn merge(work_dir: &Path, run_id: &str, src: &Source, merged: &[&str]) -> Result<()> {
    let message = format!(
        "zforge: merge output of {} ({}) into {run_id}",
        src.task, src.run
    );
    let out = git::run(
        work_dir,
        &[
            "-c",
            "user.name=zforge",
            "-c",
            "user.email=zforge@localhost",
            "merge",
            "--no-ff",
            "--no-edit",
            "-m",
            &message,
            &src.commit,
        ],
    )?;
    if out.status.success() {
        return Ok(());
    }
    let conflicted =
        git::ok(work_dir, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    let _ = git::run(work_dir, &["merge", "--abort"]);
    if conflicted.is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let why = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        bail!(
            "could not merge the output of {} ({}): {why}",
            src.task,
            src.run
        );
    }
    let tasks: Vec<&str> = merged.iter().copied().chain([src.task.as_str()]).collect();
    bail!(
        "dependency outputs conflict: {} in {}",
        tasks.join(", "),
        conflicted.lines().collect::<Vec<_>>().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::record::RunEvent;
    use chrono::Utc;
    use std::path::PathBuf;

    fn meta(id: &str, task: &str) -> RunMeta {
        RunMeta {
            id: id.into(),
            created_at: Utc::now(),
            intake: "F".into(),
            handover: "HANDOVER-001".into(),
            task: task.into(),
            manifest_sha256: "m".into(),
            worktree: PathBuf::from("/w"),
            branch: format!("zforge/{task}/{id}"),
            baseline_commit: "base".into(),
            budget_usd: 1.0,
            max_iterations: 1,
            retry_of: None,
            start: None,
            kind: Default::default(),
            checks: None,
        }
    }

    fn state(events: &[RunEvent]) -> RunState {
        RunState::replay(events).unwrap()
    }

    fn verified(commit: Option<&str>) -> RunState {
        let at = Utc::now();
        state(&[
            RunEvent::Started { at, pid: 1 },
            RunEvent::VerifyStarted { at },
            RunEvent::Verified {
                at,
                candidate: "c".into(),
                commit: commit.map(String::from),
                tolerated: Vec::new(),
                known_passing: Vec::new(),
            },
        ])
    }

    fn failed() -> RunState {
        let at = Utc::now();
        state(&[
            RunEvent::Started { at, pid: 1 },
            RunEvent::Failed {
                at,
                reason: "x".into(),
            },
        ])
    }

    #[test]
    fn the_newest_verified_run_is_the_output() {
        let runs = vec![
            (meta("RUN-001", "A"), verified(Some("old"))),
            (meta("RUN-002", "A"), verified(Some("new"))),
            (meta("RUN-003", "A"), failed()),
            (meta("RUN-004", "B"), verified(Some("other"))),
        ];
        let src = output_of("A", "HANDOVER-001", &runs).unwrap();
        assert_eq!(
            src,
            Source {
                task: "A".into(),
                run: "RUN-002".into(),
                commit: "new".into()
            }
        );
    }

    #[test]
    fn a_dependency_without_output_says_why() {
        let none = output_of("A", "HANDOVER-001", &[]).unwrap_err();
        assert_eq!(none, "A, which has not run in HANDOVER-001");

        let runs = vec![(meta("RUN-001", "A"), failed())];
        let err = output_of("A", "HANDOVER-001", &runs).unwrap_err();
        assert_eq!(
            err,
            "A, which has no verified run in HANDOVER-001 (latest: RUN-001 failed)"
        );

        let runs = vec![(meta("RUN-001", "A"), verified(None))];
        let err = output_of("A", "HANDOVER-001", &runs).unwrap_err();
        assert!(err.contains("RUN-001 has no sealed output"), "{err}");
        assert!(err.ends_with("run A again"), "{err}");
    }

    struct Repo {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let r = Self { _dir: dir, root };
            r.git(&["init", "-q", "-b", "main", "."]);
            r.write("shared.txt", "base\n");
            r.commit("base");
            r
        }

        fn git(&self, args: &[&str]) -> String {
            git::ok(&self.root, args).unwrap()
        }

        fn write(&self, file: &str, text: &str) {
            std::fs::write(self.root.join(file), text).unwrap();
        }

        fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                message,
            ]);
            self.git(&["rev-parse", "HEAD"])
        }

        /// A commit on its own branch from `main`, writing `file`.
        fn output(&self, branch: &str, file: &str, text: &str) -> String {
            self.git(&["checkout", "-q", "-b", branch, "main"]);
            self.write(file, text);
            let c = self.commit(branch);
            self.git(&["checkout", "-q", "main"]);
            c
        }
    }

    fn src(task: &str, commit: &str) -> Source {
        Source {
            task: task.into(),
            run: format!("RUN-{task}"),
            commit: commit.into(),
        }
    }

    /// AC-03: several outputs meet in the worktree.
    #[test]
    fn several_outputs_are_merged() {
        let r = Repo::new();
        let b = r.output("b", "b.txt", "from b\n");
        let d = r.output("d", "d.txt", "from d\n");
        r.git(&["checkout", "-q", "-b", "run", &b]);

        apply(
            &r.root,
            "RUN-009",
            &Start {
                commit: b.clone(),
                from: vec![src("B", &b), src("D", &d)],
            },
        )
        .unwrap();

        assert!(r.root.join("b.txt").is_file() && r.root.join("d.txt").is_file());
        assert_eq!(
            r.git(&["log", "-1", "--format=%an|%s"]),
            "zforge|zforge: merge output of D (RUN-D) into RUN-009"
        );
        assert!(git::is_clean(&r.root).unwrap());
    }

    /// AC-04: conflicting outputs stop the run, naming tasks and files, and
    /// leave no merge half-done.
    #[test]
    fn conflicting_outputs_are_refused() {
        let r = Repo::new();
        let b = r.output("b", "shared.txt", "b's line\n");
        let d = r.output("d", "shared.txt", "d's line\n");
        r.git(&["checkout", "-q", "-b", "run", &b]);

        let err = apply(
            &r.root,
            "RUN-009",
            &Start {
                commit: b.clone(),
                from: vec![src("B", &b), src("D", &d)],
            },
        )
        .unwrap_err();

        assert_eq!(
            format!("{err:#}"),
            "dependency outputs conflict: B, D in shared.txt"
        );
        assert_eq!(r.git(&["rev-parse", "HEAD"]), b);
        assert!(git::is_clean(&r.root).unwrap());
    }
}
