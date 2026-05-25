use std::path::PathBuf;

pub fn registry_dir() -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("ZFORGE_HOME") {
        return Ok(PathBuf::from(override_path));
    }
    dirs::home_dir()
        .map(|h| h.join(".zforge"))
        .ok_or_else(|| anyhow::anyhow!("unable to resolve home directory"))
}

pub fn registry_file() -> anyhow::Result<PathBuf> {
    Ok(registry_dir()?.join("registry.yaml"))
}

pub fn registry_lock_file() -> anyhow::Result<PathBuf> {
    Ok(registry_dir()?.join("registry.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_file_honors_zforge_home_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("ZFORGE_HOME", tmp.path());
        let f = registry_file().unwrap();
        assert_eq!(f, tmp.path().join("registry.yaml"));
        std::env::remove_var("ZFORGE_HOME");
    }
}
