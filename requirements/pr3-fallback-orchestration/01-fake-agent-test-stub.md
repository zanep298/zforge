# PR3-01 — Fake agent test stub

## Goal

Provide a deterministic binary that the orchestrator integration tests can spawn instead of a real LLM CLI. Behavior controlled by environment variables.

## Files touched

- `examples/fake_agent.rs` (new)
- `Cargo.toml` — register the example explicitly if not auto-discovered

## Implementation

### `examples/fake_agent.rs`

```rust
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    // Read prompt from stdin so we can later verify the orchestrator delivers it.
    let mut prompt = String::new();
    use std::io::Read;
    let _ = std::io::stdin().read_to_string(&mut prompt);

    let stderr_msg = std::env::var("FAKE_STDERR").unwrap_or_default();
    if !stderr_msg.is_empty() {
        let _ = writeln!(std::io::stderr(), "{stderr_msg}");
    }

    if let Ok(stdout_msg) = std::env::var("FAKE_STDOUT") {
        let _ = writeln!(std::io::stdout(), "{stdout_msg}");
    }

    if let Ok(sleep_ms) = std::env::var("FAKE_SLEEP_MS") {
        if let Ok(ms) = sleep_ms.parse::<u64>() {
            std::thread::sleep(Duration::from_millis(ms));
        }
    }

    let exit_code: i32 = std::env::var("FAKE_EXIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // ExitCode only accepts u8; clamp.
    ExitCode::from((exit_code & 0xFF) as u8)
}
```

### Registration helper

In tests, register an agent named `fake` in the test registry whose `command` points to the built example:

```rust
fn register_fake_agent(registry: &mut Registry) {
    let exe = env!("CARGO_TARGET_TMPDIR_OR_BIN"); // resolved at test time
    registry.agents.insert("fake".into(), AgentSpec {
        command: target_path_for_example("fake_agent").to_string_lossy().into_owned(),
        args: vec![],
    });
}
```

A simpler approach: in `tests/common/mod.rs`, expose a helper that returns `env!("CARGO_BIN_EXE_fake_agent")`. Cargo provides `CARGO_BIN_EXE_<name>` for binaries (and examples when `[[example]]` table sets `required-features` correctly). If examples are not exposed via that env var, fall back to a known path under `target/debug/examples/fake_agent`.

## Acceptance criteria

- [ ] `cargo build --example fake_agent` produces a binary.
- [ ] Running the binary with `FAKE_EXIT=2 FAKE_STDERR="rate limit exceeded"` exits with code 2 and writes the message to stderr.
- [ ] Running the binary with no env vars exits 0 silently.
- [ ] `FAKE_SLEEP_MS=200` causes a measurable delay.
- [ ] The path resolution helper used by tests returns a path that exists after `cargo build`.

## Depends on

- None. Can land first in PR 3 sequence.

## Notes

- Avoid tokio in this stub — keep it lightweight.
- Do not write `Sentinel files` to the cwd — that pollutes test workspaces. Stdin/stdout/stderr + env vars only.
- If `CARGO_BIN_EXE_*` does not cover examples, document in this task that tests must derive the path from `target/debug/examples`.

---

```yaml
status: done
owner: ""
pr: ""
```
