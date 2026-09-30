//! Carry out a decision the user typed (`super::command`) on the project:
//! find what it names — an intake, the knowledge, a file — and record it
//! through the prompt channel (D1). Nothing here asks the model anything;
//! the result is told to both the user and the model.

use super::command::Command;
use crate::config::Config;
use crate::intake::record::Decider;
use crate::intake::status::DocState;
use crate::intake::status::Rev;
use crate::intake::{handover, hash, readiness, review, Intake};
use crate::knowledge::Knowledge;
use anyhow::{anyhow, bail, Result};
use std::path::Path;

/// What a decision did, in words for the user and for the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Every line recorded or refused.
    pub lines: Vec<String>,
    /// Whether anything was recorded.
    pub recorded: bool,
    /// The intake it concerned, when one.
    pub intake: Option<String>,
}

/// What a decision can be about: one intake, or the project's knowledge.
enum Target {
    Intake(Intake),
    Knowledge(Knowledge),
}

impl Target {
    fn label(&self) -> String {
        match self {
            Self::Intake(i) => i.id.clone(),
            Self::Knowledge(_) => "knowledge".into(),
        }
    }

    /// Files sent for review and not decided on — including those edited
    /// since, so a decision on them is refused out loud rather than
    /// skipped.
    fn in_review(&self) -> Result<Vec<String>> {
        let statuses = match self {
            Self::Intake(i) => review::statuses(i)?,
            Self::Knowledge(k) => review::statuses(k)?,
        };
        Ok(statuses
            .into_iter()
            .filter(|s| matches!(s.state, DocState::InReview | DocState::ChangedSinceReview))
            .map(|s| s.file)
            .collect())
    }

    fn accept(&self, rel: &str, who: &Decider) -> Result<Rev> {
        match self {
            Self::Intake(i) => review::accept_as(i, rel, who),
            Self::Knowledge(k) => review::accept_as(k, rel, who),
        }
    }

    fn revise(&self, rel: &str, who: &Decider, note: &str) -> Result<Rev> {
        match self {
            Self::Intake(i) => review::revise_as(i, rel, who, note),
            Self::Knowledge(k) => review::revise_as(k, rel, who, note),
        }
    }
}

/// Record `cmd` for the project at `root`.
pub fn run(root: &Path, config: &Config, cmd: &Command, who: &Decider) -> Result<Outcome> {
    match cmd {
        Command::Accept { words } => accept(root, config, words, who),
        Command::Revise { words, note } => revise(root, config, words, note.as_deref(), who),
        Command::Handover { words } => hand_over(root, config, words, who),
    }
}

fn accept(root: &Path, config: &Config, words: &[String], who: &Decider) -> Result<Outcome> {
    let (target, rest) = target_for(root, config, words, |t| Ok(!t.in_review()?.is_empty()))?;
    let pending = target.in_review()?;
    let files = match rest {
        [] => pending.clone(),
        [all] if all == "all" => pending.clone(),
        named => named
            .iter()
            .map(|w| resolve_file(&pending, w, &target))
            .collect::<Result<_>>()?,
    };
    if files.is_empty() {
        bail!("nothing of {} is under review", target.label());
    }
    let mut out = outcome(&target);
    for f in files {
        match target.accept(&f, who) {
            Ok(rev) => {
                out.recorded = true;
                out.lines.push(format!(
                    "✓ accepted {f} rev {} ({})",
                    rev.revision,
                    hash::short(&rev.sha256)
                ));
            }
            Err(e) => out.lines.push(format!("✗ {f}: {e:#}")),
        }
    }
    after_decision(config, &target, &out);
    Ok(out)
}

fn revise(
    root: &Path,
    config: &Config,
    words: &[String],
    note: Option<&str>,
    who: &Decider,
) -> Result<Outcome> {
    let (target, rest) = target_for(root, config, words, |t| {
        let pending = t.in_review()?;
        Ok(words
            .first()
            .is_some_and(|w| resolve_file(&pending, w, t).is_ok()))
    })?;
    let Some((file, tail)) = rest.split_first() else {
        bail!("name the file to revise: /revise <file>: <what needs to change>");
    };
    let note = match note {
        Some(n) => n.to_string(),
        None => tail.join(" "),
    };
    let rel = resolve_file(&target.in_review()?, file, &target)?;
    let rev = target.revise(&rel, who, &note)?;
    let mut out = outcome(&target);
    out.recorded = true;
    out.lines.push(format!(
        "✓ changes requested to {rel} rev {} ({}): {note}",
        rev.revision,
        hash::short(&rev.sha256)
    ));
    after_decision(config, &target, &out);
    Ok(out)
}

fn hand_over(root: &Path, config: &Config, words: &[String], who: &Decider) -> Result<Outcome> {
    let intake = match words {
        [] => ready_intake(root, config)?,
        [id] => Intake::open(root, id)?,
        _ => bail!("usage: /handover [INTAKE]"),
    };
    let r = readiness::check(&intake, root, &[], config)?
        .with(readiness::runtime(root))
        .with_project(config);
    if !r.ready {
        bail!(
            "{} is not ready to hand over:\n{}",
            intake.id,
            readiness::render(&r).trim_end()
        );
    }
    let m = handover::create_as(&intake, root, &[], config, &r.files, who)?;
    let target = Target::Intake(intake);
    let mut out = outcome(&target);
    out.recorded = true;
    out.lines.push(format!(
        "✓ {} recorded: {} — {} file(s) pinned, baseline {} ({})",
        m.id,
        m.tasks.join(" → "),
        m.files.len(),
        m.baseline.branch,
        hash::short(&m.baseline.commit)
    ));
    after_decision(config, &target, &out);
    Ok(out)
}

fn outcome(target: &Target) -> Outcome {
    Outcome {
        lines: Vec::new(),
        recorded: false,
        intake: match target {
            Target::Intake(i) => Some(i.id.clone()),
            Target::Knowledge(_) => None,
        },
    }
}

/// Keep the commitments current after a decision on an intake, as the CLI
/// does; a failure is reported, not fatal.
fn after_decision(config: &Config, target: &Target, out: &Outcome) {
    if out.recorded && matches!(target, Target::Intake(_)) {
        if let Err(e) = crate::knowledge::commitments::write(config) {
            eprintln!("warning: commitments not updated: {e:#}");
        }
    }
}

/// The target the first word names, or the only one `fits`, with the
/// words left after it.
fn target_for<'w>(
    root: &Path,
    config: &Config,
    words: &'w [String],
    fits: impl Fn(&Target) -> Result<bool>,
) -> Result<(Target, &'w [String])> {
    if let Some((first, rest)) = words.split_first() {
        if let Some(t) = named(root, config, first) {
            return Ok((t, rest));
        }
    }
    let mut found = Vec::new();
    for t in targets(root, config) {
        if fits(&t)? {
            found.push(t);
        }
    }
    match found.len() {
        1 => Ok((found.remove(0), words)),
        0 => bail!("nothing is under review; send files with `zforge intake review`"),
        _ => {
            let names: Vec<String> = found.iter().map(Target::label).collect();
            bail!(
                "{} all have files under review; name one, e.g. `/accept {} all`",
                names.join(", "),
                names[0]
            )
        }
    }
}

fn named(root: &Path, config: &Config, word: &str) -> Option<Target> {
    if word == "knowledge" {
        return Some(Target::Knowledge(Knowledge::open(config)));
    }
    crate::intake::validate_id(word).ok()?;
    Intake::open(root, word).ok().map(Target::Intake)
}

fn targets(root: &Path, config: &Config) -> Vec<Target> {
    crate::status::intake_ids(root)
        .into_iter()
        .filter_map(|id| Intake::open(root, &id).ok())
        .map(Target::Intake)
        .chain(std::iter::once(Target::Knowledge(Knowledge::open(config))))
        .collect()
}

/// The file among `files` that `word` names: the path itself, with `.md`
/// left off, a task id, or a stage's number (`02`).
fn resolve_file(files: &[String], word: &str, target: &Target) -> Result<String> {
    let word = word.trim_end_matches(',');
    let names = |f: &str| -> bool {
        let stem = f.strip_suffix(".md").unwrap_or(f);
        f == word
            || stem == word
            || stem.strip_prefix("tasks/") == Some(word)
            || stem.strip_prefix("changes/") == Some(word)
            || stem
                .split_once('-')
                .is_some_and(|(n, _)| n == word && n.chars().all(|c| c.is_ascii_digit()))
    };
    let hits: Vec<&String> = files.iter().filter(|f| names(f)).collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(anyhow!(
            "{word} is not under review in {} (under review: {})",
            target.label(),
            if files.is_empty() {
                "nothing".to_string()
            } else {
                files.join(", ")
            }
        )),
        many => Err(anyhow!(
            "{word} matches several files: {}",
            many.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The only intake whose accepted files pass readiness and are not yet
/// handed over as they are. With a single intake, that intake even when it
/// is not ready, so the refusal says why.
fn ready_intake(root: &Path, config: &Config) -> Result<Intake> {
    let ids = crate::status::intake_ids(root);
    let mut ready = Vec::new();
    let mut handed_over = Vec::new();
    for id in &ids {
        let i = Intake::open(root, id)?;
        let r = readiness::check(&i, root, &[], config)?;
        match handover::list(&i)?.into_iter().last() {
            Some(m) if r.ready && m.files == r.files => handed_over.push((i.id.clone(), m.id)),
            _ if r.ready => ready.push(i),
            _ => {}
        }
    }
    match (ready.len(), ids.as_slice(), handed_over.as_slice()) {
        (1, ..) => Ok(ready.remove(0)),
        (0, [_], [(id, h)]) => bail!(
            "{id} is already handed over as it is ({h}); `/handover {id}` hands it over again"
        ),
        (0, [only], _) => Intake::open(root, only),
        (0, ..) => bail!("no intake is ready to hand over; `zforge readiness <ID>` says why"),
        _ => bail!(
            "{} are all ready; name one: `/handover {}`",
            ready
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            ready[0].id
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intake_target() -> Target {
        Target::Intake(Intake {
            id: "F".into(),
            dir: std::path::PathBuf::from("/nowhere"),
        })
    }

    #[test]
    fn a_file_is_named_the_short_ways() {
        let files: Vec<String> = [
            "01-outcome.md",
            "02-behavior.md",
            "tasks/TASK-003.md",
            "changes/CHANGE-RUN-009.md",
        ]
        .map(String::from)
        .to_vec();
        let t = intake_target();
        for (word, want) in [
            ("01-outcome.md", "01-outcome.md"),
            ("02-behavior", "02-behavior.md"),
            ("02", "02-behavior.md"),
            ("TASK-003", "tasks/TASK-003.md"),
            ("TASK-003,", "tasks/TASK-003.md"),
            ("tasks/TASK-003.md", "tasks/TASK-003.md"),
            ("CHANGE-RUN-009", "changes/CHANGE-RUN-009.md"),
        ] {
            assert_eq!(resolve_file(&files, word, &t).unwrap(), want, "{word}");
        }
        let err = resolve_file(&files, "03", &t).unwrap_err().to_string();
        assert!(err.contains("not under review in F"), "{err}");
    }
}
