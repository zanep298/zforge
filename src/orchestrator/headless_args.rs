//! Per-agent CLI flags that disable interactive confirmation prompts.
//!
//! When zforge runs an agent **headless** (background worker spawned by
//! `ship --async`), no human is around to answer `Allow write to foo.rs?
//! [y/n]`. Without a bypass flag the child blocks on stdin forever, the
//! worker hangs, and the job stays `running` until manually cancelled.
//!
//! Foreground mode (`zforge ship T1` without `--async`, or `zforge code
//! T1`) does NOT inject bypass flags — a user is present to answer prompts.
//!
//! Each agent's bypass flags are documented in `docs/agent-contracts.md`.

/// Return the bypass args for the given agent. Empty when we don't know the
/// agent — user is expected to bake equivalent flags into their registered
/// `AgentSpec.args`.
pub fn headless_args_for_agent(agent_name: &str) -> Vec<String> {
    match agent_name {
        // Claude Code: --dangerously-skip-permissions allows Edit / Write /
        // Bash without per-call confirmation. Without it, claude blocks on
        // stdin waiting for `y` even in `-p` print mode.
        "claude" => vec!["--dangerously-skip-permissions".into()],

        // Codex: must run via `exec` subcommand (the bare `codex` opens TUI).
        // `exec` accepts the prompt + needs:
        //   `-a never` → never escalate to human approval
        //   `-s workspace-write` → allow writes inside the project workspace
        // Could also use `--dangerously-bypass-approvals-and-sandbox` for
        // full bypass but workspace-write is the safer default.
        "codex" => vec![
            "-a".into(),
            "never".into(),
            "-s".into(),
            "workspace-write".into(),
        ],

        // OpenCode / agy / unknown: no known bypass conventions — user
        // configures via registered AgentSpec.args.
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_gets_skip_permissions() {
        let args = headless_args_for_agent("claude");
        assert!(args.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn codex_gets_never_approval_and_workspace_sandbox() {
        let args = headless_args_for_agent("codex");
        assert!(args.iter().any(|a| a == "-a"));
        assert!(args.iter().any(|a| a == "never"));
        assert!(args.iter().any(|a| a == "-s"));
        assert!(args.iter().any(|a| a == "workspace-write"));
    }

    #[test]
    fn opencode_returns_empty_for_user_to_configure() {
        assert!(headless_args_for_agent("opencode").is_empty());
    }

    #[test]
    fn unknown_agent_returns_empty() {
        assert!(headless_args_for_agent("mystery").is_empty());
    }
}
