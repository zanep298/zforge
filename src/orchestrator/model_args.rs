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
//! Codex returns an empty arg list because its phase-specific config lives
//! in `~/.codex/config.toml` profiles named `zforge_<phase>`; the profile is
//! selected by `--profile zforge_<phase>` which the user adds to the registry
//! `AgentSpec.args` directly. We do NOT auto-inject `--model X` for Codex —
//! that would conflict with its profile semantics.
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
}
