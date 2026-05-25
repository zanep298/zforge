# PR1-02 — Registry schema + paths

## Goal

Define the typed schema for `~/.zforge/registry.yaml` and centralize the path resolution helper.

## Files touched

- `src/registry/mod.rs` (new)
- `src/registry/schema.rs` (new)
- `src/registry/paths.rs` (new)
- `src/main.rs` — register the new `registry` module (`mod registry;`)

## Implementation

### `schema.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            cooldown_seconds: default_cooldown_seconds(),
            retryable_exit_codes: default_retryable_exit_codes(),
            retryable_stderr_patterns: default_retryable_stderr_patterns(),
        }
    }
}

fn default_max_retries() -> u32 { 2 }
fn default_cooldown_seconds() -> u64 { 30 }
fn default_retryable_exit_codes() -> Vec<i32> { vec![2, 124, 137] }
fn default_retryable_stderr_patterns() -> Vec<String> {
    vec![
        r"(?i)rate.?limit".into(),
        r"(?i)quota.?(exceeded|exhausted)".into(),
        r"(?i)token.?exhausted".into(),
        r"\b429\b".into(),
        r"(?i)api.?network.?error".into(),
    ]
}
```

### `paths.rs`

```rust
use std::path::PathBuf;

pub fn registry_dir() -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("ZFORGE_HOME") {
        return Ok(PathBuf::from(override_path));
    }
    dirs::home_dir()
        .map(|h| h.join(".zforge"))
        .ok_or_else(|| anyhow::anyhow!("unable to resolve home directory"))
}

pub fn registry_file() -> anyhow::Result<PathBuf> {
    Ok(registry_dir()?.join("registry.yaml"))
}

pub fn registry_lock_file() -> anyhow::Result<PathBuf> {
    Ok(registry_dir()?.join("registry.lock"))
}
```

### `mod.rs`

```rust
pub mod paths;
pub mod schema;

pub use schema::{AgentSpec, FallbackPolicy, ProjectEntry, Registry, RegisteredBy};
```

## Acceptance criteria

- [ ] `cargo build` passes.
- [ ] Round-trip test: serialize `Registry::default()` to YAML, deserialize back — equal.
- [ ] Round-trip test: serialize a registry with one project + one agent, deserialize, fields preserved.
- [ ] An old `registry.yaml` missing `fallback_policy` parses successfully and yields default policy.
- [ ] An entry missing `registered_by` defaults to `Manual`.
- [ ] `registry_file()` returns `~/.zforge/registry.yaml` by default; honors `ZFORGE_HOME` env var.

## Depends on

- PR1-01.

## Notes

- All optional fields use `#[serde(default)]` so forward/backward compat is preserved.
- `BTreeMap` instead of `HashMap` for deterministic YAML output.
- Decide here whether `ZFORGE_HOME` is the canonical override knob — locks it in for the rest of the PRs.

---

```yaml
status: done
owner: ""
pr: ""
```
