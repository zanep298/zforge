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
    #[serde(default, rename = "fallback_policy")]
    pub spawn_policy: SpawnPolicy,
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

/// Limits on an agent spawn. Stored under the historical key
/// `fallback_policy` so existing registries keep their timeout; the fallback
/// fields it once held (`max_retries`, `retryable_*`, …) are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPolicy {
    /// Per-spawn wall-clock timeout. Default 600s (10 min). Exceeded → the
    /// agent's process tree is killed and exit code 124 is synthesized (GNU
    /// timeout convention).
    #[serde(default = "default_spawn_timeout_secs")]
    pub spawn_timeout_secs: u64,
}

impl Default for SpawnPolicy {
    fn default() -> Self {
        Self {
            spawn_timeout_secs: default_spawn_timeout_secs(),
        }
    }
}

fn default_spawn_timeout_secs() -> u64 {
    600
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
        assert_eq!(back.spawn_policy.spawn_timeout_secs, 600);
    }

    #[test]
    fn a_registry_from_the_fallback_era_keeps_its_timeout() {
        let y = "projects: []\nagents: {}\nfallback_policy:\n  max_retries: 3\n  \
                 retryable_exit_codes: [2]\n  spawn_timeout_secs: 90\n";
        let r: Registry = serde_yaml::from_str(y).unwrap();
        assert_eq!(r.spawn_policy.spawn_timeout_secs, 90);
        let saved = serde_yaml::to_string(&r).unwrap();
        assert!(
            saved.contains("fallback_policy:\n  spawn_timeout_secs: 90"),
            "{saved}"
        );
        assert!(!saved.contains("max_retries"), "{saved}");
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
