# PR3-02 — Orchestrator spawn primitive

## Goal

Wrap `std::process::Command` in a small typed helper that captures exit code + stderr + duration. Foundation for the retry loop.

## Files touched

- `src/orchestrator/mod.rs` (new)
- `src/orchestrator/spawn.rs` (new)
- `src/main.rs` — `mod orchestrator;`

## Implementation

### `spawn.rs`

```rust
use crate::registry::schema::AgentSpec;
use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Debug)]
pub struct SpawnOutcome {
    pub exit_code: i32,
    pub stderr: String,
    pub stdout: String,
    pub duration_ms: u128,
}

pub fn spawn_agent(spec: &AgentSpec, prompt: &str) -> Result<SpawnOutcome> {
    let started = Instant::now();
    let mut cmd = Command::new(&spec.command);
    cmd.args(&spec.args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn agent {:?}", spec.command))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .context("write prompt to agent stdin")?;
    }

    let output = child
        .wait_with_output()
        .context("wait for agent process")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let duration_ms = started.elapsed().as_millis();

    Ok(SpawnOutcome {
        exit_code,
        stderr,
        stdout,
        duration_ms,
    })
}
```

### `mod.rs` skeleton

```rust
pub mod spawn;
// fallback, history, run_phase added in subsequent tasks
```

### Streaming consideration

`wait_with_output` blocks until the child exits and collects both streams in memory. For long-running agents that print progress, consider streaming stderr to the terminal in real time. For PR 3, keep the simple model — agents print at the end, output is small. Document this constraint; tackle streaming in a later PR if user-visible delay becomes an issue.

## Acceptance criteria

- [ ] `spawn_agent` with the fake_agent stub (`FAKE_EXIT=0`) returns `exit_code: 0`, empty stderr.
- [ ] `spawn_agent` with `FAKE_EXIT=2 FAKE_STDERR="rate limit"` returns `exit_code: 2`, stderr contains "rate limit".
- [ ] `spawn_agent` with `FAKE_SLEEP_MS=150` returns `duration_ms >= 150`.
- [ ] Stdin delivery verified — `FAKE_STDOUT_FROM_STDIN` env-flagged echo (extend fake_agent if needed) confirms the prompt arrives.
- [ ] Spawning a non-existent binary returns an `anyhow::Error` (does not panic).

## Depends on

- PR1-02 (AgentSpec), PR3-01 (fake_agent for testing).

## Notes

- Use `wait_with_output` rather than a manual `wait` + manual stream reads — simpler and avoids the deadlock that comes from blocking on stdin while the child blocks on a full stderr pipe.
- `exit_code: -1` is a sentinel for "process killed by signal" (no exit code). The fallback decision (PR3-03) must handle this case.

---

```yaml
status: done
owner: ""
pr: ""
```
