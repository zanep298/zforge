//! Headless drafting and refreshing of the project's knowledge (ONBOARD
//! REQ-003, REQ-009, TASK-008): one Claude call per unit of work, the agent
//! unable to edit any file (AC-06), its answer parsed and written by
//! zforge, then sent for review like a hand edit — never accepted
//! automatically (REQ-005, D1). A failed, interrupted or over-budget call
//! leaves every knowledge file exactly as it was (Constraints, AC-05).
//!
//! **Scope of `draft`.** The contract's Output describes one call per
//! module writing "the module's section of the file" (singular) —
//! `domain.md`, the one knowledge file with per-module sections
//! (03-solution's format: `covers` and module headings are "domain.md
//! only"). Drafting `conventions.md` and `rules.md`, which are project-wide
//! rather than per-module, is the interactive `zforge-onboard` skill's job
//! (REQ-003); this command is the headless, per-module path the skill (and
//! CI) can call without a human answering open questions.
//!
//! **Isolation (Output: "in a worktree at HEAD").** The probe already
//! refuses onboarding on an uncommitted tree (`probe::run`), so a clean
//! tree *is* HEAD's content, and each call gets its own throwaway
//! `run::worktree` checked out at that commit — never the project's real
//! working tree. `--disallowedTools` keeps Edit, Write and NotebookEdit out
//! of the agent's hands, but that alone is not airtight: Bash (also
//! disallowed here, but a client bug or an unknown runner might not honour
//! it) could still write, delete or run arbitrary commands. The worktree
//! contains whatever a call does, and [`call_agent`] treats the call as
//! failed — whatever its exit code says — unless that worktree is clean
//! when the call ends; either way the worktree and its branch are removed
//! before the call returns, so nothing it did survives.

use super::items;
use super::{lint as klint, stale, Knowledge};
use crate::config::Config;
use crate::intake::lint as intake_lint;
use crate::orchestrator::{headless_args, spawn};
use crate::run::{git, worktree};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// The only runner drafting calls know how to talk to, same as a run's.
const RUNNER: &str = "claude";
/// Generous for an exploration-only call with no test suite to run.
const CALL_TIMEOUT_SECS: u64 = 900;
const MIN_BUDGET_USD: f64 = 0.01;
/// Never given to a drafting or refresh call (Output, AC-06): it may read
/// the code, never change it. Same as `review-agent`'s read-only tools,
/// plus `Bash` — a shell is how an agent without Edit/Write would otherwise
/// still write, delete or overwrite a file.
const DISALLOWED_TOOLS: &str = "Edit,Write,NotebookEdit,Bash";

/// What happened to one file a call touched.
#[derive(Debug)]
pub struct FileOutcome {
    pub file: String,
    /// `false` means nothing was written (Constraints, AC-05): the call
    /// failed, timed out, or went over budget.
    pub applied: bool,
    /// Why the call did not apply, when `applied` is `false`.
    pub reason: Option<String>,
    /// `Some(Ok(revision))` once sent for review; `Some(Err(message))` when
    /// the file was written but review refused it (AC-03, a lint error);
    /// `None` when nothing was written at all.
    pub review: Option<std::result::Result<u32, String>>,
}

impl FileOutcome {
    fn failed(file: &str, reason: impl Into<String>) -> Self {
        Self {
            file: file.to_string(),
            applied: false,
            reason: Some(reason.into()),
            review: None,
        }
    }

    /// Whether the caller should report this as an overall problem.
    pub fn ok(&self) -> bool {
        self.applied && matches!(self.review, Some(Ok(_)))
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub outcomes: Vec<FileOutcome>,
}

impl Report {
    pub fn all_ok(&self) -> bool {
        self.outcomes.iter().all(FileOutcome::ok)
    }
}

/// `zforge onboard draft [--module M] [--budget USD]` (Output).
pub fn draft(
    project_root: &Path,
    config: &Config,
    module: Option<&str>,
    budget_usd: Option<f64>,
) -> Result<Report> {
    ensure_clean(project_root)?;
    let k = Knowledge::open(config);
    let head = git::head(project_root)?;
    let budget = budget_usd.unwrap_or(config.knowledge.draft_budget_usd);
    let modules = modules_to_draft(&k, module)?;
    let mut outcomes = Vec::new();
    for m in modules {
        outcomes.push(draft_module(project_root, &k, &head, &m, budget)?);
    }
    Ok(Report { outcomes })
}

/// `zforge onboard refresh [--budget USD]` (Output): one call per file that
/// [`stale::check`] found stale items in, answering with replacements for
/// those IDs only.
pub fn refresh(project_root: &Path, config: &Config, budget_usd: Option<f64>) -> Result<Report> {
    ensure_clean(project_root)?;
    let k = Knowledge::open(config);
    let head = git::head(project_root)?;
    let budget = budget_usd.unwrap_or(config.knowledge.draft_budget_usd);
    let report = stale::check(&k)?;
    let mut by_file: BTreeMap<String, Vec<stale::StaleItem>> = BTreeMap::new();
    for s in report.stale {
        by_file.entry(s.file.clone()).or_default().push(s);
    }
    let mut outcomes = Vec::new();
    for (file, items) in by_file {
        outcomes.push(refresh_file(
            project_root,
            &k,
            &head,
            &file,
            &items,
            budget,
        )?);
    }
    Ok(Report { outcomes })
}

fn ensure_clean(project_root: &Path) -> Result<()> {
    if !git::is_clean(project_root)? {
        bail!(
            "the working tree has uncommitted changes; commit or stash them first — \
             drafting pins new evidence to a commit"
        );
    }
    Ok(())
}

fn modules_to_draft(k: &Knowledge, module: Option<&str>) -> Result<Vec<String>> {
    if let Some(m) = module {
        return Ok(vec![m.to_string()]);
    }
    let text = std::fs::read_to_string(k.dir.join(super::BASELINE_FILE))
        .map_err(|_| anyhow!("no baseline yet — run `zforge onboard` first"))?;
    let modules = klint::baseline_modules(&text);
    if modules.is_empty() {
        bail!("baseline.md lists no modules; pass --module <name>");
    }
    Ok(modules)
}

// ---------------------------------------------------------------- draft ---

fn draft_module(
    project_root: &Path,
    k: &Knowledge,
    head: &str,
    module: &str,
    budget: f64,
) -> Result<FileOutcome> {
    let path = k.dir.join("domain.md");
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let next_dom = next_id(&current, "DOM");
    let prompt = draft_prompt(module, head, next_dom);
    let call_id = format!("draft-{module}");
    let answer = match call_agent(project_root, head, "draft", &call_id, &prompt, budget)? {
        CallResult::Answered(a) => a,
        CallResult::Failed(reason) => {
            return Ok(FileOutcome::failed(
                "domain.md",
                format!("{module}: {reason}"),
            ))
        }
    };
    let parsed = parse_draft_answer(&answer.text);
    if parsed.items.is_empty() && parsed.open_questions.is_empty() {
        return Ok(FileOutcome::failed(
            "domain.md",
            format!("{module}: the agent's answer had no items or open questions"),
        ));
    }

    let mut text = set_scalar(&current, "pinned", head);
    text = add_covers(&text, module);
    if !parsed.items.is_empty() {
        text = insert_module_section(&text, module, &parsed.items);
    }
    if !parsed.open_questions.is_empty() {
        text = append_open_questions(&text, &parsed.open_questions);
    }
    std::fs::create_dir_all(&k.dir)?;
    crate::fs::write_atomic(&path, text.as_bytes())?;

    let review = super::review(project_root, k, "domain.md")
        .map(|r| r.revision)
        .map_err(|e| format!("{e:#}"));
    Ok(FileOutcome {
        file: "domain.md".to_string(),
        applied: true,
        reason: None,
        review: Some(review),
    })
}

struct DraftAnswer {
    items: Vec<String>,
    open_questions: Vec<String>,
}

/// The draft call's answer protocol (Autonomy: "how the agent's answer is
/// delimited"): plain `DOM-` item lines, then an optional "## Open
/// questions" section — nothing else is interpreted.
fn draft_prompt(module: &str, head: &str, next_dom: u32) -> String {
    format!(
        "You are drafting project knowledge for the module `{module}` at commit {head}.\n\
         Read the code under `{module}` (and elsewhere only to see how it is used) with the \
         tools you have; you have no editing tools and must not change any file, including \
         through the shell.\n\n\
         Answer with knowledge items for this module only — business rules, invariants, \
         workflows and states that are specific to this project, not general advice. Every \
         statement needs evidence; something the code cannot show goes under \"Open \
         questions\" instead of being asserted.\n\n\
         Answer in exactly this shape and nothing else:\n\n\
         - DOM-{next_dom}: <one complete sentence>. (path/to/file.ext:line[-line][, path:line...])\n\
         - DOM-{next}: <...>. (path:line)\n\n\
         ## Open questions\n\
         - [ ] <what the code cannot show, as a question> (path:line)\n\n\
         Use fresh, increasing numbers starting at DOM-{next_dom}. Leave out a line or the \
         whole \"Open questions\" section when it does not apply. One line per item — do not \
         wrap an item across lines.",
        next = next_dom + 1,
    )
}

fn parse_draft_answer(answer: &str) -> DraftAnswer {
    let mut items = Vec::new();
    let mut open_questions = Vec::new();
    let mut in_open_questions = false;
    for raw in answer.lines() {
        let line = raw.trim_end();
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(heading) = t.strip_prefix("## ") {
            in_open_questions = intake_lint::OPEN_QUESTIONS.matches(heading);
            continue;
        }
        if in_open_questions {
            open_questions.push(t.to_string());
        } else if items::line_id(t).is_some() {
            items.push(t.to_string());
        }
    }
    DraftAnswer {
        items,
        open_questions,
    }
}

// -------------------------------------------------------------- refresh ---

fn refresh_file(
    project_root: &Path,
    k: &Knowledge,
    head: &str,
    file: &str,
    stale_items: &[stale::StaleItem],
    budget: f64,
) -> Result<FileOutcome> {
    let path = k.dir.join(file);
    let text = std::fs::read_to_string(&path).with_context(|| format!("read {file}"))?;
    let ids: Vec<String> = stale_items.iter().map(|s| s.id.clone()).collect();
    let prompt = refresh_prompt(project_root, file, stale_items)?;
    let call_id = format!("refresh-{file}");
    let answer = match call_agent(project_root, head, "refresh", &call_id, &prompt, budget)? {
        CallResult::Answered(a) => a,
        CallResult::Failed(reason) => return Ok(FileOutcome::failed(file, reason)),
    };
    let answered = parse_refresh_answer(&answer.text);
    let wanted: std::collections::BTreeSet<&String> = ids.iter().collect();
    let got: std::collections::BTreeSet<&String> = answered.keys().collect();
    if wanted != got {
        return Ok(FileOutcome::failed(
            file,
            format!(
                "the agent's answer did not cover exactly the stale items {ids:?} (got {:?})",
                answered.keys().collect::<Vec<_>>()
            ),
        ));
    }

    let mut new_text = text;
    for id in &ids {
        new_text = items::replace_item(&new_text, id, &answered[id])
            .ok_or_else(|| anyhow!("{id} not found in {file} any more"))?;
    }
    new_text = set_scalar(&new_text, "pinned", head);
    crate::fs::write_atomic(&path, new_text.as_bytes())?;

    let review = super::review(project_root, k, file)
        .map(|r| r.revision)
        .map_err(|e| format!("{e:#}"));
    Ok(FileOutcome {
        file: file.to_string(),
        applied: true,
        reason: None,
        review: Some(review),
    })
}

fn refresh_prompt(project_root: &Path, file: &str, items: &[stale::StaleItem]) -> Result<String> {
    let mut s = format!(
        "You are refreshing stale items of `{file}` at HEAD. Each item below no longer matches \
         the code it cited, or that code is gone. You have no editing tools and must not \
         change any file, including through the shell.\n\n"
    );
    for it in items {
        let cite = if it.cite.start == it.cite.end {
            format!("{}:{}", it.cite.path, it.cite.start)
        } else {
            format!("{}:{}-{}", it.cite.path, it.cite.start, it.cite.end)
        };
        s.push_str(&format!(
            "- {} previously cited {cite} ({}).\n",
            it.id,
            match it.reason {
                stale::Reason::Changed => "the code there changed",
                stale::Reason::Gone => "the file is gone",
            }
        ));
        if let Ok(current) = std::fs::read_to_string(project_root.join(&it.cite.path)) {
            s.push_str(&format!(
                "  Current `{}` (working tree, at HEAD):\n```\n{}\n```\n",
                it.cite.path, current
            ));
        }
    }
    s.push_str(
        "\nAnswer with exactly one replacement line per item above, reusing the same ID, with \
         fresh evidence at HEAD — nothing else:\n\n- <ID>: <one complete sentence>. \
         (path:line[-line])\n\nOne line per item — do not wrap an item across lines.",
    );
    Ok(s)
}

fn parse_refresh_answer(answer: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for raw in answer.lines() {
        let t = raw.trim();
        if let Some(id) = items::line_id(t) {
            out.insert(id, t.to_string());
        }
    }
    out
}

// --------------------------------------------------------- shared calls ---

struct Answer {
    text: String,
}

/// [`call_agent`]'s outcome: a usable answer, or why there is none. The
/// `Failed` case is never a hard error — nothing about it is written to any
/// knowledge file (Constraints, AC-05); the caller reports the reason.
enum CallResult {
    Answered(Answer),
    Failed(String),
}

/// A valid [`worktree::create_on`] id derived from `call_id`, which may
/// contain characters (like a module path's `/`) that id is not: every
/// character outside `[A-Za-z0-9._-]` becomes `-`.
fn worktree_id(call_id: &str) -> String {
    let cleaned: String = call_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("onboard-{cleaned}")
}

/// Spawn one headless, edit-disabled agent call in its own throwaway
/// worktree at `head`; trace and cost it like a run's. The worktree (and
/// its branch) are gone again before this returns, whatever the outcome —
/// nothing the call did to it survives.
fn call_agent(
    project_root: &Path,
    head: &str,
    phase: &str,
    call_id: &str,
    prompt: &str,
    budget_usd: f64,
) -> Result<CallResult> {
    if budget_usd < MIN_BUDGET_USD {
        bail!("budget of ${budget_usd:.2} is too small for a call");
    }
    let registry = crate::registry::io::load()?;
    let base = registry
        .resolved_agent(RUNNER, project_root)
        .ok_or_else(|| anyhow!("runner `{RUNNER}` is not in the registry; run `zforge init`"))?;
    let mut spec = base;
    let mut args = headless_args::headless_args_for_agent(RUNNER);
    args.append(&mut spec.args);
    args.push("--disallowedTools".into());
    args.push(DISALLOWED_TOOLS.into());
    args.push("--max-budget-usd".into());
    args.push(format!("{budget_usd:.2}"));
    spec.args = args;

    let wt_id = worktree_id(call_id);
    let branch = format!("zforge/onboard/{wt_id}");
    let wt = worktree::create_on(project_root, &wt_id, &branch, head)
        .with_context(|| format!("create an isolated worktree for {call_id}"))?;

    let outcome = (|| -> Result<CallResult> {
        let out = spawn::spawn_agent_in(&spec, prompt, CALL_TIMEOUT_SECS, &wt.path)
            .with_context(|| format!("spawn the agent for {call_id}"))?;

        let trace = crate::trace::from_invocation(crate::trace::Invocation {
            task_id: call_id,
            phase,
            attempt: 1,
            runner: RUNNER,
            command: std::iter::once(spec.command.clone())
                .chain(spec.args.iter().cloned())
                .collect(),
            expected: crate::trace::Expected::default(),
            stdout: &out.stdout,
            stderr: &out.stderr,
            exit_code: out.exit_code,
            timed_out: out.timed_out,
            duration_ms: out.duration_ms,
        });
        if let Err(e) =
            crate::trace::log::append(&project_root.join(".zforge/knowledge/traces"), &trace)
        {
            eprintln!("warning: trace not recorded: {e:#}");
        }

        if out.timed_out {
            return Ok(CallResult::Failed(format!(
                "timed out after {CALL_TIMEOUT_SECS}s"
            )));
        }
        // Defense in depth (AC-06): `--disallowedTools` is not proof by
        // itself — treat the call as failed, whatever its exit code says,
        // unless the worktree it ran in is still exactly as it started.
        if !git::is_clean(&wt.path)
            .with_context(|| format!("check the worktree for {call_id} stayed clean"))?
        {
            return Ok(CallResult::Failed(
                "the agent left its worktree with uncommitted changes; a drafting or refresh \
                 call must not write, delete or otherwise change any file"
                    .to_string(),
            ));
        }
        let result = extract_result(&out.stdout);
        if out.exit_code != 0 {
            return Ok(CallResult::Failed(format!(
                "agent exited with {}{}",
                out.exit_code,
                result
                    .message
                    .as_deref()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            )));
        }
        if result.is_error {
            return Ok(CallResult::Failed(format!(
                "agent reported an error{}",
                result
                    .message
                    .as_deref()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            )));
        }
        if let Some(cost) = result.cost_usd {
            if cost > budget_usd + 1e-9 {
                return Ok(CallResult::Failed(format!(
                    "cost ${cost:.2} exceeded the ${budget_usd:.2} budget"
                )));
            }
        }
        Ok(CallResult::Answered(Answer { text: result.text }))
    })();

    if let Err(e) = worktree::remove(project_root, &wt, true) {
        eprintln!(
            "warning: could not remove onboarding worktree {}: {e:#}",
            wt.path.display()
        );
    }
    let _ = git::run(project_root, &["branch", "-D", branch.as_str()]);

    outcome
}

struct RawResult {
    text: String,
    cost_usd: Option<f64>,
    is_error: bool,
    message: Option<String>,
}

/// Pull the client's final answer out of a captured `--output-format
/// stream-json` run: the `result` event's `result` field, whichever way the
/// call ended. Unlike `trace::claude::analyze` (which keeps that field only
/// on an error, since a trace has no use for a successful run's whole
/// answer), drafting needs the text either way.
fn extract_result(stdout: &str) -> RawResult {
    for line in stdout.lines().rev() {
        let t = line.trim();
        if t.is_empty() || !t.starts_with('{') {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(t) else {
            continue;
        };
        if v.get("type").and_then(|x| x.as_str()) != Some("result") {
            continue;
        }
        let is_error = v
            .get("is_error")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let text = v
            .get("result")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let cost_usd = v.get("total_cost_usd").and_then(serde_json::Value::as_f64);
        let message = is_error.then(|| text.clone());
        return RawResult {
            text,
            cost_usd,
            is_error,
            message,
        };
    }
    RawResult {
        text: String::new(),
        cost_usd: None,
        is_error: true,
        message: Some("no result event in the agent's output".to_string()),
    }
}

// ------------------------------------------------------- text rewriting ---

/// The next unused number for `prefix-N` items already in `text`.
fn next_id(text: &str, prefix: &str) -> u32 {
    items::items(text)
        .iter()
        .filter_map(|it| it.id.strip_prefix(&format!("{prefix}-")))
        .filter_map(|n| n.parse::<u32>().ok())
        .max()
        .map_or(1, |n| n + 1)
}

/// Insert or replace a top-level `key: value` frontmatter line, leaving
/// every other line — including the rest of the frontmatter and the whole
/// body — byte-identical. Adds a frontmatter block when `text` has none.
fn set_scalar(text: &str, key: &str, value: &str) -> String {
    let (fm, body) = intake_lint::split_frontmatter(text);
    let mut lines: Vec<String> =
        fm.map_or_else(Vec::new, |fm| fm.lines().map(str::to_string).collect());
    let prefix = format!("{key}:");
    match lines
        .iter()
        .position(|l| l.trim_start().starts_with(&prefix))
    {
        Some(i) => lines[i] = format!("{key}: {value}"),
        None => lines.push(format!("{key}: {value}")),
    }
    format!("---\n{}\n---\n{body}", lines.join("\n"))
}

/// Add `module` to `domain.md`'s `covers` list (deduplicated), keeping
/// every other frontmatter line as it was.
fn add_covers(text: &str, module: &str) -> String {
    let mut modules = items::covers(text);
    if !modules.iter().any(|m| m == module) {
        modules.push(module.to_string());
    }
    set_scalar(text, "covers", &format!("[{}]", modules.join(", ")))
}

/// Insert `new_lines` under a `## {module}` heading, creating the heading
/// (just before "## Open questions", or at the end) when it does not exist
/// yet; when it does, the new lines are appended after whatever is already
/// there.
fn insert_module_section(text: &str, module: &str, new_lines: &[String]) -> String {
    let (_, body) = intake_lint::split_frontmatter(text);
    let prefix = &text[..text.len() - body.len()];
    let heading = format!("## {module}");
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    let mut inserted = false;
    while i < lines.len() {
        let line = lines[i];
        if !inserted && line.trim() == heading {
            out.push(line.to_string());
            i += 1;
            let mut block: Vec<String> = Vec::new();
            while i < lines.len() && !lines[i].starts_with("## ") {
                block.push(lines[i].to_string());
                i += 1;
            }
            // Insert before any trailing blank lines that separate this
            // section from the next heading, so those blank lines still
            // come right after the new content.
            let split = block
                .iter()
                .rposition(|l| !l.trim().is_empty())
                .map_or(0, |p| p + 1);
            out.extend(block[..split].iter().cloned());
            out.extend(new_lines.iter().cloned());
            out.extend(block[split..].iter().cloned());
            inserted = true;
            continue;
        }
        if !inserted {
            if let Some(title) = line.strip_prefix("## ") {
                if intake_lint::OPEN_QUESTIONS.matches(title) {
                    out.push(heading.clone());
                    out.extend(new_lines.iter().cloned());
                    out.push(String::new());
                    inserted = true;
                }
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    if !inserted {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
        out.push(heading);
        out.extend(new_lines.iter().cloned());
    }
    let mut new_body = out.join("\n");
    if body.ends_with('\n') {
        new_body.push('\n');
    }
    format!("{prefix}{new_body}")
}

/// Append lines under `domain.md`'s "## Open questions" section, creating
/// the section at the end when it does not exist.
fn append_open_questions(text: &str, lines: &[String]) -> String {
    let (_, body) = intake_lint::split_frontmatter(text);
    let prefix = &text[..text.len() - body.len()];
    let body_lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut inserted = false;
    let mut i = 0usize;
    while i < body_lines.len() {
        let line = body_lines[i];
        out.push(line.to_string());
        if !inserted {
            if let Some(title) = line.strip_prefix("## ") {
                if intake_lint::OPEN_QUESTIONS.matches(title) {
                    // Skip to the end of the file (whatever follows the
                    // heading), appending our lines right after it, ahead
                    // of what is already there.
                    for l in lines {
                        out.push(l.clone());
                    }
                    inserted = true;
                }
            }
        }
        i += 1;
    }
    if !inserted {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
        out.push("## Open questions".to_string());
        out.extend(lines.iter().cloned());
    }
    let mut new_body = out.join("\n");
    if body.ends_with('\n') {
        new_body.push('\n');
    }
    format!("{prefix}{new_body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_draft_answer_splits_items_and_open_questions() {
        let answer = "- DOM-001: a thing happens. (a/b.rs:1)\n\
                       - DOM-002: another. (a/b.rs:2)\n\n\
                       ## Open questions\n\
                       - [ ] is this right? (a/b.rs:3)\n";
        let parsed = parse_draft_answer(answer);
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(
            parsed.open_questions,
            vec!["- [ ] is this right? (a/b.rs:3)"]
        );
    }

    #[test]
    fn parse_draft_answer_ignores_prose_outside_items() {
        let answer = "Here is what I found:\n- DOM-001: a thing happens. (a/b.rs:1)\nThat's all.";
        let parsed = parse_draft_answer(answer);
        assert_eq!(parsed.items, vec!["- DOM-001: a thing happens. (a/b.rs:1)"]);
    }

    #[test]
    fn next_id_starts_at_one_and_continues_after_the_highest() {
        assert_eq!(next_id("", "DOM"), 1);
        let text = "## m\n- DOM-001: x. (a/b.rs:1)\n- DOM-003: y. (a/b.rs:2)\n";
        assert_eq!(next_id(text, "DOM"), 4);
    }

    #[test]
    fn set_scalar_adds_frontmatter_when_absent_and_only_changes_the_key() {
        let out = set_scalar("# Domain\n\nbody\n", "pinned", "abc123");
        assert_eq!(out, "---\npinned: abc123\n---\n# Domain\n\nbody\n");

        let with_fm = "---\npinned: old\ncovers: [x]\n---\nbody\n";
        let out2 = set_scalar(with_fm, "pinned", "new");
        assert_eq!(out2, "---\npinned: new\ncovers: [x]\n---\nbody\n");
    }

    #[test]
    fn add_covers_deduplicates_and_keeps_order() {
        let text = "---\ncovers: [a, b]\n---\nbody\n";
        assert_eq!(add_covers(text, "b"), text);
        assert_eq!(add_covers(text, "c"), "---\ncovers: [a, b, c]\n---\nbody\n");
    }

    #[test]
    fn insert_module_section_creates_a_new_heading_before_open_questions() {
        let text = "# Domain\n\n## Open questions\n";
        let out = insert_module_section(
            text,
            "internal/token",
            &["- DOM-001: x. (a/b.rs:1)".to_string()],
        );
        assert_eq!(
            out,
            "# Domain\n\n## internal/token\n- DOM-001: x. (a/b.rs:1)\n\n## Open questions\n"
        );
    }

    #[test]
    fn insert_module_section_appends_to_an_existing_heading() {
        let text = "# Domain\n\n## internal/token\n- DOM-001: x. (a/b.rs:1)\n\n## Open questions\n";
        let out = insert_module_section(
            text,
            "internal/token",
            &["- DOM-002: y. (a/b.rs:2)".to_string()],
        );
        assert_eq!(
            out,
            "# Domain\n\n## internal/token\n- DOM-001: x. (a/b.rs:1)\n- DOM-002: y. (a/b.rs:2)\n\n## Open questions\n"
        );
    }

    #[test]
    fn append_open_questions_adds_after_the_heading() {
        let text = "# Domain\n\n## Open questions\n- [ ] old one\n";
        let out = append_open_questions(text, &["- [ ] new one (a/b.rs:5)".to_string()]);
        assert_eq!(
            out,
            "# Domain\n\n## Open questions\n- [ ] new one (a/b.rs:5)\n- [ ] old one\n"
        );
    }
}
