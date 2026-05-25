use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};

pub fn with_lock<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let lock_path = crate::registry::paths::registry_lock_file()?;
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
    let file: File = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open lock {lock_path:?}"))?;
    FileExt::lock_exclusive(&file).context("acquire registry lock")?;
    let result = f();
    let _ = FileExt::unlock(&file);
    result
}
