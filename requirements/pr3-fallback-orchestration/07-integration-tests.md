# PR3-07 — Orchestrator integration tests

## Goal

Exhaustive coverage of the fallback state machine using the fake_agent stub.

## Files touched

- `tests/orchestrator_test.rs` (new)
- Possibly extend `tests/common/mod.rs` with `setup_task` and `with_fake_agents` helpers.

## Test infrastructure

Each test sets up:

1. A `TestHome` (tempdir with `ZFORGE_HOME` exported).
2. A tempdir project with `.zforge/tasks/T1/.state.yaml`.
3. A registry containing `agents.primary` and `agents.fallback` both pointing at the `fake_agent` example.
4. Environment vars (`FAKE_EXIT`, `FAKE_STDERR`, etc.) configured by writing a tiny shell wrapper that sets env then exec's the example — because the `Command` we invoke takes the path verbatim, the easiest way to inject per-attempt env is via two distinct wrapper scripts (`primary.sh`, `fallback.sh`), each setting different vars.

Alternative: extend `fake_agent` to read a JSON config file pointed to by `FAKE_CONFIG`, and pass that file path as the agent's `args`. Cleaner; recommend this approach.

```rust
// fake_agent.rs change (PR3-01 amendment):
let cfg_path = std::env::args().nth(1)
    .or_else(|| std::env::var("FAKE_CONFIG").ok());
// parse JSON config: exit_code, stderr, stdout, sleep_ms
```

## Required test cases

- [ ] `success_first_attempt` — primary exits 0; orchestrator returns Ok; no history entry.
- [ ] `fallback_on_retryable_exit_124` — primary exits 124, fallback exits 0; one history entry; `active_agent == fallback`.
- [ ] `fallback_on_stderr_rate_limit` — primary exits 1 with stderr "rate limit exceeded"; fallback runs; success.
- [ ] `no_fallback_on_test_failure_exit_1` — primary exits 1 with empty stderr; orchestrator returns Err; no history entry; `active_agent` unchanged.
- [ ] `assigned_agent_immutable_after_fallback` — read state file before and after a fallback; `assigned_agent` field byte-equal.
- [ ] `max_retries_terminal_failure` — both primary and fallback exit 124; with `max_retries: 2` (default) and only one fallback agent configured, after exhausting budget orchestrator errors out. (Reminder: spec only supports one-step fallback for now; `max_retries` caps loop iterations.)
- [ ] `cooldown_respected` — set `cooldown_seconds: 1` in policy; measure elapsed time between two spawns; ≥ 1s.
- [ ] `flow_preserved_after_fallback` — task's `flow` field unchanged by fallback.
- [ ] `fallback_persists_state_atomically` — after a fallback, kill the orchestrator (panic via test hook) before the second spawn; reload `.state.yaml`; `active_agent` is the fallback, `fallback_history` has the entry.
- [ ] `non_existent_binary_clean_error` — point an agent at a fake path; orchestrator errors without panic.
- [ ] `per_project_agent_override` — set `agent_overrides.primary` for the project; assert the override is used instead of the global spec.
- [ ] `legacy_task_uses_init_default_agent` — task with no `assigned_agent`; init-time config has `agent: claude`; orchestrator picks `claude`.

## Acceptance criteria

- [ ] All listed cases exist and pass.
- [ ] `cargo test orchestrator` runs in under 30 seconds on CI (limit cooldown durations in test cases).
- [ ] No tests rely on real LLM binaries.
- [ ] `cargo clippy --tests -- -D warnings` clean.

## Depends on

- PR3-01 through PR3-06.

## Notes

- Time-dependent tests (cooldown) should assert lower bounds (`>= cooldown`), never exact equality — CI is noisy.
- If `wait_with_output` deadlock surfaces in tests with large prompts, that is a real bug — investigate streaming in PR3-02 follow-up rather than working around in tests.

---

```yaml
status: done
owner: ""
pr: ""
```
