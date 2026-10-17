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
use super::record::{
    self, Previous, Rebased, RunKind, RunMeta, RunState, RunStatus, Source, Start,
};
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
        previous: None,
        rebased: None,
    }))
}

/// Where a run of `task` in `handover` continues from instead of `start`
/// (the plan above): the newest verified output of the same task in
/// another handover of the intake, when that run started from exactly the
/// dependency outputs `start` names — so the task's own earlier work is
/// adapted to its changed contract rather than redone. The handover's
/// `baseline` is merged in when that output does not contain it; when the
/// merge would conflict, or git cannot tell, there is nothing to continue
/// from and the run starts as planned. `None` when nothing qualifies.
pub fn previous(
    project_root: &Path,
    intake: &str,
    handover: &str,
    task: &str,
    baseline: &str,
    start: Option<&Start>,
) -> Result<Option<Start>> {
    let expected: Vec<(String, String)> = start
        .map(|s| {
            s.from
                .iter()
                .map(|f| (f.task.clone(), f.commit.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut runs = record::list(project_root)?;
    runs.reverse();
    for run in runs {
        let meta = run.meta()?;
        if meta.intake != intake
            || meta.handover == handover
            || meta.kind != RunKind::Task
            || meta.task != task
        {
            continue;
        }
        let state = run.state()?;
        let (RunStatus::Verified, Some(output)) = (state.status, state.output.clone()) else {
            continue;
        };
        let started_from: Vec<(String, String)> = meta
            .start
            .iter()
            .flat_map(|s| s.from.iter())
            .map(|f| (f.task.clone(), f.commit.clone()))
            .collect();
        if started_from != expected {
            continue;
        }
        if !merges_cleanly(project_root, &output, baseline) {
            return Ok(None);
        }
        let previous = Previous {
            task: task.to_string(),
            run: meta.id.clone(),
            handover: meta.handover.clone(),
            commit: output.clone(),
        };
        return Ok(Some(Start {
            commit: output,
            from: start.map(|s| s.from.clone()).unwrap_or_default(),
            previous: Some(previous),
            rebased: None,
        }));
    }
    Ok(None)
}

/// Whether `baseline` is in `output` already, or merges into it without a
/// conflict (`git merge-tree`, which touches no work tree).
fn merges_cleanly(project_root: &Path, output: &str, baseline: &str) -> bool {
    let ok = |args: &[&str]| git::run(project_root, args).is_ok_and(|o| o.status.success());
    if !ok(&["cat-file", "-e", &format!("{output}^{{commit}}")]) {
        return false;
    }
    ok(&["merge-base", "--is-ancestor", baseline, output])
        || ok(&[
            "merge-tree",
            "--write-tree",
            "--no-messages",
            output,
            baseline,
        ])
}

/// Outputs that would not merge: the first source (in `sources`' order)
/// that conflicts with one before it, those it conflicts with, and the
/// files. Pairwise `git merge-tree`, touching no work tree; a pair where one
/// output already contains the other never conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub earlier: Vec<Source>,
    pub later: Source,
    pub files: Vec<String>,
}

pub fn conflicts(project_root: &Path, sources: &[Source]) -> Result<Option<Conflict>> {
    for (j, later) in sources.iter().enumerate() {
        let mut earlier = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for e in &sources[..j] {
            for f in conflicting_files(project_root, &e.commit, &later.commit)? {
                if !files.contains(&f) {
                    files.push(f);
                }
                if !earlier.contains(e) {
                    earlier.push(e.clone());
                }
            }
        }
        if !earlier.is_empty() {
            return Ok(Some(Conflict {
                earlier,
                later: later.clone(),
                files,
            }));
        }
    }
    Ok(None)
}

/// The files `a` and `b` conflict in; empty when they merge.
fn conflicting_files(project_root: &Path, a: &str, b: &str) -> Result<Vec<String>> {
    let contains = |x: &str, y: &str| -> Result<bool> {
        Ok(
            git::run(project_root, &["merge-base", "--is-ancestor", x, y])?
                .status
                .success(),
        )
    };
    if a == b || contains(a, b)? || contains(b, a)? {
        return Ok(Vec::new());
    }
    let out = git::run(
        project_root,
        &[
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            a,
            b,
        ],
    )?;
    match out.status.code() {
        Some(0) => Ok(Vec::new()),
        // Exit 1: conflicts — the tree id, then one conflicted path a line.
        Some(1) => Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .skip(1)
            .take_while(|l| !l.is_empty())
            .map(str::to_string)
            .collect()),
        _ => bail!(
            "git merge-tree {a} {b}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    }
}

/// Where a run of the conflict's later task starts: from its dependencies'
/// outputs (as [`plan`]) with the earlier outputs it conflicted with merged
/// in after them, so its new output contains them.
pub fn stacked(
    project_root: &Path,
    intake: &str,
    handover: &str,
    deps: &[String],
    c: &Conflict,
) -> Result<Start> {
    let task = &c.later.task;
    let mut from = plan(project_root, intake, handover, task, deps)?
        .map(|s| s.from)
        .unwrap_or_default();
    for e in &c.earlier {
        if !from.iter().any(|f| f.task == e.task) {
            from.push(e.clone());
        }
    }
    let Some(first) = from.first() else {
        bail!("{task}: nothing to start on top of");
    };
    Ok(Start {
        commit: first.commit.clone(),
        from: from.clone(),
        previous: None,
        rebased: Some(Rebased {
            onto: c.earlier.iter().map(|e| e.task.clone()).collect(),
            files: c.files.clone(),
            replaces: c.later.clone(),
        }),
    })
}

/// Merge `baseline` into a run continuing from an earlier output
/// (`previous`), when that output does not contain it yet.
/// For the reviewer of a run that continues `previous`: where the whole
/// task's change starts. Follows the chain of continued runs back to the
/// first, whose start commit holds none of the task's work, and names the
/// baseline change merged in since when the baseline moved.
pub fn review_scope(project_root: &Path, previous: &Previous, baseline: &str) -> Result<String> {
    let mut meta = record::Run::open(project_root, &previous.run)?.meta()?;
    while let Some(p) = meta.start.as_ref().and_then(|s| s.previous.clone()) {
        meta = record::Run::open(project_root, &p.run)?.meta()?;
    }
    let base = meta.start_commit().to_string();
    let mut text = format!(
        "This run continues {}'s verified output, so `git diff {{{{start}}}}` shows only what \
         changed in this run. Review the whole task — everything the contract asks for — with \
         `git diff {base}`",
        previous.run
    );
    if meta.baseline_commit != baseline {
        text.push_str(&format!(
            "; that diff also holds the baseline's own changes merged in since \
             (`git diff {} {baseline}`), which are not the task's",
            meta.baseline_commit
        ));
    }
    text.push_str(".\n");
    Ok(text)
}

pub fn bring_baseline(work_dir: &Path, run_id: &str, baseline: &str) -> Result<()> {
    let contained = git::run(work_dir, &["merge-base", "--is-ancestor", baseline, "HEAD"])?
        .status
        .success();
    if contained {
        return Ok(());
    }
    let message = format!("zforge: merge the handover's baseline into {run_id}");
    let merged = git::ok(
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
            baseline,
        ],
    );
    if merged.is_err() {
        let _ = git::run(work_dir, &["merge", "--abort"]);
    }
    merged.map(|_| ())
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
                previous: None,
                rebased: None,
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
                previous: None,
                rebased: None,
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

    /// A run continues from an earlier output only when the handover's
    /// baseline is in it or merges into it cleanly; otherwise it starts
    /// as planned.
    #[test]
    fn an_earlier_output_is_continued_only_when_the_baseline_merges() {
        let r = Repo::new();
        let out = r.output("earlier", "shared.txt", "earlier work\n");
        let base = r.git(&["rev-parse", "main"]);
        assert!(merges_cleanly(&r.root, &out, &base), "contained");
        r.write("other.txt", "x\n");
        let moved = r.commit("moved");
        assert!(merges_cleanly(&r.root, &out, &moved), "clean merge");
        r.write("shared.txt", "baseline's line\n");
        let conflicting = r.commit("conflict");
        assert!(!merges_cleanly(&r.root, &out, &conflicting));
        assert!(!merges_cleanly(
            &r.root,
            "0000000000000000000000000000000000000000",
            &moved
        ));

        r.git(&["checkout", "-q", "-b", "run", &out]);
        bring_baseline(&r.root, "RUN-009", &moved).unwrap();
        assert!(r.root.join("other.txt").is_file());
        assert!(git::is_clean(&r.root).unwrap());
    }

    #[test]
    fn conflicts_name_the_later_output_what_it_meets_and_the_files() {
        let r = Repo::new();
        let a = r.output("a", "shared.txt", "a's line\n");
        let b = r.output("b", "b.txt", "b only\n");
        let c = r.output("c", "shared.txt", "c's line\n");
        let sources = [src("A", &a), src("B", &b), src("C", &c)];

        let found = conflicts(&r.root, &sources).unwrap().unwrap();
        assert_eq!(found.later, src("C", &c));
        assert_eq!(found.earlier, vec![src("A", &a)]);
        assert_eq!(found.files, ["shared.txt"]);

        assert_eq!(conflicts(&r.root, &sources[..2]).unwrap(), None);

        // C redone on top of A's output contains it: no conflict any more.
        r.git(&["checkout", "-q", "-b", "c2", &a]);
        r.write("shared.txt", "a's line\nc's line\n");
        let c2 = r.commit("c2");
        r.git(&["checkout", "-q", "main"]);
        assert_eq!(
            conflicts(&r.root, &[src("A", &a), src("C", &c2)]).unwrap(),
            None
        );
    }
}
