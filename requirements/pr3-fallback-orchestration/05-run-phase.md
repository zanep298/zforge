# PR3-05 — `run_phase` orchestrator entry point

## Goal

The main loop. Resolves agent, spawns, decides on fallback, retries up to the policy cap, persists state.

## Files touched

- `src/orchestrator/mod.rs`
- `src/orchestrator/run.rs` (new, or inline in `mod.rs`)

## Implementation

```rust
use crate::orchestrator::{
    fallback::CompiledPolicy,
    history::{fallback_count, record_fallback},
    spawn::spawn_agent,
};
use crate::registry;
use crate::state::TaskState;
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

pub fn run_phase(task_id: &str, phase: &str, project_root: &Path) -> Result<()> {
    let registry = registry::io::load()?;
    let policy = CompiledPolicy::compile(&registry.fallback_policy)?;

    // Load state from .zforge/tasks/<id>/.state.yaml
    let state_path = project_root
        .join(".zforge/tasks")
        .join(task_id)
        .join(".state.yaml");
    let mut state = load_state(&state_path)?;

    // Build prompt once (artifacts already on disk; build_context_for_phase
    // emits /file refs for prior phases).
    let prompt = crate::prompt::context::build_context_for_phase(task_id, phase, project_root)?;
    let prompt_str = crate::prompt::engine::render(&prompt)?;

    loop {
        let agent_name = state
            .effective_agent()
            .ok_or_else(|| anyhow!("task {task_id} has no agent assigned"))?
            .to_string();

        let spec = registry
            .resolved_agent(&agent_name, project_root)
            .ok_or_else(|| anyhow!("agent {agent_name:?} not in registry"))?;

        let outcome = spawn_agent(&spec, &prompt_str)?;

        if outcome.exit_code == 0 {
            return Ok(());
        }

        let reason = match policy.should_fallback(outcome.exit_code, &outcome.stderr) {
            Some(r) => r,
            None => {
                // Non-retryable failure — propagate.
                anyhow::bail!(
                    "agent {agent_name} failed in phase {phase} (exit={code})\nstderr:\n{stderr}",
                    code = outcome.exit_code,
                    stderr = outcome.stderr.trim_end()
                );
            }
        };

        if fallback_count(&state) as u32 >= policy.max_retries {
            anyhow::bail!(
                "fallback budget exhausted ({} retries) for task {task_id}",
                policy.max_retries
            );
        }

        record_fallback(&mut state, &reason, phase)
            .context("record fallback")?;
        save_state_atomic(&state_path, &state)?;

        if policy.cooldown_ms > 0 {
            sleep(Duration::from_millis(policy.cooldown_ms));
        }
        // Loop: next iteration uses the new active_agent.
    }
}

fn load_state(path: &Path) -> Result<TaskState> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read {path:?}"))?;
    serde_yaml::from_str(&raw)
        .with_context(|| format!("parse {path:?}"))
}

fn save_state_atomic(path: &Path, state: &TaskState) -> Result<()> {
    let tmp = path.with_extension("yaml.tmp");
    let yaml = serde_yaml::to_string(state)?;
    std::fs::write(&tmp, yaml)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

### Helper on `Registry`

```rust
impl Registry {
    pub fn resolved_agent(&self, name: &str, project_root: &Path) -> Option<AgentSpec> {
        // Per-project override > global.
        let canon = std::fs::canonicalize(project_root).ok()?;
        if let Some(project) = self.projects.iter().find(|p| p.path == canon) {
            if let Some(override_spec) = project.agent_overrides.get(name) {
                return Some(override_spec.clone());
            }
        }
        self.agents.get(name).cloned()
    }
}
```

## Acceptance criteria

- [ ] Successful spawn (exit 0) returns `Ok(())` after one attempt; no fallback recorded.
- [ ] Retryable failure (exit 124) triggers fallback, records history, retries with the new agent.
- [ ] Non-retryable failure (exit 1, no matching stderr) returns an error WITHOUT recording a fallback or swapping agents.
- [ ] After `max_retries` retryable failures, returns an error with "budget exhausted".
- [ ] `cooldown_seconds` is observed between attempts.
- [ ] `active_agent` is updated and persisted to `.state.yaml` between attempts (verifiable by reading the file mid-loop via a test hook or by checking after the final failure).
- [ ] `assigned_agent` is unchanged after any number of fallbacks.
- [ ] Per-project agent override (in `agent_overrides`) is respected.
- [ ] Loading a `.state.yaml` with no `active_agent` falls back to `assigned_agent`.
- [ ] Missing task state file produces a clean error.

## Depends on

- PR3-02, PR3-03, PR3-04.

## Notes

- Prompt is built once and reused for every retry. The fallback agent sees the same prompt + the same `/file` refs to prior-phase artifacts — that is by design (consistent context).
- Cooldown sleep blocks the caller; acceptable for CLI usage. For long cooldowns + MCP, document that the tool call will be slow.
- The retry budget is `max_retries`, not `max_retries + 1`. `max_retries = 2` means up to 2 fallback swaps; total attempts is `1 + 2 = 3`.

---

```yaml
status: done
owner: ""
pr: ""
```
