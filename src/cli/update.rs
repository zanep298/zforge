use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::env;
use std::io::Write as _;
use std::process::Command;

const REPO: &str = "zanep298/zforge";
const API_URL: &str = "https://api.github.com/repos/zanep298/zforge/releases/latest";

pub fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("Current version: {}", current.cyan());

    print!("Checking for updates… ");
    std::io::stdout().flush()?;

    let resp = ureq::get(API_URL)
        .set("User-Agent", &format!("zforge/{current}"))
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("failed to reach GitHub API")?;

    let body: serde_json::Value = resp.into_json()?;
    let tag = body["tag_name"]
        .as_str()
        .context("missing tag_name in release")?;
    let latest = tag.trim_start_matches('v');

    println!("latest: {}", latest.cyan());

    if !is_newer(latest, current)? {
        println!("{} Already up to date.", "✓".green());
        return Ok(());
    }

    println!("Updating {} → {}…", current, latest.cyan());

    let target = detect_target()?;
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let asset_name = format!("zforge-{target}.{ext}");

    let assets = body["assets"]
        .as_array()
        .context("missing assets in release")?;
    let download_url = assets
        .iter()
        .find_map(|a| {
            let name = a["name"].as_str()?;
            if name == asset_name {
                a["browser_download_url"].as_str().map(str::to_owned)
            } else {
                None
            }
        })
        .with_context(|| format!("no asset {asset_name} in release {tag}"))?;

    println!("Downloading {}…", asset_name.dimmed());

    let tmp_dir = env::temp_dir().join(format!("zforge-update-{latest}"));
    std::fs::create_dir_all(&tmp_dir)?;
    let archive_path = tmp_dir.join(&asset_name);

    let resp = ureq::get(&download_url)
        .set("User-Agent", &format!("zforge/{current}"))
        .call()
        .context("download failed")?;

    {
        let mut out = std::fs::File::create(&archive_path)?;
        std::io::copy(&mut resp.into_reader(), &mut out)?;
    }

    let bin_name = if cfg!(windows) { "zforge.exe" } else { "zforge" };
    extract_binary(&archive_path, &tmp_dir, bin_name)?;

    let extracted = tmp_dir.join(bin_name);
    if !extracted.exists() {
        bail!("extraction succeeded but {bin_name} not found in archive");
    }

    let current_exe =
        env::current_exe().context("cannot determine current executable path")?;

    // Move old binary aside, copy new one in, restore on failure
    let old_path = current_exe.with_extension("old");
    let _ = std::fs::remove_file(&old_path);
    std::fs::rename(&current_exe, &old_path).with_context(|| {
        format!(
            "cannot move {} aside — check permissions",
            current_exe.display()
        )
    })?;

    if let Err(e) = std::fs::copy(&extracted, &current_exe) {
        // Restore original so the binary isn't left broken
        let _ = std::fs::rename(&old_path, &current_exe);
        return Err(e).context("failed to write new binary");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current_exe)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current_exe, perms)?;
    }

    // Cleanup temp files
    let _ = std::fs::remove_file(&old_path);
    let _ = std::fs::remove_file(&archive_path);
    let _ = std::fs::remove_file(&extracted);

    println!("{} Updated to v{}", "✓".green(), latest);
    println!(
        "  Run {} to refresh global templates.",
        "zforge install --force".cyan()
    );

    Ok(())
}

fn detect_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        (os, arch) => bail!(
            "no prebuilt binary for {os}/{arch}; build from source: \
             cargo install --git https://github.com/{REPO}"
        ),
    }
}

fn extract_binary(
    archive: &std::path::Path,
    dest: &std::path::Path,
    bin_name: &str,
) -> Result<()> {
    if cfg!(windows) {
        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Force '{}' '{}'",
                    archive.display(),
                    dest.display()
                ),
            ])
            .status()?;
        if !status.success() {
            bail!("Expand-Archive failed");
        }
    } else {
        let status = Command::new("tar")
            .args([
                "xzf",
                &archive.to_string_lossy(),
                "--directory",
                &dest.to_string_lossy(),
                bin_name,
            ])
            .status()
            .context("failed to run tar")?;
        if !status.success() {
            bail!("tar extraction failed");
        }
    }
    Ok(())
}

/// Returns true if `latest` is strictly greater than `current` (semver major.minor.patch).
fn is_newer(latest: &str, current: &str) -> Result<bool> {
    let parse = |s: &str| -> Result<(u32, u32, u32)> {
        let p: Vec<&str> = s.split('.').collect();
        if p.len() < 3 {
            bail!("invalid version string: {s}");
        }
        Ok((p[0].parse()?, p[1].parse()?, p[2].parse()?))
    };
    Ok(parse(latest)? > parse(current)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_patch() {
        assert!(is_newer("0.0.17", "0.0.16").unwrap());
    }

    #[test]
    fn same_not_newer() {
        assert!(!is_newer("0.0.16", "0.0.16").unwrap());
    }

    #[test]
    fn older_not_newer() {
        assert!(!is_newer("0.0.15", "0.0.16").unwrap());
    }

    #[test]
    fn newer_minor_beats_patch() {
        assert!(is_newer("0.1.0", "0.0.99").unwrap());
    }

    #[test]
    fn newer_major_beats_minor() {
        assert!(is_newer("1.0.0", "0.9.9").unwrap());
    }
}
