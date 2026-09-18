//! Human-readable progress output sink.
//!
//! zforge's CLI commands double as the implementation behind the MCP tools.
//! On the CLI, progress text belongs on stdout. Under MCP, stdout is the
//! JSON-RPC transport — a single stray `println!` corrupts the frame and the
//! client either drops the response or kills the server.
//!
//! Every command therefore emits progress through [`note!`] instead of
//! `println!`. The MCP server calls [`divert_to_stderr`] once at startup, after
//! which the same code paths write to stderr and leave stdout clean for
//! protocol frames.
//!
//! This is deliberately a process-global switch rather than a threaded-through
//! writer: the sink is a property of how the process was launched, not of any
//! individual call, and passing a writer through every `cli::*` signature would
//! be a much larger change for no extra safety.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

static DIVERTED: AtomicBool = AtomicBool::new(false);

/// Route all subsequent [`note!`] output to stderr. Called by the MCP server
/// before it starts reading requests. Not reversible — a process is either a
/// CLI invocation or an MCP server, never both.
pub fn divert_to_stderr() {
    DIVERTED.store(true, Ordering::SeqCst);
}

/// True when progress output is going to stderr.
pub fn is_diverted() -> bool {
    DIVERTED.load(Ordering::SeqCst)
}

/// Write one line to the active sink. Errors are swallowed: a broken pipe on
/// progress output must not fail the operation that was reporting progress.
pub fn emit(args: std::fmt::Arguments<'_>) {
    if is_diverted() {
        let _ = writeln!(std::io::stderr(), "{args}");
    } else {
        let _ = writeln!(std::io::stdout(), "{args}");
    }
}

/// `println!` for progress output that must not collide with the MCP
/// JSON-RPC stream on stdout. See [module docs](self).
#[macro_export]
macro_rules! note {
    () => {
        $crate::cli::output::emit(format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::cli::output::emit(format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // `DIVERTED` is process-global, so this test only asserts the default.
    // Flipping it here would leak into every other test in the binary; the
    // diverted path is covered end-to-end by `tests/mcp_stdout_test.rs`,
    // which runs a real MCP server subprocess.
    #[test]
    fn defaults_to_stdout() {
        assert!(!is_diverted());
    }
}
