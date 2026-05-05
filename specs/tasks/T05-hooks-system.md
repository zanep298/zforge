# T05 — Hooks System

**Phase:** 3  
**Priority:** Medium  
**Depends on:** T03  
**Status:** TODO

## Goal

Execute pre/post hooks at defined lifecycle points in the TDD workflow.

## Hook Types

| Type | Trigger | Behavior on failure |
|------|---------|---------------------|
| pre | Before CLI command runs | Abort command, print error |
| post | After CLI command succeeds | Print warning, continue |

## Hook Points

| Command | Pre Hook | Post Hook |
|---------|----------|-----------|
| `testspec` | validate-spec | — |
| `approve testspec` | validate-coverage | — |
| `verify` / `test` | — | run-tests, generate-report |
| `review` | — | generate-report |

## Hook Config Format

Hooks defined in `.zforge/config.yaml`:

```yaml
hooks:
  pre:
    - command: testspec
      script: .zforge/hooks/pre-validate-spec.sh
    - command: approve
      artifact: testspec
      script: .zforge/hooks/pre-validate-coverage.sh
  post:
    - command: verify
      script: .zforge/hooks/post-run-tests.sh
    - command: verify
      script: .zforge/hooks/post-generate-report.sh
```

## Config Changes

Add `HooksConfig` to `src/config/mod.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre: Vec<HookEntry>,
    #[serde(default)]
    pub post: Vec<HookEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HookEntry {
    pub command: String,
    pub artifact: Option<String>,  // for "approve" command discrimination
    pub script: PathBuf,
}
```

## Hook Runner

New module `src/hooks/mod.rs`:

```rust
pub fn run_pre(config: &Config, command: &str, artifact: Option<&str>, task_id: &str) -> Result<()>
pub fn run_post(config: &Config, command: &str, artifact: Option<&str>, task_id: &str) -> Result<()>
```

- Resolve script path relative to project root
- Set env vars: `ZFLOW_TASK_ID`, `ZFLOW_PROJECT_ROOT`, `ZFLOW_TASKS_DIR`
- Spawn subprocess, capture stdout/stderr
- Pre hook failure → return `Err` (command aborts)
- Post hook failure → print warning, return `Ok`

## CLI Integration

Each command handler calls hooks at start/end:

```rust
// src/cli/testspec.rs
pub fn run(task_id: &str, done: bool) -> Result<()> {
    let config = config::load()?;
    hooks::run_pre(&config, "testspec", None, task_id)?;  // aborts if fails
    // ... existing logic ...
    if done {
        hooks::run_post(&config, "testspec", None, task_id)?;
    }
    Ok(())
}
```

## Environment Variables Passed to Scripts

| Var | Value |
|-----|-------|
| `ZFLOW_TASK_ID` | current task ID |
| `ZFLOW_PROJECT_ROOT` | absolute path to project root |
| `ZFLOW_TASKS_DIR` | absolute path to `.zforge/tasks/` |
| `ZFLOW_COMMAND` | name of command being run |

## File Structure

```
src/hooks/
└── mod.rs    # run_pre, run_post, HookRunner
```

## Test Cases

- Pre hook exits 0 → command continues
- Pre hook exits 1 → command aborts with hook error message
- Post hook exits 1 → warning printed, Ok returned
- Hook receives correct env vars
- Missing script → error "hook script not found: <path>"
- No hooks configured → no-op, no error
