# PR1-07 — Wire `zforge init` to auto-register

## Goal

After `init` finishes scaffolding `.zforge/`, automatically register the project in `~/.zforge/registry.yaml`. Add flags to control the behavior.

## Files touched

- `src/cli/init.rs` — extend the clap arg struct and call `registry::auto::auto_register` at the tail of `run()`

## Implementation

### New flags on `init`

Add to the existing init args struct:

```rust
/// Skip auto-registration of this project in ~/.zforge/registry.yaml.
#[arg(long)]
pub no_register: bool,

/// Override the registered project name (default: sanitized basename of cwd).
#[arg(long)]
pub name: Option<String>,

/// Set this project as the current_project after registering.
#[arg(long)]
pub switch: bool,
```

### Registration call

At the end of init's existing `run()` (after the `.zforge/` scaffold and any other side effects succeed):

```rust
if !args.no_register {
    let cwd = std::env::current_dir()
        .context("get cwd for auto-register")?;
    match crate::registry::auto::auto_register(&cwd, args.name.as_deref(), args.switch) {
        Ok(crate::registry::auto::AutoResult::Registered { name }) => {
            println!("registered project {name} in ~/.zforge/registry.yaml");
        }
        Ok(crate::registry::auto::AutoResult::AlreadyExists { name }) => {
            println!("project {name} already registered");
        }
        Ok(crate::registry::auto::AutoResult::Suffixed { requested, final_name }) => {
            eprintln!(
                "warning: name {requested:?} already in registry; \
                 registered as {final_name:?}"
            );
        }
        Ok(crate::registry::auto::AutoResult::Updated { old_name, new_name }) => {
            println!("renamed registry entry: {old_name} -> {new_name}");
        }
        Err(e) => {
            // Non-fatal: init scaffold itself succeeded.
            eprintln!("warning: registry update failed: {e:#}");
        }
    }
}
```

### Edge cases

- `init` must call `auto_register` AFTER the local scaffold completes, so `validate::ensure_zforge_dir` passes.
- If `init` exits early on any prior step (existing behavior), do not call `auto_register`.
- Existing `init --local`, `--agent`, `--codex`, etc. flags must continue working unchanged.

## Acceptance criteria

- [ ] `zforge init` in a fresh dir creates `.zforge/` AND adds a registry entry with `registered_by: init`.
- [ ] `zforge init` in a dir that is already registered with the same name → prints "already registered", exit 0, no mutation.
- [ ] `zforge init --name custom` registers as `custom` instead of basename.
- [ ] `zforge init --switch` sets `current_project` to the just-registered name.
- [ ] `zforge init --no-register` does not touch `~/.zforge/registry.yaml`.
- [ ] If `~/.zforge/` is not writable (simulated by chmod or `ZFORGE_HOME=/nonexistent`), init prints a warning but exits 0.
- [ ] An init invocation on a dir whose path is registered under a different name (e.g., user ran `init --name a` then `init --name b` in the same dir) renames the entry; `registered_at` preserved.

## Depends on

- PR1-05.

## Notes

- Do not gate the entire init on registry success — local scaffold is the contract; registry is best-effort.
- The warning text on `Suffixed` deliberately goes to stderr so script consumers parsing stdout are not confused.

---

```yaml
status: done
owner: ""
pr: ""
```
