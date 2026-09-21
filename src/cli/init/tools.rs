//! External tools wired up by `zforge init`: rtk, CodeGraph and caveman.
//!
//! Each step reports what it actually did. A tool that is missing, fails to
//! install or is skipped is said so plainly rather than reported as set up.
//!
//! FIX-016 — `rtk init -g` configures Claude Code only; Codex and OpenCode
//! need `--codex` / `--opencode`. Every init used the Claude form, so the
//! other clients never got rtk. Verified against rtk 0.37.2: each form is
//! idempotent on re-run.
//!
//! FIX-012 — CodeGraph was only written into `.mcp.json` (Claude's project
//! file), so Codex and OpenCode projects never had it. It is now registered
//! through each client's native, project-scoped mechanism, pinned to the
//! project with `--path` (without it, `codegraph serve` looks for the index
//! from the client's working directory and finds nothing when the client
//! starts it elsewhere). Verified with the real clients:
//!
//! | client   | mechanism                                           | loads |
//! |----------|-----------------------------------------------------|-------|
//! | Claude   | `claude mcp add --scope local` (per-project, private) | at once |
//! | OpenCode | `mcp.codegraph` in `<project>/opencode.json`          | at once |
//! | Codex    | `[mcp_servers.codegraph]` in `<project>/.codex/config.toml` | once the project is trusted in Codex |
//!
//! Claude's `.mcp.json` is no longer used: servers declared there stay
//! "pending approval" until approved interactively, and the project cannot
//! approve itself.

use super::runner::on_path;
use colored::Colorize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Name of the CodeGraph server in every client's config.
pub(crate) const CODEGRAPH_SERVER: &str = "codegraph";

#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolOptions {
    /// Install missing tools (brew / npm / curl). `--no-install` turns this
    /// off: tools already present are still configured.
    pub install_missing: bool,
}

// ─── rtk ─────────────────────────────────────────────────────────────────────

/// `rtk init` argument lists for the scaffolded clients. `--opencode`
/// installs the OpenCode plugin *in addition to* the Claude setup, so it
/// subsumes the plain Claude form when both are wanted.
pub(crate) fn rtk_init_invocations(targets: &[&str]) -> Vec<Vec<&'static str>> {
    let mut out = Vec::new();
    let has = |t: &str| targets.contains(&t);
    if has("opencode") {
        out.push(vec!["init", "-g", "--opencode"]);
    } else if has("claude") {
        out.push(vec!["init", "-g"]);
    }
    if has("codex") {
        out.push(vec!["init", "-g", "--codex"]);
    }
    out
}

pub(crate) fn setup_rtk(targets: &[&str], opts: ToolOptions) {
    println!();
    println!("Setting up rtk…");
    if !on_path("rtk") {
        if !opts.install_missing {
            println!(
                "{} rtk not installed — skipped (--no-install)",
                "–".dimmed()
            );
            return;
        }
        if !install_rtk() {
            return;
        }
    } else {
        println!("{} rtk already installed", "–".dimmed());
    }

    for args in rtk_init_invocations(targets) {
        match Command::new("rtk").args(&args).status() {
            Ok(s) if s.success() => println!("{} rtk {}", "✓".green(), args.join(" ")),
            Ok(s) => eprintln!("  {} rtk {} exited with {s}", "⚠".yellow(), args.join(" ")),
            Err(e) => eprintln!("  {} rtk {} failed: {e}", "⚠".yellow(), args.join(" ")),
        }
    }
}

fn install_rtk() -> bool {
    if let Ok(s) = Command::new("brew").args(["install", "rtk"]).status() {
        if s.success() {
            println!("{} rtk installed via brew", "✓".green());
            return true;
        }
    }
    println!("  brew unavailable, trying curl installer…");
    let script =
        "curl -fsSL https://raw.githubusercontent.com/rtk-ai/rtk/refs/heads/master/install.sh | sh";
    match Command::new("sh").args(["-c", script]).status() {
        Ok(s) if s.success() => {
            println!("{} rtk installed via curl", "✓".green());
            true
        }
        Ok(s) => {
            eprintln!("  {} rtk install exited with {s}", "⚠".yellow());
            false
        }
        Err(e) => {
            eprintln!("  {} rtk install failed: {e}", "⚠".yellow());
            false
        }
    }
}

// ─── CodeGraph ───────────────────────────────────────────────────────────────

/// Arguments that start CodeGraph's MCP server pinned to `root`.
pub(crate) fn codegraph_server_args(root: &Path) -> Vec<String> {
    vec![
        "serve".into(),
        "--mcp".into(),
        "--path".into(),
        root.display().to_string(),
    ]
}

pub(crate) fn setup_codegraph(cwd: &Path, targets: &[&str], opts: ToolOptions) {
    println!();
    println!("Setting up codegraph…");

    if !on_path("codegraph") {
        if !opts.install_missing {
            println!(
                "{} codegraph not installed — skipped (--no-install); no MCP server registered",
                "–".dimmed()
            );
            return;
        }
        println!("  Installing @colbymchenry/codegraph via npm…");
        match Command::new("npm")
            .args(["install", "-g", "@colbymchenry/codegraph"])
            .status()
        {
            Ok(s) if s.success() => println!("{} codegraph installed", "✓".green()),
            Ok(s) => {
                eprintln!(
                    "  {} npm install exited with {s} — codegraph not set up",
                    "⚠".yellow()
                );
                return;
            }
            Err(e) => {
                eprintln!(
                    "  {} npm not found ({e}) — codegraph not set up",
                    "⚠".yellow()
                );
                return;
            }
        }
    } else {
        println!("{} codegraph already installed", "–".dimmed());
    }

    if !run_codegraph(cwd, &["init"], "codegraph init — .codegraph/ ready") {
        return;
    }
    println!("  Indexing codebase…");
    run_codegraph(
        cwd,
        &["index", "--quiet"],
        "codegraph index — codebase indexed",
    );

    let root = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    for target in targets {
        let result = match *target {
            "claude" => register_codegraph_claude(cwd, &root),
            "codex" => register_codegraph_codex(cwd, &root),
            "opencode" => register_codegraph_opencode(cwd, &root),
            _ => continue,
        };
        match result {
            Ok(msg) => println!("{} {msg}", "✓".green()),
            Err(msg) => eprintln!("  {} {target}: {msg}", "⚠".yellow()),
        }
    }
}

fn run_codegraph(cwd: &Path, args: &[&str], ok_msg: &str) -> bool {
    match Command::new("codegraph")
        .args(args)
        .current_dir(cwd)
        .status()
    {
        Ok(s) if s.success() => {
            println!("{} {ok_msg}", "✓".green());
            true
        }
        Ok(s) => {
            eprintln!(
                "  {} codegraph {} exited with {s}",
                "⚠".yellow(),
                args.join(" ")
            );
            false
        }
        Err(e) => {
            eprintln!(
                "  {} codegraph {} failed: {e}",
                "⚠".yellow(),
                args.join(" ")
            );
            false
        }
    }
}

/// `claude mcp add --scope local`: private to this user and this project,
/// and active immediately (a `.mcp.json` entry would wait for approval).
fn register_codegraph_claude(cwd: &Path, root: &Path) -> Result<String, String> {
    if !on_path("claude") {
        return Err("`claude` not on PATH — CodeGraph not registered for Claude Code".into());
    }
    // Replace a previous registration so the pinned path is current.
    let _ = Command::new("claude")
        .args(["mcp", "remove", "--scope", "local", CODEGRAPH_SERVER])
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let mut args: Vec<String> = [
        "mcp",
        "add",
        "--scope",
        "local",
        CODEGRAPH_SERVER,
        "--",
        "codegraph",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(codegraph_server_args(root));
    let out = Command::new("claude")
        .args(&args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("could not run `claude mcp add`: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`claude mcp add` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok("Claude Code — codegraph MCP registered (local scope)".into())
}

fn register_codegraph_opencode(cwd: &Path, root: &Path) -> Result<String, String> {
    let path = cwd.join("opencode.json");
    let updated = merge_opencode_codegraph(read_text(&path)?.as_deref(), root)?;
    write_text(&path, &updated)?;
    Ok("opencode.json — codegraph MCP registered".into())
}

fn register_codegraph_codex(cwd: &Path, root: &Path) -> Result<String, String> {
    let path = cwd.join(".codex").join("config.toml");
    let updated = merge_codex_codegraph(read_text(&path)?.as_deref().unwrap_or(""), root);
    write_text(&path, &updated)?;
    if codex_project_trusted(root) {
        Ok(".codex/config.toml — codegraph MCP registered".into())
    } else {
        Ok(format!(
            ".codex/config.toml — codegraph MCP registered. {} Codex loads project config only for \
             trusted projects: open `codex` in this directory once and trust it",
            "Note:".yellow()
        ))
    }
}

/// Add or replace `mcp.codegraph` in an `opencode.json`, keeping every other
/// key and server.
pub(crate) fn merge_opencode_codegraph(
    existing: Option<&str>,
    root: &Path,
) -> Result<String, String> {
    let mut doc: Value = match existing.map(str::trim) {
        None | Some("") => json!({ "$schema": "https://opencode.ai/config.json" }),
        Some(s) => serde_json::from_str(s)
            .map_err(|e| format!("opencode.json is not valid JSON ({e}); not modified"))?,
    };
    let obj = doc
        .as_object_mut()
        .ok_or("opencode.json root is not an object; not modified")?;
    let mcp = obj.entry("mcp").or_insert_with(|| json!({}));
    let mcp = mcp
        .as_object_mut()
        .ok_or("opencode.json `mcp` is not an object; not modified")?;
    let mut command = vec![Value::String("codegraph".into())];
    command.extend(codegraph_server_args(root).into_iter().map(Value::String));
    mcp.insert(
        CODEGRAPH_SERVER.into(),
        json!({ "type": "local", "command": command, "enabled": true }),
    );
    serde_json::to_string_pretty(&doc)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

/// Add or replace `[mcp_servers.codegraph]` in a Codex `config.toml`,
/// keeping every other section.
pub(crate) fn merge_codex_codegraph(existing: &str, root: &Path) -> String {
    let header = format!("[mcp_servers.{CODEGRAPH_SERVER}]");
    let base = crate::cli::mcp_register::strip_toml_section(existing, &header);
    let args = codegraph_server_args(root)
        .iter()
        .map(|a| toml_string(a))
        .collect::<Vec<_>>()
        .join(", ");
    let block = format!("{header}\ncommand = \"codegraph\"\nargs = [{args}]\n");
    let base = if base.trim().is_empty() {
        String::new()
    } else {
        base
    };
    crate::cli::mcp_register::append_block(&base, &block)
}

/// Whether `root` is marked trusted in the user's Codex config
/// (`$CODEX_HOME/config.toml`, default `~/.codex/config.toml`).
pub(crate) fn codex_project_trusted(root: &Path) -> bool {
    let Some(cfg) = crate::cli::mcp_register::codex_config_path() else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(cfg) else {
        return false;
    };
    trusted_in(&text, root)
}

fn trusted_in(codex_config: &str, root: &Path) -> bool {
    let mut candidates = vec![root.display().to_string()];
    if let Ok(c) = root.canonicalize() {
        candidates.push(c.display().to_string());
    }
    let headers: Vec<String> = candidates
        .iter()
        .map(|p| format!("[projects.{}]", toml_string(p)))
        .collect();
    let mut in_section = false;
    for line in codex_config.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = headers.iter().any(|h| h == t);
            continue;
        }
        if in_section {
            let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
            if compact == "trust_level=\"trusted\"" {
                return true;
            }
        }
    }
    false
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn read_text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

fn write_text(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    crate::state::write_atomic(path, content.as_bytes()).map_err(|e| format!("{e:#}"))
}

// ─── caveman ─────────────────────────────────────────────────────────────────

pub(crate) fn ensure_caveman(opts: ToolOptions) {
    let Some(home) = dirs::home_dir() else { return };
    let activate: PathBuf = home
        .join(".claude")
        .join("hooks")
        .join("caveman-activate.js");
    if activate.exists() {
        println!("{} caveman already installed", "–".dimmed());
        return;
    }
    let manual = if cfg!(target_os = "windows") {
        "irm https://raw.githubusercontent.com/JuliusBrussee/caveman/main/install.ps1 | iex"
    } else {
        "curl -fsSL https://raw.githubusercontent.com/JuliusBrussee/caveman/main/install.sh | bash"
    };
    if !opts.install_missing {
        println!(
            "{} caveman not installed — skipped (--no-install). Install: {manual}",
            "–".dimmed()
        );
        return;
    }

    println!();
    println!("Installing caveman (terse output mode)…");
    #[cfg(target_os = "windows")]
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            manual,
        ])
        .status();
    #[cfg(not(target_os = "windows"))]
    let status = Command::new("bash").arg("-c").arg(manual).status();

    match status {
        Ok(s) if s.success() => println!("{} caveman installed", "✓".green()),
        Ok(s) => eprintln!(
            "  {} caveman install exited {s}. Run manually: {manual}",
            "⚠".yellow()
        ),
        Err(e) => eprintln!(
            "  {} caveman install failed: {e}. Run manually: {manual}",
            "⚠".yellow()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtk_uses_the_flag_for_each_client() {
        assert_eq!(rtk_init_invocations(&["claude"]), vec![vec!["init", "-g"]]);
        assert_eq!(
            rtk_init_invocations(&["codex"]),
            vec![vec!["init", "-g", "--codex"]]
        );
        assert_eq!(
            rtk_init_invocations(&["opencode"]),
            vec![vec!["init", "-g", "--opencode"]]
        );
    }

    // `--opencode` already includes the Claude setup; no second Claude run.
    #[test]
    fn rtk_all_runs_opencode_form_and_codex_form() {
        assert_eq!(
            rtk_init_invocations(&["claude", "codex", "opencode"]),
            vec![
                vec!["init", "-g", "--opencode"],
                vec!["init", "-g", "--codex"],
            ]
        );
    }

    #[test]
    fn codegraph_is_pinned_to_the_project() {
        let args = codegraph_server_args(Path::new("/p/app"));
        assert_eq!(args, vec!["serve", "--mcp", "--path", "/p/app"]);
    }

    #[test]
    fn opencode_merge_keeps_other_servers_and_keys() {
        let existing = r#"{"$schema":"x","theme":"dark","mcp":{"zforge":{"type":"local","command":["zforge","mcp"]}}}"#;
        let out = merge_opencode_codegraph(Some(existing), Path::new("/p/app")).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcp"]["zforge"]["command"][0], "zforge");
        assert_eq!(
            v["mcp"]["codegraph"]["command"],
            json!(["codegraph", "serve", "--mcp", "--path", "/p/app"])
        );
    }

    #[test]
    fn opencode_merge_replaces_a_stale_codegraph_entry() {
        let existing =
            r#"{"mcp":{"codegraph":{"type":"local","command":["codegraph","serve","--mcp"]}}}"#;
        let out = merge_opencode_codegraph(Some(existing), Path::new("/new")).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcp"]["codegraph"]["command"][4], "/new");
    }

    #[test]
    fn opencode_merge_refuses_invalid_json() {
        assert!(merge_opencode_codegraph(Some("{nope"), Path::new("/p")).is_err());
    }

    #[test]
    fn codex_merge_keeps_other_sections_and_is_idempotent() {
        let existing = "[mcp_servers.zforge]\ncommand = \"zforge\"\nargs = [\"mcp\"]\n";
        let once = merge_codex_codegraph(existing, Path::new("/p/app"));
        assert!(once.contains("[mcp_servers.zforge]"));
        assert!(once.contains(
            "[mcp_servers.codegraph]\ncommand = \"codegraph\"\nargs = [\"serve\", \"--mcp\", \"--path\", \"/p/app\"]\n"
        ));
        let twice = merge_codex_codegraph(&once, Path::new("/p/app"));
        assert_eq!(once, twice);
        assert_eq!(twice.matches("[mcp_servers.codegraph]").count(), 1);
    }

    #[test]
    fn codex_merge_into_empty_file() {
        let out = merge_codex_codegraph("", Path::new("/p"));
        assert!(out.starts_with("[mcp_servers.codegraph]"));
    }

    #[test]
    fn codex_trust_is_read_from_the_projects_table() {
        let cfg = "[projects.\"/p/app\"]\ntrust_level = \"trusted\"\n\n[projects.\"/p/other\"]\ntrust_level = \"untrusted\"\n";
        assert!(trusted_in(cfg, Path::new("/p/app")));
        assert!(!trusted_in(cfg, Path::new("/p/other")));
        assert!(!trusted_in(cfg, Path::new("/p/missing")));
        assert!(!trusted_in("", Path::new("/p/app")));
    }
}
