pub mod reader;
pub mod writer;

/// Replace `path` with `bytes` atomically: temp file in the same directory
/// (so the rename cannot cross filesystems), fsync, rename.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;

    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("write_atomic: {path:?} has no file name"))?
        .to_string_lossy();
    // Per-process suffix so two writers never share a temp file.
    let tmp = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp).with_context(|| format!("create {tmp:?}"))?;
        f.write_all(bytes)
            .with_context(|| format!("write {tmp:?}"))?;
        f.sync_all().with_context(|| format!("fsync {tmp:?}"))?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("rename {tmp:?} -> {path:?}"))?;
    Ok(())
}
