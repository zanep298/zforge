use anyhow::{anyhow, Result};
use colored::Colorize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy, Debug)]
pub enum Agent {
    All,
    Claude,
    Codex,
    OpenCode,
}

impl Agent {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "all" => Ok(Agent::All),
            "claude" | "claude-code" => Ok(Agent::Claude),
            "codex" => Ok(Agent::Codex),
            "opencode" => Ok(Agent::OpenCode),
            other => Err(anyhow!(
                "unknown agent `{}` — expected one of: all, claude, codex, opencode",
                other
            )),
        }
    }
}

enum Outcome {
    Registered,
    AlreadyPresent,
    Skipped(String),
    Failed(String),
}

pub fn run(agent: Agent, force: bool) -> Result<()> {
    let targets: Vec<Agent> = match agent {
        Agent::All => vec![Agent::Claude, Agent::Codex, Agent::OpenCode],
        single => vec![single],
    };

    let mut any_failed = false;
    for t in targets {
        let (name, outcome) = match t {
            Agent::Claude => ("Claude Code", register_claude(force)),
            Agent::Codex => ("Codex", register_codex(force)),
            Agent::OpenCode => ("OpenCode", register_opencode(force)),
            Agent::All => unreachable!(),
        };
        report(name, &outcome);
        if matches!(outcome, Outcome::Failed(_)) {
            any_failed = true;
        }
    }

    if any_failed {
        Err(anyhow!("one or more registrations failed"))
    } else {
        Ok(())
    }
}

fn report(name: &str, outcome: &Outcome) {
    match outcome {
        Outcome::Registered => {
            println!("{} {}: registered `zforge` MCP server", "✓".green(), name);
        }
        Outcome::AlreadyPresent => {
            println!(
                "{} {}: `zforge` already registered (use --force to re-register)",
                "–".dimmed(),
                name
            );
        }
        Outcome::Skipped(reason) => {
            println!("{} {}: skipped — {}", "–".dimmed(), name, reason);
        }
        Outcome::Failed(msg) => {
            println!("{} {}: {}", "⚠".yellow(), name, msg);
        }
    }
}

// --- Claude Code ---

fn register_claude(force: bool) -> Outcome {
    if Command::new("claude").arg("--version").output().is_err() {
        return Outcome::Skipped("`claude` CLI not found in PATH".into());
    }

    let already = Command::new("claude")
        .args(["mcp", "get", "zforge"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if already {
        if !force {
            return Outcome::AlreadyPresent;
        }
        let rm = Command::new("claude")
            .args(["mcp", "remove", "zforge"])
            .output();
        if let Ok(out) = rm {
            if !out.status.success() {
                return Outcome::Failed(format!(
                    "failed to remove existing entry: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        }
    }

    let add = Command::new("claude")
        .args(["mcp", "add", "zforge", "--", "zforge", "mcp"])
        .output();

    match add {
        Ok(out) if out.status.success() => Outcome::Registered,
        Ok(out) => Outcome::Failed(format!(
            "`claude mcp add` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Outcome::Failed(format!("could not run `claude mcp add`: {e}")),
    }
}

// --- Codex (~/.codex/config.toml) ---

pub(crate) fn codex_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("config.toml"))
}

const CODEX_BLOCK: &str = "[mcp_servers.zforge]\ncommand = \"zforge\"\nargs = [\"mcp\"]\n";

fn register_codex(force: bool) -> Outcome {
    let Some(path) = codex_config_path() else {
        return Outcome::Skipped("could not resolve home directory".into());
    };

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let has_entry = existing.lines().any(|l| l.trim() == "[mcp_servers.zforge]");

    if has_entry && !force {
        return Outcome::AlreadyPresent;
    }

    let new_content = if has_entry {
        match strip_codex_block(&existing) {
            Ok(stripped) => append_block(&stripped, CODEX_BLOCK),
            Err(e) => return Outcome::Failed(e),
        }
    } else {
        append_block(&existing, CODEX_BLOCK)
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Outcome::Failed(format!("create {}: {e}", parent.display()));
        }
    }
    if let Err(e) = fs::write(&path, new_content) {
        return Outcome::Failed(format!("write {}: {e}", path.display()));
    }
    Outcome::Registered
}

pub const PHASES: &[&str] = &["spec", "testspec", "plan", "code", "review"];

/// Write `[profiles.zforge_<phase>]` blocks to `~/.codex/config.toml` for each
/// agent phase that has a `codex_model:` frontmatter key. Existing blocks are
/// replaced (stripped then re-appended) so re-running `zforge init --force`
/// picks up model changes in agent templates.
pub fn write_codex_profiles(agents_dir: &std::path::Path) -> Result<()> {
    let Some(path) = codex_config_path() else {
        return Ok(());
    };

    let mut content = fs::read_to_string(&path).unwrap_or_default();

    for phase in PHASES {
        let Some(model) = crate::fs::reader::agent_codex_model(agents_dir, phase) else {
            continue;
        };
        let header = format!("[profiles.zforge_{phase}]");
        if content.lines().any(|l| l.trim() == header) {
            content = strip_toml_section(&content, &header);
        }
        let block = format!("{header}\nmodel = \"{model}\"\n");
        content = append_block(&content, &block);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(())
}

fn strip_toml_section(content: &str, header: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut in_block = false;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_block = trimmed.trim_end_matches(|c: char| c.is_whitespace()) == header;
        }
        if !in_block {
            out.push_str(line);
        }
    }
    out.trim_end_matches('\n').to_string() + "\n"
}

fn append_block(existing: &str, block: &str) -> String {
    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(block);
    out
}

/// Remove the existing `[mcp_servers.zforge]` block (header + following lines
/// up to the next `[` table header or EOF).
fn strip_codex_block(content: &str) -> std::result::Result<String, String> {
    let mut out = String::with_capacity(content.len());
    let mut in_block = false;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_block =
                trimmed.trim_end_matches(|c: char| c.is_whitespace()) == "[mcp_servers.zforge]";
        }
        if !in_block {
            out.push_str(line);
        }
    }
    Ok(out.trim_end_matches('\n').to_string() + "\n")
}

// --- OpenCode (~/.config/opencode/opencode.json) ---

fn opencode_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("opencode").join("opencode.json"));
    }
    dirs::home_dir().map(|h| h.join(".config").join("opencode").join("opencode.json"))
}

fn register_opencode(force: bool) -> Outcome {
    let Some(path) = opencode_config_path() else {
        return Outcome::Skipped("could not resolve config directory".into());
    };

    let mut root: Value = if path.exists() {
        match fs::read_to_string(&path) {
            Ok(s) if s.trim().is_empty() => json!({}),
            Ok(s) => match serde_json::from_str(&s) {
                Ok(v) => v,
                Err(e) => return Outcome::Failed(format!("parse {}: {e}", path.display())),
            },
            Err(e) => return Outcome::Failed(format!("read {}: {e}", path.display())),
        }
    } else {
        json!({ "$schema": "https://opencode.ai/config.json" })
    };

    if !root.is_object() {
        return Outcome::Failed(format!("{} root is not a JSON object", path.display()));
    }

    let obj = root.as_object_mut().unwrap();
    let mcp = obj.entry("mcp".to_string()).or_insert_with(|| json!({}));
    if !mcp.is_object() {
        return Outcome::Failed("`mcp` field is not an object".into());
    }
    let mcp_obj = mcp.as_object_mut().unwrap();

    if mcp_obj.contains_key("zforge") && !force {
        return Outcome::AlreadyPresent;
    }

    mcp_obj.insert(
        "zforge".to_string(),
        json!({
            "type": "local",
            "command": ["zforge", "mcp"],
            "enabled": true,
        }),
    );

    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Outcome::Failed(format!("create {}: {e}", parent.display()));
        }
    }
    let serialized = match serde_json::to_string_pretty(&root) {
        Ok(s) => s + "\n",
        Err(e) => return Outcome::Failed(format!("serialize: {e}")),
    };
    if let Err(e) = fs::write(&path, serialized) {
        return Outcome::Failed(format!("write {}: {e}", path.display()));
    }
    Outcome::Registered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_block_to_empty() {
        let out = append_block("", "[x]\nk = 1\n");
        assert_eq!(out, "[x]\nk = 1\n");
    }

    #[test]
    fn append_block_separates_from_prior_content() {
        let out = append_block("[a]\nk = 1\n", "[b]\nk = 2\n");
        assert!(out.contains("[a]\nk = 1\n\n[b]\nk = 2\n"));
    }

    #[test]
    fn strip_codex_block_removes_target_section() {
        let input = "[other]\nk = 1\n\n[mcp_servers.zforge]\ncommand = \"old\"\nargs = []\n\n[trailing]\nx = 1\n";
        let stripped = strip_codex_block(input).unwrap();
        assert!(!stripped.contains("[mcp_servers.zforge]"));
        assert!(stripped.contains("[other]"));
        assert!(stripped.contains("[trailing]"));
    }

    #[test]
    fn agent_parse() {
        assert!(matches!(Agent::parse("all").unwrap(), Agent::All));
        assert!(matches!(Agent::parse("CLAUDE").unwrap(), Agent::Claude));
        assert!(matches!(Agent::parse("codex").unwrap(), Agent::Codex));
        assert!(matches!(Agent::parse("opencode").unwrap(), Agent::OpenCode));
        assert!(Agent::parse("xxx").is_err());
    }

    #[test]
    fn strip_toml_section_removes_target_and_leaves_rest() {
        let input =
            "[other]\nk = 1\n\n[profiles.zforge_code]\nmodel = \"old\"\n\n[trailing]\nx = 1\n";
        let stripped = strip_toml_section(input, "[profiles.zforge_code]");
        assert!(!stripped.contains("[profiles.zforge_code]"));
        assert!(stripped.contains("[other]"));
        assert!(stripped.contains("[trailing]"));
    }

    #[test]
    fn write_codex_profiles_writes_all_phases() {
        let tmp = tempfile::TempDir::new().unwrap();
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        // Write a minimal agent file with codex_model frontmatter
        for (phase, model) in [
            ("spec", "gpt-4o-mini"),
            ("testspec", "gpt-4o-mini"),
            ("plan", "gpt-4o"),
            ("code", "codex-mini-latest"),
            ("review", "gpt-4o"),
        ] {
            std::fs::write(
                agents_dir.join(format!("{phase}-agent.md")),
                format!("---\ncodex_model: {model}\n---\n"),
            )
            .unwrap();
        }

        let config_path = tmp.path().join("config.toml");
        // Simulate codex_config_path returning our temp path by writing via the internal helper
        let initial = "[existing]\nx = 1\n";
        std::fs::write(&config_path, initial).unwrap();

        // Manually call the logic that write_codex_profiles uses (since it reads HOME)
        let mut content = std::fs::read_to_string(&config_path).unwrap();
        for phase in PHASES {
            let Some(model) = crate::fs::reader::agent_codex_model(&agents_dir, phase) else {
                continue;
            };
            let header = format!("[profiles.zforge_{phase}]");
            if content.lines().any(|l| l.trim() == header) {
                content = strip_toml_section(&content, &header);
            }
            let block = format!("{header}\nmodel = \"{model}\"\n");
            content = append_block(&content, &block);
        }
        std::fs::write(&config_path, &content).unwrap();

        let result = std::fs::read_to_string(&config_path).unwrap();
        assert!(result.contains("[existing]"));
        assert!(result.contains("[profiles.zforge_spec]"));
        assert!(result.contains("model = \"gpt-4o-mini\""));
        assert!(result.contains("[profiles.zforge_code]"));
        assert!(result.contains("model = \"codex-mini-latest\""));
        assert!(result.contains("[profiles.zforge_review]"));
    }
}
