# PR1-10 — Integration tests

## Goal

End-to-end coverage of registry behavior across CRUD, init auto-register, and concurrency.

## Files touched

- `tests/registry_test.rs` (new)
- `tests/init_auto_register_test.rs` (new)
- `tests/status_global_test.rs` (new)
- `tests/common/mod.rs` (new) — shared helpers

## Test infrastructure

### `tests/common/mod.rs`

```rust
use std::path::PathBuf;
use tempfile::TempDir;

pub struct TestHome {
    pub home: TempDir,
}

impl TestHome {
    pub fn new() -> Self {
        let home = tempfile::tempdir().expect("tempdir");
        std::env::set_var("ZFORGE_HOME", home.path());
        Self { home }
    }

    pub fn registry_path(&self) -> PathBuf {
        self.home.path().join("registry.yaml")
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        std::env::remove_var("ZFORGE_HOME");
    }
}
```

Add `tempfile = "3"` to `[dev-dependencies]` in `Cargo.toml`.

### Test serialization

Registry tests mutate a shared env var (`ZFORGE_HOME`). Use the `serial_test` crate or a single integration test file that runs serially by default. Recommendation: add `serial_test = "3"` as dev-dep and mark each test `#[serial_test::serial]`.

## Required test cases

### `tests/registry_test.rs`

- [ ] `roundtrip_default_registry` — default → YAML → parse → equal.
- [ ] `roundtrip_full_registry` — registry with projects + agents + policy → YAML → parse → equal.
- [ ] `old_registry_without_fallback_policy_loads` — manual YAML missing `fallback_policy` parses successfully with defaults.
- [ ] `atomic_write_survives_mid_write` — inject a panic between `write_all` and `rename` (use a helper that overrides the tmp path or test the `save_atomic` invariant directly); confirm the original `registry.yaml` is intact.
- [ ] `lock_serializes_concurrent_writers` — spawn two threads each calling `with_lock(|| append entry)`; both entries end up in the final registry; no torn writes.
- [ ] `name_validation_rejects_bad_inputs` — empty, 65 chars, space, dot, slash all rejected.
- [ ] `next_available_name_finds_first_gap`.

### `tests/init_auto_register_test.rs`

- [ ] `init_in_fresh_dir_registers` — create tempdir with `.zforge/`, call `auto_register`, assert `Registered`.
- [ ] `init_in_same_dir_is_noop` — second call returns `AlreadyExists`.
- [ ] `init_same_name_diff_path_suffixes` — register `foo` at dir A, then call with same name at dir B → `Suffixed { final_name: "foo-2" }`.
- [ ] `init_same_path_diff_name_updates` — register at dir A with name `a`, then call with name `b` at dir A → `Updated`, `registered_at` preserved.
- [ ] `switch_flag_sets_current_project`.
- [ ] `init_without_zforge_dir_errors`.
- [ ] `concurrent_init_two_threads_both_succeed` — two threads each in different tempdirs call `auto_register`; both entries present in final registry.

### `tests/status_global_test.rs`

- [ ] `empty_registry_returns_empty` — `run_global` with no projects exits cleanly.
- [ ] `aggregates_tasks_across_projects` — set up two tempdir projects each with one task; assert both rows in output.
- [ ] `skips_malformed_state_yaml` — write garbage to one project's `.state.yaml`; assert other projects still appear; warning in stderr.
- [ ] `skips_missing_path` — registry references a path that no longer exists; assert skip.
- [ ] `respects_timeout` — simulate a slow project (best-effort: register a path with permissions denied + assert timeout warning, or inject a sleep via a feature flag).
- [ ] `terminal_tasks_excluded` — task at `Reviewed` state is omitted.

## Acceptance criteria

- [ ] `cargo test` (full suite) passes.
- [ ] All listed test cases exist and pass.
- [ ] No test leaks state into `$HOME/.zforge/` (verify by running tests with `HOME=/dev/null` or strict tempdir audit).
- [ ] `cargo clippy --tests -- -D warnings` passes.

## Depends on

- All previous PR 1 tasks.

## Notes

- Concurrent test for `auto_register` is the highest-risk one — `fs2::flock` semantics on macOS vs Linux differ slightly. If flaky on CI, switch to file-based mutex via `OpenOptions::create_new` lock file.
- Avoid time-dependent assertions on `registered_at` — assert presence, not exact value.

---

```yaml
status: done
owner: ""
pr: ""
```
