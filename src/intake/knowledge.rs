//! Product knowledge index (workflow §9, decision D6).
//!
//! Generated from the intakes' accepted revisions only: each requirement
//! and each mandatory design decision, linked to the revision and hash it
//! comes from. Nothing is copied or reworded beyond the item's own line
//! (§9.2), and two statuses are kept apart:
//!
//! - **decision** — `active` while the item is in the file's current
//!   accepted revision, `superseded` once a later accepted revision drops it;
//! - **implementation** — derived only from records: `verified` when a run
//!   of a task covering it ended verified (with that run and the candidate
//!   it tested), `handed_over` when such a task is in a handover manifest,
//!   else `not_implemented`. Never from what an agent wrote; `integrated`
//!   waits for Mốc C.

use super::handover;
use super::lint::{self, TaskMeta};
use super::record::{self, DecisionKind};
use super::{hash, intakes_dir, Intake};
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const MANDATORY_DECISIONS: &str = "Quyết định bắt buộc";

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
}

pub fn index_dir(project_root: &Path) -> PathBuf {
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
    let mut entries = Vec::new();
    for id in ids {
        let intake = Intake::open(project_root, &id)?;
        entries.extend(intake_entries(&intake, &verified)?);
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

fn intake_entries(intake: &Intake, verified: &VerifiedTasks) -> Result<Vec<Entry>> {
    // Tasks as accepted, and the manifests they were handed over in.
    let mut metas: BTreeMap<String, TaskMeta> = BTreeMap::new();
    for f in intake.files() {
        let Some(t) = f.strip_prefix("tasks/").and_then(|n| n.strip_suffix(".md")) else {
            continue;
        };
        if let Some((_, _, text)) = accepted_revisions(intake, &f)?.pop() {
            if let Ok(meta) = lint::parse_task_meta(&text) {
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
        lint::strip_comments(text)
            .lines()
            .filter_map(|l| {
                let id = lint::defined_requirements(l).pop()?;
                Some((id, item_text(l)))
            })
            .collect()
    };
    collect(
        intake,
        lint::OUTCOME,
        "requirement",
        requirement_lines,
        &mut entries,
    )?;
    let decision_lines = |text: &str| -> Vec<(String, String)> {
        let clean = lint::strip_comments(text);
        lint::sections(&clean)
            .into_iter()
            .find(|(t, _)| t == MANDATORY_DECISIONS)
            .map(|(_, lines)| lines)
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

/// Write `index.md` (view) and `index.json` (for tools). Returns the entries.
pub fn write(project_root: &Path) -> Result<Vec<Entry>> {
    let entries = build(project_root)?;
    let dir = index_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let mut json = serde_json::to_string_pretty(&entries)?;
    json.push('\n');
    crate::state::write_atomic(&dir.join("index.json"), json.as_bytes())?;
    crate::state::write_atomic(&dir.join("index.md"), render(&entries).as_bytes())?;
    Ok(entries)
}

pub fn render(entries: &[Entry]) -> String {
    let mut out = String::from(
        "# Knowledge index\n\n<!-- Generated by `zforge knowledge index` from accepted intake revisions; \
         editing it changes nothing. -->\n\n",
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
