//! The review pack: one Markdown document with everything the user needs
//! to decide on an intake's files under review at once — the brief, then
//! for each file its revision, summary and changes since the accepted
//! revision, the warnings, and the exact words to decide with. A tiny or
//! fixup intake's 02/03/04 still in the standard text zforge wrote
//! (`templates::is_standard`) take one line: there is nothing in them to
//! read.
//!
//! Read-only and stored nowhere: it is rendered from the records each time.

use super::lint::{self, Severity};
use super::readability::SUMMARY;
use super::record::{Decision, DecisionKind};
use super::status::{DocState, Rev};
use super::{batch, hash, record, review, templates, Intake, BRIEF_FILE};
use crate::knowledge::docs::DocSet;
use anyhow::Result;
use std::fmt::Write as _;

/// Text with every Markdown heading outside fenced blocks one level lower,
/// so an embedded document sits under the pack's own headings.
fn demote(text: &str) -> String {
    let mut in_fence = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && line.starts_with('#') {
            out.push_str("##");
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The `## Summary` section of `text`, without guidance comments.
fn summary(text: &str) -> Option<String> {
    let (_, body) = lint::split_frontmatter(text);
    let secs = lint::sections(&lint::strip_comments(body));
    let lines = lint::section(&secs, SUMMARY)?;
    let s = lines.join("\n").trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Why `file` changed since its accepted revision `accepted` (`None`:
/// none), up to the revision under review: each revision's note and each
/// request for changes, newest first. Empty for a first revision.
fn history(log: &[Decision], file: &str, accepted: Option<&Rev>, pending: u32) -> Vec<String> {
    let (since, same) = accepted.map_or((0, ""), |a| (a.revision, a.sha256.as_str()));
    log.iter()
        .rev()
        .filter(|d| d.file == file && d.revision > since && d.revision <= pending)
        .filter_map(|d| match d.decision {
            DecisionKind::Review if d.revision > 1 => Some(if !d.note.is_empty() {
                format!("revision {}: {}", d.revision, d.note)
            } else if d.sha256 == same {
                format!("revision {}: unchanged, to confirm again", d.revision)
            } else {
                format!("revision {}: no note", d.revision)
            }),
            DecisionKind::NeedsRevision => Some(format!(
                "revision {}, changes asked: {}",
                d.revision, d.note
            )),
            _ => None,
        })
        .collect()
}

/// The review pack of `intake`.
pub fn render(intake: &Intake) -> Result<String> {
    let id = &intake.id;
    let mut out = format!("# {id} — Review pack\n\n");

    let brief = std::fs::read_to_string(intake.dir.join(BRIEF_FILE))
        .map(|b| lint::strip_comments(&b))
        .unwrap_or_default();
    let brief_body: String = brief
        .lines()
        .skip_while(|l| l.trim().is_empty() || l.starts_with("# "))
        .collect::<Vec<_>>()
        .join("\n");
    out.push_str("## Brief\n\n");
    if brief_body.trim().is_empty() {
        out.push_str("_No brief written yet._\n\n");
    } else {
        out.push_str(demote(brief_body.trim()).trim_end());
        out.push_str("\n\n");
    }

    let files = batch::under_review(intake)?;
    if files.is_empty() {
        let _ = writeln!(
            out,
            "## Files under review\n\nNothing is under review. Send the intake with \
             `zforge intake review {id} --all`."
        );
        return Ok(out);
    }

    let statuses = review::statuses(intake)?;
    let log = record::read(intake)?;
    let profile = std::fs::read_to_string(intake.dir.join(lint::OUTCOME))
        .ok()
        .and_then(|o| lint::profile(&o));
    let mut warnings = Vec::new();
    let mut changed = Vec::new();
    out.push_str("## Files under review\n\n");
    for f in &files {
        let Some(s) = statuses.iter().find(|s| &s.file == f) else {
            continue;
        };
        let Some(pending) = &s.in_review else {
            continue;
        };
        let text = record::read_snapshot(intake, f, pending.revision)?;
        let _ = writeln!(
            out,
            "### {f} — revision {} ({})\n",
            pending.revision,
            hash::short(&pending.sha256)
        );
        if s.state == DocState::ChangedSinceReview {
            changed.push(f.clone());
            let _ = writeln!(
                out,
                "**Edited since it was sent for review — it cannot be accepted until it is \
                 sent again.**\n"
            );
        }
        if let Some(p) = profile
            .as_deref()
            .filter(|p| templates::is_standard(p, f, &text))
        {
            let _ = writeln!(
                out,
                "Standard text for a {p} intake — nothing of its own to decide.\n"
            );
            continue;
        }
        match summary(&text) {
            Some(sum) => {
                let _ = writeln!(out, "{}\n", demote(&sum).trim_end());
            }
            None if f.starts_with("tasks/") => {}
            None => out.push_str("_No summary._\n\n"),
        }
        let why = history(&log, f, s.accepted.as_ref(), pending.revision);
        if !why.is_empty() {
            out.push_str("Why it changed, newest first:\n\n");
            for line in &why {
                let _ = writeln!(out, "- {line}");
            }
            out.push('\n');
        }
        match &s.accepted {
            Some(a) => {
                let diff = review::diff_against(intake, f, a.revision, pending.revision);
                if diff.trim().is_empty() {
                    let _ = writeln!(
                        out,
                        "Unchanged since accepted revision {}: confirm it again, since something \
                         it builds on changed.\n",
                        a.revision
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "Changes since accepted revision {}:\n\n```diff\n{}\n```\n",
                        a.revision,
                        diff.trim_end()
                    );
                }
            }
            None => out.push_str("New: no accepted revision yet.\n\n"),
        }
        warnings.extend(
            intake
                .lint(f, &text)
                .into_iter()
                .filter(|i| i.severity == Severity::Warning),
        );
    }

    out.push_str("## Warnings\n\n");
    if warnings.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for w in &warnings {
            let _ = writeln!(out, "- {}: {}", w.file, w.message);
        }
        out.push('\n');
    }

    out.push_str("## Decide\n\n");
    if !changed.is_empty() {
        let _ = writeln!(
            out,
            "First send the edited files again (`zforge intake review {id} --all`): {}.\n",
            changed.join(", ")
        );
    }
    let _ = writeln!(
        out,
        "- Accept every file above: type `/accept {id} all`\n\
         - Ask for changes: type `/revise {id} <file>: <what to change>`\n\
         - At a terminal: `zforge intake accept {id} --all`"
    );
    Ok(out)
}
