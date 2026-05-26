//! Per-agent CLI flag conventions for selecting a model.
//!
//! `models.yaml` stores a model identifier per (assistant, phase). Different
//! AI coding agents accept that identifier via different flags:
//!
//! | Agent     | Flag syntax           | Source of truth          |
//! |-----------|----------------------|--------------------------|
//! | claude    | `--model <name>`      | Anthropic Claude Code    |
//! | opencode  | `--model <name>`      | OpenCode CLI             |
//! | codex     | (profile-based)       | `~/.codex/config.toml`   |
//! | agy       | `--model <name>`      | Treated as standard CLI  |
//! | (other)   | (no injection)        | User must hand-craft args|
//!
//! Codex returns an empty `--model` arg list because its phase-specific
//! config lives in `~/.codex/config.toml` profiles named `zforge_<phase>`.
//! The profile is selected via `--profile zforge_<phase>`, which the
//! orchestrator injects per-phase through `profile_args_for_agent` (called
//! by `with_profile_args` in `run.rs`). We do NOT auto-inject `--model X`
//! for codex — that would conflict with its profile semantics.
//!
//! Unknown agent names fall through with no injection so users can register
//! custom agents whose CLI we don't recognize. They wire model selection into
//! `AgentSpec.args` themselves.

/// Build the `--model X` (or equivalent) args for a given agent + model name.
/// Returns an empty vec when the agent doesn't accept zforge-managed model
/// selection (Codex profile-based, or unknown agent).
pub fn model_args_for_agent(agent_name: &str, model: &str) -> Vec<String> {
    match agent_name {
        "claude" | "opencode" | "agy" => vec!["--model".into(), model.into()],
        "codex" => vec![],
        _ => vec![],
    }
}

/// Per-phase profile-selection args. Currently only codex uses profile
/// routing: `--profile zforge_<phase>` is prepended at spawn time so each
/// phase picks up the right `[profiles.zforge_<phase>]` block from
/// `~/.codex/config.toml`. Codex needs the flag BEFORE its `exec`
/// subcommand, which is why callers prepend rather than append.
///
/// Returns an empty vec for agents that don't use profile routing.
pub fn profile_args_for_agent(agent_name: &str, phase: &str) -> Vec<String> {
    match agent_name {
        "codex" => vec!["--profile".into(), format!("zforge_{phase}")],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_gets_model_flag() {
        assert_eq!(
            model_args_for_agent("claude", "opus"),
            vec!["--model".to_string(), "opus".to_string()]
        );
    }

    #[test]
    fn opencode_gets_model_flag() {
        assert_eq!(
            model_args_for_agent("opencode", "sonnet"),
            vec!["--model".to_string(), "sonnet".to_string()]
        );
    }

    #[test]
    fn agy_gets_model_flag() {
        assert_eq!(
            model_args_for_agent("agy", "haiku"),
            vec!["--model".to_string(), "haiku".to_string()]
        );
    }

    #[test]
    fn codex_skips_injection() {
        // Codex relies on `~/.codex/config.toml` profiles, not a CLI flag.
        assert!(model_args_for_agent("codex", "opus").is_empty());
    }

    #[test]
    fn unknown_agent_skips_injection() {
        // User-registered custom agent whose CLI we don't recognize. They
        // must bake model selection into AgentSpec.args themselves.
        assert!(model_args_for_agent("mystery", "opus").is_empty());
    }

    #[test]
    fn codex_gets_profile_args() {
        assert_eq!(
            profile_args_for_agent("codex", "plan"),
            vec!["--profile".to_string(), "zforge_plan".to_string()]
        );
    }

    #[test]
    fn claude_has_no_profile_args() {
        assert!(profile_args_for_agent("claude", "plan").is_empty());
    }
}
