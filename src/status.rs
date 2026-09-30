//! `zforge status`: where each intake of a project stands, and what to do
//! next. Everything is derived — from the intake's decision log, its
//! handover manifests and the runs' event logs — nothing is stored.

use crate::intake::{self, handover, readiness, review, status::DocState, Intake};
use crate::run::feature::{self, FeatureState, Progress};
use crate::run::feature_ops::{self, Step};
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct ProjectStatus {
    pub root: PathBuf,
    /// The project still has files of the removed task pipeline:
    /// `zforge migrate` moves them.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub needs_migration: bool,
    /// Whether the project's own knowledge is onboarded (ONBOARD REQ-001):
    /// shown above the intakes, since it applies to the whole project.
    pub onboarding: OnboardingStatus,
    pub intakes: Vec<IntakeStatus>,
}

/// One stale citation, named for `zforge status`'s `knowledge: N items
/// stale — …` line (ONBOARD REQ-009, TASK-004): which item, and which
/// knowledge file it is in.
#[derive(Debug, Clone, Serialize)]
pub struct StaleKnowledge {
    pub id: String,
    pub file: String,
}

/// `zforge status`'s view of `crate::onboard::OnboardState`: just enough
/// to print a line and, while the project is not onboarded, to point at
/// `zforge onboard` (Output, AC-06). `stale` is filled in separately by
/// [`onboarding_status`] — deriving it needs the knowledge document set,
/// not just `OnboardState` (ONBOARD TASK-004).
#[derive(Debug, Clone, Serialize)]
pub struct OnboardingStatus {
    pub onboarded: bool,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<StaleKnowledge>,
    /// Tests still on the known-failure list that a run recorded passing
    /// (ONBOARD TASK-011, AC-04, business rule 8) — worth dropping from
    /// `zforge onboard baseline --known`. Filled in separately by
    /// [`onboarding_status`], the same way `stale` is.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub known_passing: Vec<String>,
}

impl From<crate::onboard::OnboardState> for OnboardingStatus {
    fn from(s: crate::onboard::OnboardState) -> Self {
        let accepted = s
            .files
            .iter()
            .filter(|f| f.state == DocState::Accepted)
            .count();
        let total = s.files.len();
        let baseline_bit = match (s.baseline.result, &s.baseline.reason) {
            (None, _) => "baseline not probed yet".to_string(),
            (Some(true), _) => "baseline green".to_string(),
            (Some(false), Some(reason)) => format!("baseline red: {reason}"),
            (Some(false), None) => {
                let unknown = s
                    .baseline
                    .failing
                    .iter()
                    .filter(|t| !s.baseline.known.contains(t))
                    .count();
                if s.baseline.failing.is_empty() {
                    "baseline red".to_string()
                } else if unknown == 0 {
                    format!("baseline red, {} known", s.baseline.failing.len())
                } else {
                    format!("baseline red, {unknown} unknown failure(s)")
                }
            }
        };
        let summary = format!("{accepted}/{total} knowledge files accepted, {baseline_bit}");
        let next = (!s.onboarded).then(|| "zforge onboard".to_string());
        Self {
            onboarded: s.onboarded,
            summary,
            next,
            stale: Vec::new(),
            known_passing: Vec::new(),
        }
    }
}

fn onboarding_status(project_root: &Path) -> OnboardingStatus {
    let config_path = project_root.join(".zforge").join("config.yaml");
    let Ok(config) = crate::config::load_from(&config_path) else {
        return OnboardingStatus {
            onboarded: false,
            summary: "not onboarded".to_string(),
            next: Some("zforge onboard".to_string()),
            stale: Vec::new(),
            known_passing: Vec::new(),
        };
    };
    let mut s: OnboardingStatus = match crate::onboard::state(&config) {
        Ok(s) => s.into(),
        Err(_) => OnboardingStatus {
            onboarded: false,
            summary: "not onboarded".to_string(),
            next: Some("zforge onboard".to_string()),
            stale: Vec::new(),
            known_passing: Vec::new(),
        },
    };
    let k = crate::knowledge::Knowledge::open(&config);
    if let Ok(report) = crate::knowledge::stale::check(&k) {
        s.stale = report
            .stale
            .into_iter()
            .map(|i| StaleKnowledge {
                id: i.id,
                file: i.file,
            })
            .collect();
    }
    if let Ok(known) = crate::knowledge::known::list(&k) {
        if !known.is_empty() {
            s.known_passing = recovered_known_tests(project_root, &known);
        }
    }
    s
}

/// Known-failure tests some run recorded as passing (ONBOARD TASK-011,
/// AC-04, business rule 8): a run's `verified.known_passing` named one of
/// them and it is still on `known`, so it is worth dropping from the list.
fn recovered_known_tests(project_root: &Path, known: &[String]) -> Vec<String> {
    let mut found = BTreeSet::new();
    for run in crate::run::record::list(project_root).into_iter().flatten() {
        let Ok(state) = run.state() else { continue };
        for t in &state.known_passing {
            if known.contains(t) {
                found.insert(t.clone());
            }
        }
    }
    found.into_iter().collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct IntakeStatus {
    pub id: String,
    /// File name → derived state (`draft`, `in_review`, …, `accepted`).
    pub files: BTreeMap<String, String>,
    /// Accepted files resting on something that has a newer accepted
    /// revision (D6): they must be confirmed again before a handover.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<String>,
    pub handovers: Vec<HandoverStatus>,
    /// The next command to run, in words a user can follow.
    pub next: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandoverStatus {
    pub id: String,
    /// Task → progress label (`ready`, `running`, `verified`, `failed`, …).
    pub tasks: BTreeMap<String, String>,
    pub integration: String,
}

/// The status of every intake under `project_root`, in id order.
pub fn project(project_root: &Path) -> Result<ProjectStatus> {
    let mut intakes = Vec::new();
    for id in intake_ids(project_root) {
        let intake = Intake::open(project_root, &id)?;
        intakes.push(intake_status(project_root, &intake)?);
    }
    Ok(ProjectStatus {
        root: project_root.to_path_buf(),
        needs_migration: crate::migrate::project::needs_migration(project_root),
        onboarding: onboarding_status(project_root),
        intakes,
    })
}

fn intake_ids(project_root: &Path) -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(intake::intakes_dir(project_root))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|id| intake::validate_id(id).is_ok())
        .collect();
    ids.sort();
    ids
}

fn intake_status(project_root: &Path, intake: &Intake) -> Result<IntakeStatus> {
    let docs = review::statuses(intake)?;
    let stale: Vec<String> = readiness::stale(intake)?.into_iter().collect();
    let mut handovers = Vec::new();
    let mut latest: Option<FeatureState> = None;
    for m in handover::list(intake)? {
        let f = feature::load(project_root, &format!("{}/{}", intake.id, m.id))?;
        handovers.push(HandoverStatus {
            id: m.id.clone(),
            tasks: f
                .tasks
                .iter()
                .map(|t| (t.task.clone(), t.progress.label().to_string()))
                .collect(),
            integration: f.integration.label().to_string(),
        });
        latest = Some(f);
    }
    let files: BTreeMap<String, String> = docs
        .iter()
        .map(|d| (d.file.clone(), label(d.state, d.has_draft)))
        .collect();
    let next = next_step(&intake.id, &docs, &stale, latest.as_ref());
    Ok(IntakeStatus {
        id: intake.id.clone(),
        files,
        stale,
        handovers,
        next,
    })
}

fn label(state: DocState, has_draft: bool) -> String {
    match (state, has_draft) {
        (DocState::Accepted, true) => "accepted, draft pending".into(),
        (s, _) => s.as_str().into(),
    }
}

/// What the user does next, earliest stage first: files still being
/// written or reviewed come before handing over, and handing over before
/// running.
fn next_step(
    id: &str,
    docs: &[intake::status::DocStatus],
    stale: &[String],
    latest: Option<&FeatureState>,
) -> String {
    let named = |pred: &dyn Fn(&intake::status::DocStatus) -> bool| -> Vec<&str> {
        docs.iter()
            .filter(|d| pred(d))
            .map(|d| d.file.as_str())
            .collect()
    };
    let in_review = named(&|d| d.state == DocState::InReview);
    if !in_review.is_empty() {
        return format!(
            "decide on {} in a terminal: `zforge intake accept|revise {id} <file>`",
            few(&in_review)
        );
    }
    let to_send = named(&|d| {
        matches!(
            d.state,
            DocState::Draft | DocState::ChangedSinceReview | DocState::NeedsRevision
        ) || (d.state == DocState::Accepted && d.has_draft)
    });
    if !to_send.is_empty() {
        return format!(
            "finish {} and send it for review: `zforge intake review {id} <file>`",
            few(&to_send)
        );
    }
    if !stale.is_empty() {
        return format!(
            "confirm {} again against what changed upstream: `zforge intake review {id} <file>`",
            few(stale)
        );
    }
    let Some(f) = latest else {
        return format!("`zforge readiness {id}`, then `zforge handover {id}` in a terminal");
    };
    run_step(f)
}

/// Files named in a hint: the first few, then how many more.
fn few<S: AsRef<str>>(files: &[S]) -> String {
    const SHOWN: usize = 3;
    let names: Vec<&str> = files.iter().take(SHOWN).map(AsRef::as_ref).collect();
    match files.len().saturating_sub(SHOWN) {
        0 => names.join(", "),
        more => format!("{} and {more} more", names.join(", ")),
    }
}

fn run_step(f: &FeatureState) -> String {
    let h = &f.handover;
    let none = BTreeSet::new();
    match feature_ops::next_step(f, &none, &none) {
        Step::Busy { run, .. } => format!("{h} is running ({run}): `zforge run status {h}`"),
        Step::Task { .. } | Step::Integration { .. } => {
            format!("`zforge run {h}` (resumes where it stopped)")
        }
        Step::Done if f.is_verified() => {
            format!("{h} is verified; merge its integration branch, then `zforge knowledge index`")
        }
        Step::Done => {
            let stopped: Vec<&str> = f
                .tasks
                .iter()
                .filter(|t| !matches!(t.progress, Progress::Verified { .. }))
                .map(|t| t.task.as_str())
                .collect();
            format!(
                "{} did not pass: see `zforge run status {h}`; amend the intake and hand over again",
                stopped.join(", ")
            )
        }
    }
}

/// One project's status as seen from `zforge status --global`.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalRow {
    pub project: String,
    pub path: PathBuf,
    #[serde(flatten)]
    pub result: GlobalResult,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GlobalResult {
    Ok {
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        needs_migration: bool,
        onboarding: OnboardingStatus,
        intakes: Vec<IntakeStatus>,
    },
    Skipped {
        reason: String,
    },
}

/// Every registered project's status. Projects are read in parallel; one
/// that takes longer than `timeout` in total, is gone or cannot be read is
/// reported as skipped instead of holding up the rest.
pub fn global(timeout: Duration) -> Result<Vec<GlobalRow>> {
    let registry = crate::registry::io::load()?;
    let pending: Vec<_> = registry
        .projects
        .into_iter()
        .map(|entry| {
            let (tx, rx) = mpsc::channel();
            let path = entry.path.clone();
            std::thread::spawn(move || {
                let _ = tx.send(project_at(&path));
            });
            (entry, rx)
        })
        .collect();
    let deadline = Instant::now() + timeout;
    Ok(pending
        .into_iter()
        .map(|(entry, rx)| {
            let left = deadline.saturating_duration_since(Instant::now());
            let result = match rx.recv_timeout(left) {
                Ok(Ok(s)) => GlobalResult::Ok {
                    needs_migration: s.needs_migration,
                    onboarding: s.onboarding,
                    intakes: s.intakes,
                },
                Ok(Err(e)) => GlobalResult::Skipped {
                    reason: format!("{e:#}"),
                },
                Err(_) => GlobalResult::Skipped {
                    reason: format!("no answer within {}ms", timeout.as_millis()),
                },
            };
            GlobalRow {
                project: entry.name,
                path: entry.path,
                result,
            }
        })
        .collect())
}

fn project_at(path: &Path) -> Result<ProjectStatus> {
    if !path.join(".zforge").is_dir() {
        anyhow::bail!("{} has no .zforge/", path.display());
    }
    project(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intake::status::DocStatus;

    fn doc(file: &str, state: DocState) -> DocStatus {
        DocStatus {
            file: file.into(),
            state,
            accepted: None,
            in_review: None,
            has_draft: false,
            last_revision: 0,
            current_sha256: None,
            revision_note: None,
        }
    }

    #[test]
    fn a_decision_waiting_comes_before_writing() {
        let docs = [
            doc("01-outcome.md", DocState::Accepted),
            doc("02-behavior.md", DocState::Draft),
            doc("03-solution.md", DocState::InReview),
        ];
        let next = next_step("F", &docs, &[], None);
        assert!(next.starts_with("decide on 03-solution.md"), "{next}");
    }

    #[test]
    fn unfinished_files_then_stale_ones_then_the_handover() {
        let draft = [
            doc("01-outcome.md", DocState::Accepted),
            doc("02-behavior.md", DocState::NeedsRevision),
        ];
        assert!(next_step("F", &draft, &[], None).contains("intake review F"));

        let accepted = [doc("01-outcome.md", DocState::Accepted)];
        let stale = ["tasks/TASK-001.md".to_string()];
        let next = next_step("F", &accepted, &stale, None);
        assert!(
            next.starts_with("confirm tasks/TASK-001.md again"),
            "{next}"
        );

        let next = next_step("F", &accepted, &[], None);
        assert!(next.contains("zforge readiness F"), "{next}");
    }

    #[test]
    fn long_lists_are_cut() {
        assert_eq!(few(&["a", "b"]), "a, b");
        assert_eq!(few(&["a", "b", "c", "d", "e"]), "a, b, c and 2 more");
    }
}
