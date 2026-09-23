//! Handover manifests (workflow §6.3).
//!
//! A manifest pins what a run may use: the accepted revision and hash of
//! every file handed over, the tasks in dependency order, the baseline
//! branch and commit (D6), the execution policy and the delivery boundary.
//! Runs read the contract from the pinned snapshots, never from the working
//! files (D2). Manifests are written once under `.records/handovers/` and
//! never changed; a new handover is a new manifest.

use super::readiness::{self, Pinned};
use super::record::CHANNEL_TTY;
use super::Intake;
use crate::config::Config;
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
    /// Tasks handed over, in dependency order.
    pub tasks: Vec<String>,
    /// Accepted revisions the contract consists of.
    pub files: Vec<Pinned>,
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
    let manifest = Manifest {
        id: format!("HANDOVER-{n:03}"),
        intake: intake.id.clone(),
        created_at: Utc::now(),
        channel: CHANNEL_TTY.into(),
        by,
        tasks: r.tasks,
        files: r.files,
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
    crate::state::write_atomic(&path, text.as_bytes())?;
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
