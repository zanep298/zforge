//! The one terminal confirmation behind a user's decision at the CLI —
//! `zforge intake accept|revise` and `zforge onboard accept|revise` (D1).
//! Both need an interactive terminal and a typed word; this is written
//! once so their refusal and prompt texts stay identical instead of
//! drifting apart as each command grows its own copy.

use anyhow::{bail, Result};
use std::io::{BufRead, IsTerminal, Write};

/// Refuse unless both stdin and stdout are an interactive terminal.
/// `command` names the invocation for the message, e.g.
/// `"zforge intake accept"`.
pub fn require_terminal(command: &str) -> Result<()> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!(
            "`{command}` records the user's decision and needs an interactive terminal; \
             it cannot be run by an agent or from a script"
        );
    }
    Ok(())
}

/// Print the confirmation prompt, read one line from stdin, and refuse
/// unless it is exactly `word`.
pub fn type_to_confirm(word: &str) -> Result<()> {
    print!("Type `{word}` to confirm: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if answer.trim() != word {
        bail!("not confirmed; nothing recorded");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test` runs with neither stdin nor stdout attached to a
    /// terminal, so this is always the refused path — exactly the path an
    /// agent takes when it runs the binary directly (see the binary-level
    /// coverage in `tests/onboard_test.rs` and `tests/intake_test.rs`).
    #[test]
    fn require_terminal_names_the_command_and_refuses_without_a_tty() {
        let err = require_terminal("zforge intake accept")
            .unwrap_err()
            .to_string();
        assert!(err.contains("zforge intake accept"), "{err}");
        assert!(err.contains("needs an interactive terminal"), "{err}");
        assert!(
            err.contains("cannot be run by an agent or from a script"),
            "{err}"
        );
    }
}
