//! Claude Code's `UserPromptSubmit` hook: the user's decisions typed in
//! chat (D1, second channel).
//!
//! `zforge init` registers `zforge hook prompt` for Claude. Claude Code runs
//! it on every message the user sends, with the literal text, before the
//! model sees it; the model cannot produce such a message. When the text
//! is `/accept`, `/revise` or `/handover` ([`command`]), zforge records the
//! decision itself ([`decide`]) through the `claude-prompt` channel, then
//! tells the user what it recorded (`systemMessage`) and the model the
//! same (`additionalContext`), so the conversation carries on from it. Any
//! other message passes through untouched.
//!
//! The model is denied `zforge hook` in `.claude/settings.json`, like the
//! terminal decisions. As with the terminal's check, this keeps an agent
//! from deciding by mistake or zeal; it is not a defence against code
//! running as the user.

pub mod command;
pub mod decide;

use crate::intake::record::Decider;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// What Claude Code sends a `UserPromptSubmit` hook (the fields zforge
/// reads).
#[derive(Debug, Deserialize)]
pub struct Input {
    pub hook_event_name: String,
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

/// The hook's answer for `input`: `None` when the message is not a
/// decision (print nothing; Claude Code passes it on).
pub fn handle(input: &Input, by: Option<String>) -> Option<Value> {
    if input.hook_event_name != "UserPromptSubmit" {
        return None;
    }
    let parsed = command::parse(&input.prompt)?;
    let cwd = input.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let result = parsed
        .map_err(anyhow::Error::msg)
        .and_then(|cmd| record(&cwd, &cmd, by, input.session_id.clone()).map(|o| (cmd, o)));
    Some(match result {
        Ok((cmd, outcome)) => answer(&input.prompt, &cmd, &outcome, &cwd),
        Err(e) => refused(&input.prompt, &format!("{e:#}")),
    })
}

fn record(
    cwd: &Path,
    cmd: &command::Command,
    by: Option<String>,
    session: Option<String>,
) -> anyhow::Result<decide::Outcome> {
    let config_path = crate::config::find_config_from(cwd)
        .ok_or_else(|| anyhow::anyhow!("not in a zforge project (no .zforge/config.yaml)"))?;
    let config = crate::config::load_from(&config_path)?;
    let root = config.project_root();
    decide::run(&root, &config, cmd, &Decider::prompt(by, session))
}

fn answer(prompt: &str, cmd: &command::Command, outcome: &decide::Outcome, cwd: &Path) -> Value {
    let lines = outcome.lines.join("\n");
    let next = outcome
        .intake
        .as_deref()
        .and_then(|id| next_step(cwd, id))
        .map(|n| format!("\nNext for this intake: {n}"))
        .unwrap_or_default();
    let context = format!(
        "The user typed `{}` and zforge's prompt hook handled it before this \
         message reached you — you did not record it, and must never run \
         `zforge intake accept|revise`, `zforge handover` or `zforge hook` \
         yourself.\n{lines}{next}\n{}",
        prompt.trim(),
        if outcome.recorded {
            "Carry on from what was recorded (after a handover: start it with `run_start`)."
        } else {
            "Nothing was recorded: tell the user why, and what to fix."
        }
    );
    output(&format!("zforge {}:\n{lines}", cmd.name()), &context)
}

fn refused(prompt: &str, why: &str) -> Value {
    output(
        &format!("zforge: nothing recorded — {why}"),
        &format!(
            "The user typed `{}`; zforge's prompt hook recorded nothing: {why}. \
             Tell the user, and help them fix it. Never record the decision yourself.",
            prompt.trim()
        ),
    )
}

fn output(user: &str, model: &str) -> Value {
    json!({
        "systemMessage": user,
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": model,
        }
    })
}

fn next_step(cwd: &Path, intake: &str) -> Option<String> {
    let config_path = crate::config::find_config_from(cwd)?;
    let config = crate::config::load_from(&config_path).ok()?;
    let status = crate::status::project(&config.project_root()).ok()?;
    status
        .intakes
        .into_iter()
        .find(|i| i.id == intake)
        .map(|i| i.next)
}
