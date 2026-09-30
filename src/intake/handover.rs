//! Handover manifests (workflow §6.3).
//!
//! A manifest pins what a run may use: the accepted revision and hash of
//! every file handed over, the tasks in dependency order, the baseline
//! branch and commit (D6), the execution policy and the delivery boundary.
//! Runs read the contract from the pinned snapshots, never from the working
//! files (D2). Manifests are written once under `.records/handovers/` and
//! never changed; a new handover is a new manifest.

use super::readiness::{self, Pinned};
use super::record::Decider;
use super::Intake;
use crate::config::Config;
use crate::knowledge::{self, Knowledge};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DELIVERY: &str = "local changes and a verification report; no push or merge";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub intake: String,
    pub created_at: DateTime<Utc>,
    pub channel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// The Claude Code session the user handed over from (`claude-prompt`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Tasks handed over, in dependency order.
    pub tasks: Vec<String>,
    /// Accepted revisions the contract consists of.
    pub files: Vec<Pinned>,
    /// Accepted revisions of the project's own knowledge at handover
    /// (ONBOARD REQ-010): whichever of `domain.md`/`conventions.md`/
    /// `rules.md` are currently accepted — all three on an onboarded
    /// project, fewer or none otherwise. Absent (defaults to empty) in a
    /// manifest written before this field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge: Vec<Pinned>,
    /// The baseline's known-failure list at the moment of handover
    /// (ONBOARD REQ-010, business rule 8): what a run of this handover may
    /// tolerate without failing (TASK-007). Absent (defaults to empty) in
    /// an older manifest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_failures: Vec<String>,
    pub baseline: Baseline,
    pub policy: Policy,
    pub delivery: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    /// The branch "integrated" is judged against (`knowledge.baseline`).
    pub branch: String,
    /// HEAD when the handover was made.
    pub commit: String,
    /// Fingerprint of the working tree at handover (see `evidence`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Policy {
    pub max_iterations: u32,
    pub budget_usd: f64,
}

fn dir(intake: &Intake) -> PathBuf {
    intake.records_dir().join("handovers")
}

/// Every manifest of the intake, oldest first.
pub fn list(intake: &Intake) -> Result<Vec<Manifest>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir(intake))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p)?;
            serde_json::from_str(&text).with_context(|| format!("parse {}", p.display()))
        })
        .collect()
}

/// Write a manifest for `requested` tasks (all when empty). Readiness is
/// checked again under the intake's lock and must still pin exactly
/// `confirmed` — the revisions the user was shown and agreed to — so an
/// edit or acceptance in between cannot slip into the handover. The CLI
/// only calls this for a human at a terminal (D1).
pub fn create(
    intake: &Intake,
    project_root: &Path,
    requested: &[String],
    config: &Config,
    confirmed: &[Pinned],
    by: Option<String>,
) -> Result<Manifest> {
    create_as(
        intake,
        project_root,
        requested,
        config,
        confirmed,
        &Decider::terminal(by),
    )
}

/// [`create`] through `who`'s channel (D1: a terminal, or the user's own
/// message to Claude Code read by the prompt hook).
pub fn create_as(
    intake: &Intake,
    project_root: &Path,
    requested: &[String],
    config: &Config,
    confirmed: &[Pinned],
    who: &Decider,
) -> Result<Manifest> {
    let _lock = super::review::lock(intake)?;
    let r = readiness::check(intake, project_root, requested, config)?;
    if !r.ready {
        bail!(
            "{} is not ready to hand over; run `zforge readiness {}`",
            intake.id,
            intake.id
        );
    }
    if r.files != confirmed {
        bail!("the accepted revisions changed after they were shown; check readiness again");
    }
    let commit = git_head(project_root)?;
    let n = list(intake)?.len() + 1;
    let k = Knowledge::open(config);
    let mut knowledge_pins = Vec::new();
    for f in knowledge::FILES {
        let status = knowledge::file_status(&k, f)?;
        if let Some(a) = status.accepted {
            knowledge_pins.push(Pinned {
                file: f.to_string(),
                revision: a.revision,
                sha256: a.sha256,
            });
        }
    }
    let known_failures = knowledge::known::list(&k)?;
    let manifest = Manifest {
        id: format!("HANDOVER-{n:03}"),
        intake: intake.id.clone(),
        created_at: Utc::now(),
        channel: who.channel.into(),
        by: who.by.clone(),
        session: who.session.clone(),
        tasks: r.tasks,
        files: r.files,
        knowledge: knowledge_pins,
        known_failures,
        baseline: Baseline {
            branch: config.knowledge.baseline.clone(),
            commit,
            candidate: crate::evidence::fingerprint(project_root)
                .hash()
                .map(String::from),
        },
        policy: Policy {
            max_iterations: config.execution.max_iterations,
            budget_usd: config.execution.budget_usd.unwrap_or_default(),
        },
        delivery: DELIVERY.into(),
    };
    std::fs::create_dir_all(dir(intake))?;
    let path = dir(intake).join(format!("{}.json", manifest.id));
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    let mut text = serde_json::to_string_pretty(&manifest)?;
    text.push('\n');
    crate::fs::write_atomic(&path, text.as_bytes())?;
    Ok(manifest)
}

fn git_head(root: &Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .context("run git")?;
    if !out.status.success() {
        bail!(
            "cannot read HEAD: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intake::review;
    use crate::knowledge::known;

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

    fn config_at(root: &Path, test_command: &str) -> Config {
        let yaml = format!(
            "project:\n  name: p\n  language: go\n  test_command: \"{test_command}\"\n\
             execution:\n  budget_usd: 1.0\n"
        );
        let mut cfg: Config = serde_yaml::from_str(&yaml).unwrap();
        cfg.config_file = root.join(".zforge").join("config.yaml");
        cfg
    }

    /// A minimal intake, accepted top to bottom, with one task serving its
    /// one requirement — just enough for `readiness::check` to be ready.
    fn accept_intake(root: &Path) -> Intake {
        let i = review::create(root, "F").unwrap();
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
            review::review(&i, &f).unwrap();
            review::accept(&i, &f, None).unwrap();
        }
        i
    }

    fn accept_all_knowledge(root: &Path, k: &Knowledge) {
        for f in knowledge::FILES {
            let title = f.trim_end_matches(".md");
            std::fs::write(
                k.dir.join(f),
                format!("# {title}\n\nA thing happens.\n\n## Open questions\n"),
            )
            .unwrap();
            knowledge::review(root, k, f).unwrap();
            knowledge::accept(k, f, None).unwrap();
        }
    }

    /// AC-01: a handover on an onboarded project pins the three accepted
    /// knowledge revisions with their hashes and the known-failure list.
    #[test]
    fn create_pins_accepted_knowledge_and_known_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        std::fs::write(
            root.join("fake_test.sh"),
            "#!/bin/sh\nprintf -- '--- FAIL: TestA (0.00s)\\nFAIL\\n'\nexit 1\n",
        )
        .unwrap();
        commit_all(root);

        let cfg = config_at(root, &format!("sh {}", root.join("fake_test.sh").display()));
        let report = crate::knowledge::probe::run(&cfg).unwrap();
        assert!(!report.baseline.result);

        let k = Knowledge::open(&cfg);
        accept_all_knowledge(root, &k);
        known::record_known(&k, &["TestA".to_string()], None).unwrap();

        let i = accept_intake(root);
        let r = readiness::check(&i, root, &[], &cfg).unwrap();
        assert!(r.ready, "{:?}", r.checks);
        let m = create(&i, root, &[], &cfg, &r.files, None).unwrap();

        assert_eq!(m.knowledge.len(), 3, "{:?}", m.knowledge);
        for f in knowledge::FILES {
            let pin = m
                .knowledge
                .iter()
                .find(|p| p.file == f)
                .unwrap_or_else(|| panic!("{f} missing from pins: {:?}", m.knowledge));
            let status = knowledge::file_status(&k, f).unwrap();
            let accepted = status.accepted.unwrap();
            assert_eq!(pin.revision, accepted.revision);
            assert_eq!(pin.sha256, accepted.sha256);
        }
        assert_eq!(m.known_failures, vec!["TestA".to_string()]);
    }

    /// AC-03: a project that never onboarded still hands over — nothing
    /// about the knowledge or known-failure pins refuses it.
    #[test]
    fn create_on_a_project_never_onboarded_pins_nothing_but_still_hands_over() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        commit_all(root);
        let cfg = config_at(root, "true");

        let i = accept_intake(root);
        let r = readiness::check(&i, root, &[], &cfg).unwrap();
        assert!(r.ready, "{:?}", r.checks);
        let m = create(&i, root, &[], &cfg, &r.files, None).unwrap();

        assert!(m.knowledge.is_empty());
        assert!(m.known_failures.is_empty());
    }

    /// AC-06: a manifest written before these fields existed still loads,
    /// with both new fields defaulting to empty.
    #[test]
    fn a_manifest_without_knowledge_fields_loads_as_before() {
        let json = r#"{
            "id": "HANDOVER-001",
            "intake": "F",
            "created_at": "2024-01-01T00:00:00Z",
            "channel": "cli-tty",
            "tasks": ["TASK-001"],
            "files": [],
            "baseline": {"branch": "main", "commit": "abc"},
            "policy": {"max_iterations": 3, "budget_usd": 1.0},
            "delivery": "local"
        }"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert!(m.knowledge.is_empty());
        assert!(m.known_failures.is_empty());
    }
}
