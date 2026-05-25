use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    #[serde(default)]
    pub current_project: Option<String>,
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSpec>,
    #[serde(default)]
    pub fallback_policy: FallbackPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "Utc::now")]
    pub registered_at: DateTime<Utc>,
    #[serde(default = "default_registered_by")]
    pub registered_by: RegisteredBy,
    #[serde(default)]
    pub agent_overrides: BTreeMap<String, AgentSpec>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RegisteredBy {
    Init,
    Manual,
}

fn default_registered_by() -> RegisteredBy {
    RegisteredBy::Manual
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl Registry {
    /// Resolve `name` to an `AgentSpec`, honoring per-project `agent_overrides`
    /// when `project_root` matches a registered project. Falls back to the
    /// global `agents{}` entry. Returns `None` when neither layer has the
    /// agent.
    ///
    /// `project_root` is canonicalized to match how `ProjectEntry::path` is
    /// stored (always canonical). A non-existent or non-canonicalizable path
    /// just skips the override layer — the global agent still resolves.
    pub fn resolved_agent(&self, name: &str, project_root: &Path) -> Option<AgentSpec> {
        if let Ok(canon) = std::fs::canonicalize(project_root) {
            if let Some(project) = self.projects.iter().find(|p| p.path == canon) {
                if let Some(override_spec) = project.agent_overrides.get(name) {
                    return Some(override_spec.clone());
                }
            }
        }
        self.agents.get(name).cloned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackPolicy {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_cooldown_seconds")]
    pub cooldown_seconds: u64,
    #[serde(default = "default_retryable_exit_codes")]
    pub retryable_exit_codes: Vec<i32>,
    #[serde(default = "default_retryable_stderr_patterns")]
    pub retryable_stderr_patterns: Vec<String>,
    /// Per-spawn wall-clock timeout. Default 600s (10 min). Exceeded → kill
    /// child, return synthesized exit code 124 (GNU timeout convention),
    /// which `default_retryable_exit_codes` already classifies as retryable
    /// so fallback fires.
    #[serde(default = "default_spawn_timeout_secs")]
    pub spawn_timeout_secs: u64,
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            cooldown_seconds: default_cooldown_seconds(),
            retryable_exit_codes: default_retryable_exit_codes(),
            retryable_stderr_patterns: default_retryable_stderr_patterns(),
            spawn_timeout_secs: default_spawn_timeout_secs(),
        }
    }
}

fn default_spawn_timeout_secs() -> u64 {
    600
}

fn default_max_retries() -> u32 {
    2
}
fn default_cooldown_seconds() -> u64 {
    30
}
fn default_retryable_exit_codes() -> Vec<i32> {
    vec![2, 124, 137]
}
/// Default retryable patterns. Despite the name `retryable_stderr_patterns`
/// (kept for backward compat with existing `registry.yaml` files), the
/// orchestrator scans **stdout + stderr concatenated** against these patterns
/// because real binaries (claude, codex) write failure text to stdout. See
/// `docs/agent-contracts.md` for the per-agent verification.
fn default_retryable_stderr_patterns() -> Vec<String> {
    vec![
        // Generic LLM provider signals — appear in either stream.
        r"(?i)rate.?limit".into(),
        r"(?i)quota.?(exceeded|exhausted)".into(),
        r"(?i)token.?exhausted".into(),
        r"\b429\b".into(),
        r"(?i)api.?network.?error".into(),
        // Codex prints JSON to stdout even on transient failures. Match the
        // status codes inside the JSON: 429 (rate limit), 5xx (server).
        r#""status"\s*:\s*429"#.into(),
        r#""status"\s*:\s*5\d{2}"#.into(),
        r#""type"\s*:\s*"rate_limit_error""#.into(),
        r#""type"\s*:\s*"overloaded_error""#.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_registry() {
        let r = Registry::default();
        let y = serde_yaml::to_string(&r).unwrap();
        let back: Registry = serde_yaml::from_str(&y).unwrap();
        assert_eq!(back.projects.len(), 0);
        assert_eq!(back.fallback_policy.max_retries, 2);
    }

    #[test]
    fn old_registry_without_fallback_policy_loads() {
        let y = "projects: []\nagents: {}\n";
        let r: Registry = serde_yaml::from_str(y).unwrap();
        assert_eq!(r.fallback_policy.max_retries, 2);
        assert_eq!(r.fallback_policy.cooldown_seconds, 30);
    }

    #[test]
    fn registered_by_defaults_to_manual_when_missing() {
        let y = r#"
projects:
  - name: foo
    path: /tmp/foo
    registered_at: "2026-01-01T00:00:00Z"
"#;
        let r: Registry = serde_yaml::from_str(y).unwrap();
        assert_eq!(r.projects[0].registered_by, RegisteredBy::Manual);
    }
}
