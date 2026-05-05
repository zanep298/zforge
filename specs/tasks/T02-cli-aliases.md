# T02 — CLI Command Aliases

**Phase:** 1  
**Priority:** High  
**Status:** TODO

## Goal

Add `implement` and `test` as canonical command names per the spec, keeping `code` and `verify` as aliases for backwards compatibility.

## Current Commands vs Spec

| Spec | Current | Action |
|------|---------|--------|
| `zf implement <TASK_ID>` | `zf code <TASK_ID>` | Add alias |
| `zf test <TASK_ID>` | `zf verify <TASK_ID>` | Add alias |

All other commands already match.

## Required Changes

### src/main.rs

Add `Implement` and `Test` variants to `Commands` enum that delegate to the same handlers:

```rust
Implement {
    task_id: String,
    #[arg(long)]
    done: bool,
},
Test {
    task_id: String,
    #[arg(long)]
    command: Option<String>,
    #[arg(long, default_value = "600")]
    timeout: u64,
},
```

Match arms:
```rust
Commands::Implement { task_id, done } => cli::code::run(&task_id, done),
Commands::Test { task_id, command, timeout } => cli::verify::run(&task_id, command, timeout),
```

### State hints

Update `State::hint()` in `src/state/mod.rs`:
- `PlanReviewed` hint: `"run: zf implement <TASK-ID>"` (was `zf code`)
- `Coded` hint: `"run: zf test <TASK-ID>"` (was `zf verify`)

Keep old hints as secondary line so both forms are documented.

## Test Cases

- `zf implement TASK-1 --done` behaves identically to `zf code TASK-1 --done`
- `zf test TASK-1` behaves identically to `zf verify TASK-1`
- `zf code` still works (no removal)
- `zf verify` still works (no removal)

## Files to Modify

| File | Change |
|------|--------|
| `src/main.rs` | Add `Implement` + `Test` command variants |
| `src/state/mod.rs` | Update hint strings for `PlanReviewed` and `Coded` |
