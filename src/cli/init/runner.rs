//! Pick the project's default runner at init (FIX-015).
//!
//! Before this, `init --agent codex` still seeded a `claude` registry entry
//! and the dispatcher hardcoded `claude` as the runner for tasks without
//! `--agent`, so a codex-only project quietly ran every agentless phase
//! through Claude. The choice is now made once, here, and written to
//! `runner.default` in `.zforge/config.yaml`; dispatch reads it from there.

use anyhow::{bail, Result};

/// Preference order when several clients are scaffolded (`--agent all`).
pub(crate) const RUNNER_ORDER: [&str; 3] = ["claude", "codex", "opencode"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunnerChoice {
    /// `--default-runner` named it.
    Explicit(String),
    /// Only one client was scaffolded.
    OnlyTarget(String),
    /// Several targets; the first in `RUNNER_ORDER` found on `$PATH`.
    FirstOnPath(String),
    /// Several targets and none on `$PATH`; first in `RUNNER_ORDER`.
    NoneOnPath(String),
}

impl RunnerChoice {
    pub fn name(&self) -> &str {
        match self {
            Self::Explicit(n)
            | Self::OnlyTarget(n)
            | Self::FirstOnPath(n)
            | Self::NoneOnPath(n) => n,
        }
    }

    pub fn explain(&self) -> String {
        match self {
            Self::Explicit(n) => format!("{n} (from --default-runner)"),
            Self::OnlyTarget(n) => format!("{n} (the only scaffolded client)"),
            Self::FirstOnPath(n) => format!(
                "{n} (first of {} found on PATH; override with --default-runner)",
                RUNNER_ORDER.join(" → ")
            ),
            Self::NoneOnPath(n) => format!(
                "{n} (none of the scaffolded clients is on PATH; install it or pass --default-runner)"
            ),
        }
    }
}

/// `targets` are the clients being scaffolded, in any order. `on_path`
/// reports whether a client's binary is installed (injected for tests).
pub(crate) fn resolve_default_runner(
    targets: &[&str],
    explicit: Option<&str>,
    on_path: impl Fn(&str) -> bool,
) -> Result<RunnerChoice> {
    if targets.is_empty() {
        bail!("no client scaffolded — cannot choose a default runner");
    }
    if let Some(name) = explicit {
        let name = name.to_ascii_lowercase();
        if !targets.contains(&name.as_str()) {
            bail!(
                "--default-runner {name} is not one of the clients being set up ({}). \
                 Add it with --agent {name} or --agent all.",
                targets.join(", ")
            );
        }
        return Ok(RunnerChoice::Explicit(name));
    }
    if let [only] = targets {
        return Ok(RunnerChoice::OnlyTarget((*only).to_string()));
    }
    let ordered: Vec<&str> = RUNNER_ORDER
        .iter()
        .copied()
        .filter(|r| targets.contains(r))
        .collect();
    if let Some(found) = ordered.iter().find(|r| on_path(r)) {
        return Ok(RunnerChoice::FirstOnPath((*found).to_string()));
    }
    Ok(RunnerChoice::NoneOnPath(ordered[0].to_string()))
}

/// True when `bin` resolves on `$PATH`.
pub(crate) fn on_path(bin: &str) -> bool {
    which::which(bin).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 3] = ["claude", "codex", "opencode"];

    #[test]
    fn single_target_is_the_runner() {
        let c = resolve_default_runner(&["codex"], None, |_| false).unwrap();
        assert_eq!(c, RunnerChoice::OnlyTarget("codex".into()));
    }

    #[test]
    fn all_picks_first_installed_in_order() {
        let c = resolve_default_runner(&ALL, None, |b| b != "claude").unwrap();
        assert_eq!(c, RunnerChoice::FirstOnPath("codex".into()));
        let c = resolve_default_runner(&ALL, None, |b| b == "opencode").unwrap();
        assert_eq!(c, RunnerChoice::FirstOnPath("opencode".into()));
        let c = resolve_default_runner(&ALL, None, |_| true).unwrap();
        assert_eq!(c, RunnerChoice::FirstOnPath("claude".into()));
    }

    #[test]
    fn order_does_not_depend_on_target_order() {
        let c = resolve_default_runner(&["opencode", "codex"], None, |_| true).unwrap();
        assert_eq!(c.name(), "codex");
    }

    #[test]
    fn none_installed_falls_back_to_first_in_order_and_says_so() {
        let c = resolve_default_runner(&ALL, None, |_| false).unwrap();
        assert_eq!(c, RunnerChoice::NoneOnPath("claude".into()));
        assert!(c.explain().contains("none of the scaffolded clients"));
    }

    #[test]
    fn explicit_wins_over_detection() {
        let c = resolve_default_runner(&ALL, Some("OpenCode"), |_| true).unwrap();
        assert_eq!(c, RunnerChoice::Explicit("opencode".into()));
    }

    #[test]
    fn explicit_must_be_a_scaffolded_target() {
        let err = resolve_default_runner(&["codex"], Some("claude"), |_| true).unwrap_err();
        assert!(err.to_string().contains("--agent claude"), "{err}");
    }
}
