# PR1-03 — Registry I/O: load, save, lock

## Goal

Provide safe, atomic, lock-protected read/write helpers for `registry.yaml`.

## Files touched

- `src/registry/io.rs` (new)
- `src/registry/lock.rs` (new)
- `src/registry/mod.rs` — re-export `load`, `save_atomic`, `with_lock`

## Implementation

### `lock.rs`

```rust
use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};

pub fn with_lock<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let lock_path = crate::registry::paths::registry_lock_file()?;
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {parent:?}"))?;
    }
    let file: File = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("open lock {lock_path:?}"))?;
    file.lock_exclusive().context("acquire registry lock")?;
    let result = f();
    // Best-effort unlock — drop also releases.
    let _ = file.unlock();
    result
}
```

### `io.rs`

```rust
use crate::registry::{paths, schema::Registry};
use anyhow::{Context, Result};
use std::io::Write;

pub fn load() -> Result<Registry> {
    let path = paths::registry_file()?;
    if !path.exists() {
        return Ok(Registry::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read {path:?}"))?;
    serde_yaml::from_str(&raw)
        .with_context(|| format!("parse {path:?}"))
}

pub fn save_atomic(registry: &Registry) -> Result<()> {
    let path = paths::registry_file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {parent:?}"))?;
    }
    let tmp = path.with_extension("yaml.tmp");
    let yaml = serde_yaml::to_string(registry).context("serialize registry")?;
    {
        let mut f = std::fs::File::create(&tmp)
            .with_context(|| format!("create {tmp:?}"))?;
        f.write_all(yaml.as_bytes())
            .with_context(|| format!("write {tmp:?}"))?;
        f.sync_all().context("fsync registry tmp")?;
    }
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename {tmp:?} -> {path:?}"))?;
    Ok(())
}
```

### `mod.rs` additions

```rust
pub mod io;
pub mod lock;

pub use io::{load, save_atomic};
pub use lock::with_lock;
```

## Acceptance criteria

- [ ] `load()` returns `Registry::default()` when the file does not exist.
- [ ] `save_atomic` writes via `.tmp` + rename; partial writes (simulated by injecting an error before rename) leave the original file intact.
- [ ] `with_lock` serializes two concurrent threads attempting to mutate the registry — second waits.
- [ ] All public functions return `anyhow::Result` with `with_context` annotations on every fallible I/O call.
- [ ] No `unwrap()` / `expect()` outside test code.

## Depends on

- PR1-02.

## Notes

- Advisory lock — does not prevent reads. `status --global` is allowed to read a partially-stale snapshot.
- `sync_all()` on the temp file before rename guarantees durability of contents before the rename swaps the inode.
- Lock file lives at `~/.zforge/registry.lock` — never deleted; size 0; idempotent.

---

```yaml
status: done
owner: ""
pr: ""
```
