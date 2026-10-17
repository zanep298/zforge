//! `zforge intake amend <NEW> --from <ID>`: a new intake that changes one
//! whose work is already integrated (D6 as amended 12/10/2026).
//!
//! Its stages and tasks start as copies of the source intake's latest
//! *accepted* revisions — never its working files — each marked with what
//! it amends, so the user writes only what changes. They are drafts like
//! any other: nothing is reviewed or accepted here, an unchanged copy too
//! (D1); the user accepts them, e.g. all at once with `/accept all`.
//!
//! With `--fixup` ([`fixup`]) nothing is copied: for a few contained
//! fixes, the new intake states only them — a short 01-outcome with new
//! REQ IDs, 02-behavior and 03-solution that say "as accepted in <ID>,
//! except:", a breakdown with one task and that task's stub. All four
//! stages still exist and are reviewed, accepted and pinned like any
//! intake's.
//!
//! When the source intake's handover is not integrated yet, editing that
//! intake in place and handing it over again is the lighter path: tasks
//! whose items did not change are reused (`run::reuse`).

use super::{handover, record, review, Intake, BRIEF_FILE, STAGES};
use anyhow::{bail, Result};
use std::path::Path;

/// Where the amended intake's work stands, as far as the records say.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Origin {
    pub intake: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handover: Option<String>,
    /// The verified integration run of that handover, and its output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

impl Origin {
    fn describe(&self) -> String {
        let mut s = self.intake.clone();
        if let Some(h) = &self.handover {
            s.push_str(&format!(", {h}"));
        }
        match (&self.integration_run, &self.commit) {
            (Some(r), Some(c)) => s.push_str(&format!(", integration {r} at {c}")),
            (Some(r), None) => s.push_str(&format!(", integration {r}")),
            _ => {}
        }
        s
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Amended {
    pub intake: String,
    pub origin: Origin,
    /// Files copied, with the source revision each came from.
    pub copied: Vec<(String, u32)>,
    /// Source files with no accepted revision, not copied.
    pub skipped: Vec<String>,
}

/// Create intake `new_id` from `from_id`'s accepted revisions. `handover`
/// names the source handover to record; by default its latest.
pub fn amend(
    project_root: &Path,
    new_id: &str,
    from_id: &str,
    handover_id: Option<&str>,
) -> Result<Amended> {
    let src = Intake::open(project_root, from_id)?;
    let mut accepted = Vec::new();
    let mut skipped = Vec::new();
    for s in review::statuses(&src)? {
        if !(STAGES.contains(&s.file.as_str()) || s.file.starts_with("tasks/")) {
            continue;
        }
        match &s.accepted {
            Some(a) => accepted.push((s.file.clone(), a.revision)),
            None => skipped.push(s.file.clone()),
        }
    }
    if !accepted.iter().any(|(f, _)| STAGES.contains(&f.as_str())) {
        bail!("{from_id} has no accepted stage to amend");
    }
    let origin = origin(project_root, &src, handover_id)?;
    // Read every snapshot before creating anything.
    let mut texts = Vec::new();
    for (file, rev) in &accepted {
        texts.push((file.clone(), record::read_snapshot(&src, file, *rev)?));
    }

    let new = review::create(project_root, new_id)?;
    let note = origin.describe();
    for ((file, rev), (_, text)) in accepted.iter().zip(&texts) {
        let body = if file.starts_with("tasks/") {
            task_copy(text, from_id, new_id, &origin)
        } else {
            stage_copy(text, from_id, new_id, &note, *rev)
        };
        let path = new.dir.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::fs::write_atomic(&path, body.as_bytes())?;
    }
    let brief = super::templates::brief(new_id);
    let (title, rest) = brief.split_once('\n').unwrap_or((&brief, ""));
    crate::fs::write_atomic(
        &new.dir.join(BRIEF_FILE),
        format!("{title}\n\nAmends {from_id}: write only what changes.\nAmended: {note}.\n{rest}")
            .as_bytes(),
    )?;
    Ok(Amended {
        intake: new.id,
        origin,
        copied: accepted,
        skipped,
    })
}

/// What [`fixup`] wrote.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Fixup {
    pub intake: String,
    pub origin: Origin,
    /// Files written, in the order the user works through them.
    pub written: Vec<String>,
    /// The source's accepted revision of each stage the new one refers to.
    pub refers_to: Vec<(String, u32)>,
    /// The first new requirement ID, continuing after the source's.
    pub first_requirement: String,
}

/// The one task a fixup intake starts with.
pub const FIXUP_TASK: &str = "TASK-001";

/// Create fixup intake `new_id` amending `from_id`: compact stages that
/// refer to the source's accepted revisions, and one task stub. Every
/// stage of the source must be accepted — a fixup amends settled work.
pub fn fixup(
    project_root: &Path,
    new_id: &str,
    from_id: &str,
    handover_id: Option<&str>,
) -> Result<Fixup> {
    let src = Intake::open(project_root, from_id)?;
    let statuses = review::statuses(&src)?;
    let mut refers_to = Vec::new();
    for stage in STAGES {
        match statuses
            .iter()
            .find(|s| s.file == stage)
            .and_then(|s| s.accepted.as_ref())
        {
            Some(a) => refers_to.push((stage.to_string(), a.revision)),
            None => bail!(
                "{from_id}'s {stage} has no accepted revision: a fixup amends an intake whose \
                 stages are all accepted"
            ),
        }
    }
    let origin = origin(project_root, &src, handover_id)?;
    let outcome = record::read_snapshot(&src, super::lint::OUTCOME, refers_to[0].1)?;
    let last = super::lint::defined_requirements(&outcome)
        .iter()
        .filter_map(|r| r.strip_prefix("REQ-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let req = format!("REQ-{:03}", last + 1);

    let new = review::create(project_root, new_id)?;
    let describe = origin.describe();
    let mut written = Vec::new();
    for (stage, rev) in &refers_to {
        let snapshot = record::snapshot_path(&src, stage, *rev);
        let snapshot = snapshot
            .strip_prefix(project_root)
            .unwrap_or(&snapshot)
            .display()
            .to_string();
        let revision = rev.to_string();
        let body = super::templates::fixup_stage(
            stage,
            new_id,
            &[
                ("from", from_id),
                ("origin", &describe),
                ("revision", &revision),
                ("req", &req),
                ("snapshot", &snapshot),
            ],
        )
        .expect("fixup template for every stage");
        crate::fs::write_atomic(&new.dir.join(stage), body.as_bytes())?;
        written.push(stage.clone());
    }
    let task = super::templates::task(new_id, FIXUP_TASK).replacen(
        "requirements: []",
        &format!("requirements: [{req}]"),
        1,
    );
    let task = task_copy(&task, from_id, new_id, &origin);
    let rel = format!("{}/{FIXUP_TASK}.md", super::TASKS_DIR);
    crate::fs::write_atomic(&new.dir.join(&rel), task.as_bytes())?;
    written.push(rel);

    let brief = super::templates::brief(new_id);
    let (title, rest) = brief.split_once('\n').unwrap_or((&brief, ""));
    crate::fs::write_atomic(
        &new.dir.join(BRIEF_FILE),
        format!(
            "{title}\n\nFixup of {from_id}: only the fixes; everything else stays as \
             accepted.\nAmended: {describe}.\n{rest}"
        )
        .as_bytes(),
    )?;
    Ok(Fixup {
        intake: new.id,
        origin,
        written,
        refers_to,
        first_requirement: req,
    })
}

/// The source handover — named, or the latest — and its verified
/// integration, when there is one.
fn origin(project_root: &Path, src: &Intake, handover_id: Option<&str>) -> Result<Origin> {
    let manifests = handover::list(src)?;
    let chosen = match handover_id {
        Some(h) => match manifests.iter().find(|m| m.id == h) {
            Some(m) => Some(m),
            None => bail!("{} has no handover {h}", src.id),
        },
        None => manifests.last(),
    };
    let mut o = Origin {
        intake: src.id.clone(),
        ..Origin::default()
    };
    let Some(m) = chosen else { return Ok(o) };
    o.handover = Some(m.id.clone());
    if let Ok(f) = crate::run::feature::load(project_root, &format!("{}/{}", src.id, m.id)) {
        if let crate::run::feature::Progress::Verified { run, commit, .. } = f.integration {
            o.integration_run = Some(run);
            o.commit = commit;
        }
    }
    Ok(o)
}

/// A stage copy: the title renamed, and a note under it.
fn stage_copy(text: &str, from: &str, new: &str, note: &str, rev: u32) -> String {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let renamed = first
        .strip_prefix(&format!("# {from} "))
        .map(|t| format!("# {new} {t}"))
        .unwrap_or_else(|| first.to_string());
    let comment = format!(
        "<!-- amends: {note}. Copied from {from}'s accepted revision {rev}: change only what \
         changes, then send it for review. -->"
    );
    if first.starts_with("# ") {
        format!("{renamed}\n\n{comment}\n{rest}")
    } else {
        format!("{comment}\n\n{text}")
    }
}

/// A task copy: `parent` set to the new intake, `amends` added.
fn task_copy(text: &str, from: &str, new: &str, origin: &Origin) -> String {
    let (Some(fm), body) = super::lint::split_frontmatter(text) else {
        return text.to_string();
    };
    let mut lines: Vec<String> = Vec::new();
    for line in fm.lines() {
        if line.trim_start().starts_with("parent:") {
            lines.push(format!("parent: {new}"));
            lines.push(format!("amends: {from}"));
            if let Some(h) = &origin.handover {
                lines.push(format!("amends_handover: {h}"));
            }
            if let Some(c) = &origin.commit {
                lines.push(format!("amends_commit: {c}"));
            }
        } else {
            lines.push(line.to_string());
        }
    }
    format!("---\n{}\n---\n{body}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_renamed_and_marked() {
        let o = Origin {
            intake: "F".into(),
            handover: Some("HANDOVER-002".into()),
            integration_run: Some("RUN-009".into()),
            commit: Some("abc".into()),
        };
        let t = task_copy(
            "---\nid: TASK-001\nparent: F\nrequirements: [REQ-001]\n---\n\n# TASK-001\n",
            "F",
            "G",
            &o,
        );
        let meta = super::super::lint::parse_task_meta(&t).unwrap();
        assert_eq!(meta.parent, "G");
        assert!(t.contains("amends: F\namends_handover: HANDOVER-002\namends_commit: abc\n"));
        assert!(t.ends_with("\n# TASK-001\n"), "{t}");

        let s = stage_copy("# F — Outcome\n\n## Summary\n", "F", "G", &o.describe(), 3);
        assert!(s.starts_with("# G — Outcome\n\n<!-- amends: F, HANDOVER-002, integration RUN-009 at abc. Copied from F's accepted revision 3"), "{s}");
        assert!(s.ends_with("\n## Summary\n"));
    }
}
