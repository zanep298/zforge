use crate::registry::{paths, schema::Registry};
use anyhow::{Context, Result};
use std::io::Write;

pub fn load() -> Result<Registry> {
    let path = paths::registry_file()?;
    if !path.exists() {
        return Ok(Registry::default());
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {path:?}"))?;
    serde_yaml::from_str(&raw).with_context(|| format!("parse {path:?}"))
}

pub fn save_atomic(registry: &Registry) -> Result<()> {
    let path = paths::registry_file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
    let tmp = path.with_extension("yaml.tmp");
    let yaml = serde_yaml::to_string(registry).context("serialize registry")?;
    {
        let mut f = std::fs::File::create(&tmp).with_context(|| format!("create {tmp:?}"))?;
        f.write_all(yaml.as_bytes())
            .with_context(|| format!("write {tmp:?}"))?;
        f.sync_all().context("fsync registry tmp")?;
    }
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {tmp:?} -> {path:?}"))?;
    Ok(())
}
