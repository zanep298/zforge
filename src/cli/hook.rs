//! `zforge hook prompt` — Claude Code's `UserPromptSubmit` hook (see
//! [`crate::hook`]). Registered by `zforge init`; never run by hand.

use anyhow::{Context, Result};
use std::io::Read;

/// Read the hook's input on stdin and print its answer, if any. A message
/// that is not a decision prints nothing, so Claude Code passes it on
/// unchanged; input that is not a `UserPromptSubmit` event is ignored the
/// same way rather than failing the user's message.
pub fn prompt() -> Result<()> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .context("read the hook input")?;
    let Ok(input) = serde_json::from_str::<crate::hook::Input>(&raw) else {
        return Ok(());
    };
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    if let Some(answer) = crate::hook::handle(&input, by) {
        println!("{answer}");
    }
    Ok(())
}
