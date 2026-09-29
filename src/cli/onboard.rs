//! `zforge onboard …` — review and decide on the project's own knowledge
//! files (ONBOARD TASK-001). `accept` and `revise` need an interactive
//! terminal and a typed confirmation, exactly like `zforge intake accept`
//! / `revise` (D1): an agent can prepare and review, never decide.

use crate::cli::confirm;
use crate::config;
use crate::intake::hash;
use crate::intake::lint;
use crate::intake::status::DocState;
use crate::knowledge::{self, known, probe, Knowledge};
use anyhow::{anyhow, bail, Result};
use clap::Subcommand;
use colored::Colorize;

#[derive(Debug, Subcommand)]
pub enum OnboardCmd {
    /// Every knowledge file's review state and open questions.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Send a knowledge file's current content for review as a new revision.
    Review { file: String },
    /// Accept the revision under review. Interactive terminal only.
    Accept { file: String },
    /// Ask for changes to the revision under review. Interactive terminal only.
    Revise {
        file: String,
        /// What needs to change.
        #[arg(long)]
        note: String,
    },
    /// Record or clear the baseline's known-failure list. Interactive
    /// terminal only (D1).
    Baseline {
        /// Comma-separated test names to record as known failures.
        /// Replaces the list in force; it does not add to it.
        #[arg(long)]
        known: Option<String>,
        /// Clear the known-failure list.
        #[arg(long)]
        clear: bool,
    },
}

/// `cmd = None` is bare `zforge onboard`: probe the project (Output).
pub fn run(cmd: Option<OnboardCmd>) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let root = config.project_root();
    let k = Knowledge::open(&config);
    match cmd {
        None => run_probe(&config),
        Some(OnboardCmd::Status { json }) => status(&k, json),
        Some(OnboardCmd::Review { file }) => send_for_review(&root, &k, &file),
        Some(OnboardCmd::Accept { file }) => decide(&k, &file, None),
        Some(OnboardCmd::Revise { file, note }) => decide(&k, &file, Some(&note)),
        Some(OnboardCmd::Baseline { known, clear }) => baseline(&k, known, clear),
    }
}

/// Probe the project (no model) and print what it found and the next
/// step (Output). Refuses on an uncommitted working tree (AC-03); a red
/// or missing test command is reported, not an error (AC-04).
fn run_probe(config: &config::Config) -> Result<()> {
    let report = probe::run(config)?;
    for f in &report.created_files {
        println!(
            "{} docs/knowledge/{f} — created (draft it, then `zforge onboard review {f}`)",
            "✓".green()
        );
    }
    let b = &report.baseline;
    println!(
        "commit     {} ({}{})",
        hash::short(&b.commit),
        b.branch,
        if b.clean { ", clean" } else { "" }
    );
    println!("language   {}; {}", b.language, b.toolchain);
    println!(
        "tracked    {} files, {} lines",
        b.tracked_files, b.tracked_lines
    );
    println!(
        "docs       {}",
        if b.docs.is_empty() {
            "none found".to_string()
        } else {
            b.docs.join(", ")
        }
    );
    println!(
        "codegraph  {}",
        if b.codegraph {
            "indexed"
        } else {
            "not indexed"
        }
    );
    if b.result {
        println!(
            "{} baseline   {} → PASS in {:.1}s",
            "✓".green(),
            b.test_command,
            b.duration_secs
        );
    } else if !b.failing.is_empty() {
        println!(
            "{} baseline   {} → FAIL in {:.1}s: {}",
            "✗".red(),
            b.test_command,
            b.duration_secs,
            b.failing.join(", ")
        );
    } else {
        println!(
            "{} baseline   {} → RED: {}",
            "✗".red(),
            b.test_command,
            b.reason.as_deref().unwrap_or("unknown reason")
        );
    }
    println!();
    let state = crate::onboard::state(config)?;
    if state.onboarded {
        println!("{} onboarded", "✓".green());
    } else {
        println!(
            "not onboarded yet — next: `zforge onboard status` or `zforge onboard review <file>`"
        );
        if !b.result && !b.failing.is_empty() {
            let unknown: Vec<&String> = b
                .failing
                .iter()
                .filter(|t| !state.baseline.known.contains(t))
                .collect();
            if !unknown.is_empty() {
                println!(
                    "  {} unrecorded failing test(s): {} — record with `zforge onboard baseline --known {}`",
                    unknown.len(),
                    unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                    unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
                );
            }
        }
    }
    Ok(())
}

/// `zforge onboard baseline --known <tests>` / `--clear`. Interactive
/// terminal and a typed confirmation, like `accept`/`revise` (D1).
fn baseline(k: &Knowledge, known_arg: Option<String>, clear: bool) -> Result<()> {
    match (&known_arg, clear) {
        (Some(_), true) => bail!("pass either --known <tests> or --clear, not both"),
        (None, false) => bail!("pass --known <tests> (comma-separated) or --clear"),
        _ => {}
    }
    confirm::require_terminal("zforge onboard baseline")?;
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    if clear {
        println!("{}", "CLEAR the known-failure list".bold());
        confirm::type_to_confirm("clear")?;
        known::clear(k, by)?;
        println!("{} known-failure list cleared", "✓".green());
    } else {
        let tests: Vec<String> = known_arg
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if tests.is_empty() {
            bail!("--known needs at least one test name");
        }
        println!(
            "{} known-failure list: {}",
            "RECORD".bold(),
            tests.join(", ")
        );
        confirm::type_to_confirm("known")?;
        let recorded = known::record_known(k, &tests, by)?;
        println!(
            "{} known-failure list is now: {}",
            "✓".green(),
            recorded.join(", ")
        );
    }
    Ok(())
}

fn send_for_review(root: &std::path::Path, k: &Knowledge, rel: &str) -> Result<()> {
    let r = knowledge::review(root, k, rel)?;
    for w in &r.warnings {
        eprintln!("{} {}: {}", "⚠".yellow(), w.file, w.message);
    }
    if r.unchanged {
        println!(
            "{rel} revision {} is already under review ({})",
            r.revision,
            hash::short(&r.sha256)
        );
    } else {
        println!(
            "{} {rel} sent for review as revision {} ({})",
            "✓".green(),
            r.revision,
            hash::short(&r.sha256)
        );
        match &r.diff {
            Some(d) if !d.trim().is_empty() => {
                println!("\nChanges since the accepted revision:\n{d}")
            }
            Some(_) => {}
            None => println!("No accepted revision yet to compare with."),
        }
    }
    println!(
        "The user decides at a terminal: `zforge onboard accept {rel}` or `zforge onboard revise {rel} --note …`"
    );
    Ok(())
}

/// Accept (`note = None`) or request changes, after a human confirms.
fn decide(k: &Knowledge, rel: &str, note: Option<&str>) -> Result<()> {
    let (verb, word) = match note {
        None => ("accept", "accept"),
        Some(_) => ("request changes to", "revise"),
    };
    confirm::require_terminal(&format!("zforge onboard {word}"))?;
    let pending = knowledge::pending_review(k, rel)?;
    println!(
        "{} knowledge/{rel} revision {} ({})",
        verb.to_uppercase().bold(),
        pending.revision,
        hash::short(&pending.sha256)
    );
    if let Some(n) = note {
        println!("note: {n}");
    }
    confirm::type_to_confirm(word)?;
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    let rev = match note {
        None => knowledge::accept(k, rel, by)?,
        Some(n) => knowledge::revise(k, rel, by, n)?,
    };
    println!(
        "{} recorded for {rel} revision {}",
        "✓".green(),
        rev.revision
    );
    Ok(())
}

#[derive(serde::Serialize)]
struct FileView {
    #[serde(flatten)]
    status: crate::intake::status::DocStatus,
    open_questions: Vec<String>,
    issues: Vec<lint::Issue>,
}

/// `zforge onboard status`'s full view: each file, plus every stale and
/// moved citation among the accepted files (ONBOARD REQ-009, TASK-004,
/// Output: "onboard status lists stale items and moved citations").
#[derive(serde::Serialize)]
struct StatusView {
    files: Vec<FileView>,
    stale: Vec<crate::knowledge::stale::StaleItem>,
    moved: Vec<crate::knowledge::stale::Moved>,
}

fn status(k: &Knowledge, json: bool) -> Result<()> {
    let views: Vec<FileView> = knowledge::statuses(k)?
        .into_iter()
        .map(|status| {
            let text = std::fs::read_to_string(k.dir.join(&status.file)).unwrap_or_default();
            FileView {
                open_questions: lint::open_questions(&text),
                issues: knowledge::lint_issues(&status.file, &text),
                status,
            }
        })
        .collect();
    let stale_report = knowledge::stale::check(k)?;
    if json {
        let view = StatusView {
            files: views,
            stale: stale_report.stale,
            moved: stale_report.moved,
        };
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }
    println!("{}  ({})", "knowledge".bold(), k.dir.display());
    for v in &views {
        let s = &v.status;
        let state = match s.state {
            DocState::Accepted if s.has_draft => "accepted, draft pending".yellow(),
            DocState::Accepted => "accepted".green(),
            DocState::InReview => "in review".cyan(),
            DocState::ChangedSinceReview => "changed since review".red(),
            DocState::NeedsRevision => "needs revision".red(),
            DocState::Draft => "draft".normal(),
        };
        let rev = s
            .accepted
            .as_ref()
            .map(|a| format!(" rev {}", a.revision))
            .unwrap_or_default();
        println!("  {:<24} {state}{rev}", s.file);
        if let Some(n) = &s.revision_note {
            println!("      requested: {n}");
        }
        for q in &v.open_questions {
            println!("      ? {q}");
        }
        for issue in &v.issues {
            let mark = match issue.severity {
                lint::Severity::Error => "✗".red(),
                lint::Severity::Warning => "⚠".yellow(),
            };
            println!("      {mark} {}", issue.message);
        }
    }
    if !stale_report.stale.is_empty() {
        println!("{}", "stale:".bold());
        for s in &stale_report.stale {
            let reason = match s.reason {
                crate::knowledge::stale::Reason::Changed => "changed",
                crate::knowledge::stale::Reason::Gone => "gone",
            };
            println!("  {} {}: {} — {reason}", s.id, s.file, cite_label(&s.cite));
        }
    }
    if !stale_report.moved.is_empty() {
        println!("{}", "moved:".bold());
        for m in &stale_report.moved {
            println!(
                "  {} {}: {} → {}-{}",
                m.id,
                m.file,
                cite_label(&m.cite),
                m.new_start,
                m.new_end
            );
        }
    }
    Ok(())
}

fn cite_label(cite: &crate::knowledge::items::Cite) -> String {
    if cite.start == cite.end {
        format!("{}:{}", cite.path, cite.start)
    } else {
        format!("{}:{}-{}", cite.path, cite.start, cite.end)
    }
}
