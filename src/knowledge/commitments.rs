//! Commitments (ONBOARD REQ-011, TASK-005): the record of what intakes
//! committed to — each requirement and each mandatory design decision,
//! linked to the revision and hash it comes from — generated next to the
//! project's knowledge, apart from the files the user reviews and
//! approves (03-solution "Where things live").
//!
//! Moved here from `intake::knowledge` (the original v1.5 `knowledge
//! index`) with the same rules (workflow §9, decision D6): nothing is
//! copied or reworded beyond the item's own line, and two statuses are
//! kept apart:
//!
//! - **decision** — `active` while the item is in the file's current
//!   accepted revision, `superseded` once a later accepted revision drops it;
//! - **implementation** — derived only from records and git, the highest
//!   level that holds: `integrated` when the output of a verified
//!   integration run of a handover holding one of its tasks is in the
//!   baseline branch now (`git merge-base --is-ancestor`; a squash or rebase
//!   merge is not recognised, and nothing assumes it); `integration_verified`
//!   when that run exists; `verified` when a run of such a task ended
//!   verified (with that run and the candidate it tested); `handed_over`
//!   when such a task is in a handover manifest; else `not_implemented`.
//!   Never from what an agent wrote, and never stored: every build asks
//!   again (MOC-C TASK-007).
//!
//! `write` puts `commitments.md` in `knowledge.dir` (generated, not
//! reviewed) and `commitments.json` in `.zforge/knowledge/` — local, since
//! it names local runs and only tools read it there. It also removes the
//! `.zforge/knowledge/index.{md,json}` this replaces, once it can tell
//! they are zforge's own (AC-03): a file the project put there itself is
//! left alone.

use crate::config::Config;
use crate::intake::handover;
use crate::intake::lint::{self as intake_lint, TaskMeta};
use crate::intake::record::{self, DecisionKind};
use crate::intake::{hash, intakes_dir, Intake};
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Active,
    Superseded,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Implementation {
    NotImplemented,
    HandedOver,
    /// A run of a task serving this item passed its verification;
    /// `verified_by` and `candidate` on the entry say which run and which
    /// tree.
    Verified,
    /// The handover's integration check passed; `integration` says where.
    IntegrationVerified,
    /// That integration's output is in the baseline branch.
    Integrated,
}

/// The integration run that checked an item's handover.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Integration {
    pub handover: String,
    pub run: String,
    pub candidate: Option<String>,
    /// The integration's sealed output.
    pub commit: String,
    /// The baseline branch `integrated` is judged against.
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub intake: String,
    /// `REQ-001`, or `03-solution.md#3` for the third mandatory decision.
    pub id: String,
    pub kind: &'static str,
    /// The item's line as accepted.
    pub text: String,
    pub source: String,
    pub revision: u32,
    pub sha256: String,
    pub decision: DecisionStatus,
    pub implementation: Implementation,
    /// Tasks that answer for a requirement.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<String>,
    /// Handover manifests those tasks are in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub handovers: Vec<String>,
    /// The run that verified one of those tasks (latest first seen).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
    /// Fingerprint of the tree that run verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<Integration>,
}

/// Where `commitments.json` lives: local, next to run and handover
/// records — the same directory the retired `index.{md,json}` used.
pub fn json_dir(project_root: &Path) -> PathBuf {
    project_root.join(".zforge").join("knowledge")
}

/// Every entry across all intakes, in intake order.
pub fn build(project_root: &Path) -> Result<Vec<Entry>> {
    let mut ids: Vec<String> = std::fs::read_dir(intakes_dir(project_root))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    let verified = verified_runs(project_root)?;
    let integrations = integration_runs(project_root)?;
    let mut entries = Vec::new();
    for id in ids {
        let intake = Intake::open(project_root, &id)?;
        let mut these = intake_entries(&intake, &verified)?;
        add_integration(project_root, &intake, &integrations, &mut these)?;
        entries.extend(these);
    }
    Ok(entries)
}

/// Accepted revisions of `file`, oldest first, with their text.
fn accepted_revisions(intake: &Intake, file: &str) -> Result<Vec<(u32, String, String)>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for d in record::read(intake)? {
        if d.file == file && d.decision == DecisionKind::Accepted && seen.insert(d.revision) {
            out.push((
                d.revision,
                d.sha256.clone(),
                record::read_snapshot(intake, file, d.revision)?,
            ));
        }
    }
    Ok(out)
}

/// The run that verified a task, and the tree it tested.
#[derive(Debug, Clone)]
pub struct VerifiedBy {
    pub run: String,
    pub candidate: Option<String>,
}

/// `(intake, task)` → the latest run that verified it.
type VerifiedTasks = BTreeMap<(String, String), VerifiedBy>;

/// `(intake, task)` → the latest run that verified it, with its candidate.
/// Read-only: the run log is the record, and reading it never changes it.
fn verified_runs(project_root: &Path) -> Result<VerifiedTasks> {
    let mut out = BTreeMap::new();
    for run in crate::run::record::list(project_root)? {
        let meta = run.meta()?;
        let state = run.state()?;
        if meta.kind == crate::run::record::RunKind::Task
            && state.status == crate::run::record::RunStatus::Verified
        {
            out.insert(
                (meta.intake.clone(), meta.task.clone()),
                VerifiedBy {
                    run: run.id.clone(),
                    candidate: state.last_candidate.clone(),
                },
            );
        }
    }
    Ok(out)
}

/// Verified integration runs, oldest first: `(intake, handover, run,
/// candidate, commit)`.
type IntegrationRuns = Vec<(String, String, String, Option<String>, String)>;

fn integration_runs(project_root: &Path) -> Result<IntegrationRuns> {
    let mut out = Vec::new();
    for run in crate::run::record::list(project_root)? {
        let meta = run.meta()?;
        if meta.kind != crate::run::record::RunKind::Integration {
            continue;
        }
        let state = run.state()?;
        if let (crate::run::record::RunStatus::Verified, Some(commit)) =
            (state.status, state.output.clone())
        {
            out.push((
                meta.intake,
                meta.handover,
                run.id,
                state.last_candidate,
                commit,
            ));
        }
    }
    Ok(out)
}

/// Raise verified requirements whose handover passed its integration check
/// — and whose integration is in the baseline branch now — to those levels.
fn add_integration(
    project_root: &Path,
    intake: &Intake,
    runs: &IntegrationRuns,
    entries: &mut [Entry],
) -> Result<()> {
    let branches: BTreeMap<String, String> = handover::list(intake)?
        .into_iter()
        .map(|m| (m.id, m.baseline.branch))
        .collect();
    let integrated =
        |commit: &str, branch: &str| crate::run::git::in_branch(project_root, commit, branch);
    for e in entries.iter_mut() {
        if e.kind != "requirement" || e.implementation != Implementation::Verified {
            continue;
        }
        let candidates: Vec<Integration> = runs
            .iter()
            .rev()
            .filter(|(i, h, ..)| *i == intake.id && e.handovers.contains(h))
            .filter_map(|(_, h, run, candidate, commit)| {
                Some(Integration {
                    handover: h.clone(),
                    run: run.clone(),
                    candidate: candidate.clone(),
                    commit: commit.clone(),
                    branch: branches.get(h)?.clone(),
                })
            })
            .collect();
        if let Some(i) = candidates.iter().find(|i| integrated(&i.commit, &i.branch)) {
            e.implementation = Implementation::Integrated;
            e.integration = Some(i.clone());
        } else if let Some(i) = candidates.into_iter().next() {
            e.implementation = Implementation::IntegrationVerified;
            e.integration = Some(i);
        }
    }
    Ok(())
}

fn intake_entries(intake: &Intake, verified: &VerifiedTasks) -> Result<Vec<Entry>> {
    // Tasks as accepted, and the manifests they were handed over in.
    let mut metas: BTreeMap<String, TaskMeta> = BTreeMap::new();
    for f in intake.files() {
        let Some(t) = f.strip_prefix("tasks/").and_then(|n| n.strip_suffix(".md")) else {
            continue;
        };
        if let Some((_, _, text)) = accepted_revisions(intake, &f)?.pop() {
            if let Ok(meta) = intake_lint::parse_task_meta(&text) {
                metas.insert(t.to_string(), meta);
            }
        }
    }
    let mut handed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in handover::list(intake)? {
        for t in &m.tasks {
            handed.entry(t.clone()).or_default().push(m.id.clone());
        }
    }

    let mut entries = Vec::new();
    let requirement_lines = |text: &str| -> Vec<(String, String)> {
        intake_lint::strip_comments(text)
            .lines()
            .filter_map(|l| {
                let id = intake_lint::defined_requirements(l).pop()?;
                Some((id, item_text(l)))
            })
            .collect()
    };
    collect(
        intake,
        intake_lint::OUTCOME,
        "requirement",
        requirement_lines,
        &mut entries,
    )?;
    let decision_lines = |text: &str| -> Vec<(String, String)> {
        let clean = intake_lint::strip_comments(text);
        let secs = intake_lint::sections(&clean);
        intake_lint::section(&secs, intake_lint::BINDING_DECISIONS)
            .unwrap_or_default()
            .iter()
            .filter(|l| l.trim_start().starts_with("- ") || l.trim_start().starts_with("* "))
            .map(|l| item_text(l))
            .enumerate()
            .map(|(i, t)| (format!("03-solution.md#{}", i + 1), t))
            .collect()
    };
    collect(
        intake,
        "03-solution.md",
        "decision",
        decision_lines,
        &mut entries,
    )?;

    for e in &mut entries {
        if e.kind != "requirement" {
            continue;
        }
        e.tasks = metas
            .iter()
            .filter(|(_, m)| m.requirements.contains(&e.id))
            .map(|(t, _)| t.clone())
            .collect();
        let mut hs: Vec<String> = e
            .tasks
            .iter()
            .flat_map(|t| handed.get(t).cloned().unwrap_or_default())
            .collect();
        hs.sort();
        hs.dedup();
        if e.decision == DecisionStatus::Active && !hs.is_empty() {
            e.implementation = Implementation::HandedOver;
        }
        e.handovers = hs;
        // A superseded item is never reported as built.
        if e.decision != DecisionStatus::Active {
            continue;
        }
        if let Some(v) = e
            .tasks
            .iter()
            .find_map(|t| verified.get(&(intake.id.clone(), t.clone())))
        {
            e.implementation = Implementation::Verified;
            e.verified_by = Some(v.run.clone());
            e.candidate = v.candidate.clone();
        }
    }
    Ok(entries)
}

/// Entries from every accepted revision of `file`: items in the current
/// accepted revision are active; items only in older ones are superseded
/// and cite the last revision that had them.
fn collect(
    intake: &Intake,
    file: &str,
    kind: &'static str,
    items: impl Fn(&str) -> Vec<(String, String)>,
    out: &mut Vec<Entry>,
) -> Result<()> {
    let revisions = accepted_revisions(intake, file)?;
    let Some((current_rev, _, current_text)) = revisions.last().cloned() else {
        return Ok(());
    };
    let current: BTreeSet<String> = items(&current_text).into_iter().map(|(id, _)| id).collect();
    let mut latest: BTreeMap<String, (u32, String, String)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for (rev, sha, text) in &revisions {
        for (id, line) in items(text) {
            if !latest.contains_key(&id) {
                order.push(id.clone());
            }
            latest.insert(id, (*rev, sha.clone(), line));
        }
    }
    for id in order {
        let (revision, sha256, text) = latest[&id].clone();
        let active = current.contains(&id) && revision == current_rev;
        out.push(Entry {
            intake: intake.id.clone(),
            id,
            kind,
            text,
            source: file.to_string(),
            revision,
            sha256,
            decision: if active {
                DecisionStatus::Active
            } else {
                DecisionStatus::Superseded
            },
            implementation: Implementation::NotImplemented,
            tasks: Vec::new(),
            handovers: Vec::new(),
            verified_by: None,
            candidate: None,
            integration: None,
        });
    }
    Ok(())
}

/// A list item's text without its bullet.
fn item_text(line: &str) -> String {
    line.trim()
        .trim_start_matches(['-', '*', '#'])
        .trim()
        .to_string()
}

/// A line every version of the retired `index.md` carried — used only to
/// tell zforge's own generated file apart from one the project wrote
/// itself (AC-03), the same way `cli::init::instructions` recognises a
/// generated `CLAUDE.md`.
const LEGACY_MARKER: &str = "Generated by `zforge knowledge index`";

fn legacy_generated(text: &str) -> bool {
    text.contains(LEGACY_MARKER)
}

/// Remove the retired `.zforge/knowledge/index.md` and its `index.json`
/// companion once `commitments.md`/`commitments.json` replace them —
/// only when `index.md` is zforge's own (AC-03); anything else under
/// `.zforge/knowledge/` (a file the project put there itself) is left
/// alone.
fn remove_legacy(project_root: &Path) -> Result<()> {
    let dir = json_dir(project_root);
    let md = dir.join("index.md");
    let ours = match std::fs::read_to_string(&md) {
        Ok(text) => legacy_generated(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    if ours {
        std::fs::remove_file(&md)?;
        let json = dir.join("index.json");
        if json.is_file() {
            std::fs::remove_file(&json)?;
        }
    }
    Ok(())
}

/// Write `commitments.md` in `knowledge.dir` (generated, not reviewed) and
/// `commitments.json` in `.zforge/knowledge/` (Output). Returns the
/// entries. Never touches `domain.md`, `conventions.md` or `rules.md`
/// (AC-04).
pub fn write(config: &Config) -> Result<Vec<Entry>> {
    let project_root = config.project_root();
    let entries = build(&project_root)?;

    let knowledge_dir = config.knowledge_dir();
    std::fs::create_dir_all(&knowledge_dir)?;
    crate::fs::write_atomic(
        &knowledge_dir.join("commitments.md"),
        render(&entries).as_bytes(),
    )?;

    let json_dir = json_dir(&project_root);
    std::fs::create_dir_all(&json_dir)?;
    let mut json = serde_json::to_string_pretty(&entries)?;
    json.push('\n');
    crate::fs::write_atomic(&json_dir.join("commitments.json"), json.as_bytes())?;

    remove_legacy(&project_root)?;
    Ok(entries)
}

pub fn render(entries: &[Entry]) -> String {
    let mut out = String::from(
        "# Commitments\n\n<!-- Generated by `zforge knowledge index` from accepted intake \
         revisions; editing it changes nothing. -->\n\n",
    );
    if entries.is_empty() {
        out.push_str("Nothing accepted yet.\n");
        return out;
    }
    out.push_str(
        "| Intake | ID | Item | Source | Decision | Implementation |\n|---|---|---|---|---|---|\n",
    );
    for e in entries {
        let implementation = match e.implementation {
            Implementation::NotImplemented => "not implemented".to_string(),
            Implementation::HandedOver => format!("handed over ({})", e.handovers.join(", ")),
            Implementation::Verified => format!(
                "verified ({}{})",
                e.verified_by.as_deref().unwrap_or("?"),
                e.candidate
                    .as_deref()
                    .map(|c| format!(", candidate {}", hash::short(c)))
                    .unwrap_or_default()
            ),
            Implementation::IntegrationVerified => match &e.integration {
                Some(i) => format!(
                    "integration verified ({} of {}{})",
                    i.run,
                    i.handover,
                    i.candidate
                        .as_deref()
                        .map(|c| format!(", candidate {}", hash::short(c)))
                        .unwrap_or_default()
                ),
                None => "integration verified".to_string(),
            },
            Implementation::Integrated => match &e.integration {
                Some(i) => format!(
                    "integrated ({} of {}, {} in {})",
                    i.run,
                    i.handover,
                    hash::short(&i.commit),
                    i.branch
                ),
                None => "integrated".to_string(),
            },
        };
        let decision = match e.decision {
            DecisionStatus::Active => "active",
            DecisionStatus::Superseded => "superseded",
        };
        out.push_str(&format!(
            "| {} | {} | {} | [{} rev {}](../intakes/{}/{}) `{}` | {} | {} |\n",
            e.intake,
            e.id,
            e.text.replace('|', "\\|"),
            e.source,
            e.revision,
            e.intake,
            e.source,
            hash::short(&e.sha256),
            decision,
            implementation
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn config(root: &Path) -> Config {
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        let config_file = root.join(".zforge").join("config.yaml");
        std::fs::write(
            &config_file,
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();
        crate::config::load_from(&config_file).unwrap()
    }

    /// AC-01: writing commitments creates both files, empty but present
    /// when nothing has been accepted yet.
    #[test]
    fn write_creates_commitments_md_and_json() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let cfg = config(tmp.path());
        std::fs::create_dir_all(intakes_dir(tmp.path())).unwrap();

        let entries = write(&cfg).unwrap();
        assert!(entries.is_empty());
        let md = cfg.knowledge_dir().join("commitments.md");
        assert!(md.is_file(), "{}", md.display());
        let json = json_dir(tmp.path()).join("commitments.json");
        assert!(json.is_file(), "{}", json.display());
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(parsed, serde_json::json!([]));
    }

    /// AC-03: a retired, zforge-generated `index.md`/`index.json` pair is
    /// removed once `commitments.*` replace it; a file the project wrote
    /// itself in the same directory stays.
    #[test]
    fn write_removes_the_retired_generated_index_and_keeps_other_files() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let cfg = config(tmp.path());
        std::fs::create_dir_all(intakes_dir(tmp.path())).unwrap();
        let legacy_dir = json_dir(tmp.path());
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(
            legacy_dir.join("index.md"),
            "# Knowledge index\n\n<!-- Generated by `zforge knowledge index` from accepted \
             intake revisions; editing it changes nothing. -->\n\nNothing accepted yet.\n",
        )
        .unwrap();
        std::fs::write(legacy_dir.join("index.json"), "[]\n").unwrap();
        std::fs::write(legacy_dir.join("notes.md"), "hand-written notes\n").unwrap();

        write(&cfg).unwrap();

        assert!(!legacy_dir.join("index.md").exists());
        assert!(!legacy_dir.join("index.json").exists());
        assert!(legacy_dir.join("notes.md").is_file());
        assert_eq!(
            std::fs::read_to_string(legacy_dir.join("notes.md")).unwrap(),
            "hand-written notes\n"
        );
    }

    /// AC-03: a hand-written `index.md` — no marker — is left alone,
    /// together with any `index.json` beside it.
    #[test]
    fn write_leaves_a_hand_written_index_alone() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let cfg = config(tmp.path());
        std::fs::create_dir_all(intakes_dir(tmp.path())).unwrap();
        let legacy_dir = json_dir(tmp.path());
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("index.md"), "# My own notes\n").unwrap();
        std::fs::write(legacy_dir.join("index.json"), "not generated by zforge\n").unwrap();

        write(&cfg).unwrap();

        assert_eq!(
            std::fs::read_to_string(legacy_dir.join("index.md")).unwrap(),
            "# My own notes\n"
        );
        assert_eq!(
            std::fs::read_to_string(legacy_dir.join("index.json")).unwrap(),
            "not generated by zforge\n"
        );
    }

    /// AC-04: `knowledge index` never writes the three reviewed knowledge
    /// files.
    #[test]
    fn write_never_touches_the_reviewed_knowledge_files() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let cfg = config(tmp.path());
        std::fs::create_dir_all(intakes_dir(tmp.path())).unwrap();

        write(&cfg).unwrap();

        for f in super::super::FILES {
            assert!(
                !cfg.knowledge_dir().join(f).exists(),
                "{f} should not have been written"
            );
        }
    }
}
