//! `zforge models` — which model runs each phase, per client.
//!
//! zforge names no model of its own: models change faster than a built-in
//! choice could follow. A phase runs on the model the user chose in
//! `models.yaml` — the project's `.zforge/models.yaml` over the machine's
//! `~/.zforge/models.yaml` — and otherwise on the client's own default.
//! `set` / `unset` edit one of those files in place (comments kept) and
//! render the project's agent definitions again, so the definitions never
//! lag behind the choice. Runs read `models.yaml` at every agent call.

use crate::config::{self, ModelsConfig};
use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};

pub const CLIENTS: [&str; 3] = ["claude", "codex", "opencode"];
pub const PHASES: [&str; 5] = ["spec", "testspec", "plan", "code", "review"];

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ModelsArgs {
    #[command(subcommand)]
    pub cmd: Option<ModelsCmd>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum ModelsCmd {
    /// Choose the model for a phase (`all` for every phase).
    Set {
        /// spec, testspec, plan, code, review — or `all`.
        phase: String,
        /// As the client names it: `opus`, `sonnet`, a full model id, …
        model: String,
        #[arg(long, default_value = "claude")]
        client: String,
        /// Every project on this machine (`~/.zforge/models.yaml`).
        #[arg(long)]
        global: bool,
    },
    /// Go back to the client's own default for a phase (`all` for every phase).
    Unset {
        phase: String,
        #[arg(long, default_value = "claude")]
        client: String,
        #[arg(long)]
        global: bool,
    },
}

pub fn run(args: ModelsArgs) -> Result<()> {
    let project = config::load().ok();
    match args.cmd {
        None => show(project.as_ref(), args.json),
        Some(ModelsCmd::Set {
            phase,
            model,
            client,
            global,
        }) => change(project.as_ref(), &phase, &client, Some(&model), global),
        Some(ModelsCmd::Unset {
            phase,
            client,
            global,
        }) => change(project.as_ref(), &phase, &client, None, global),
    }
}

fn global_path() -> Result<PathBuf> {
    Ok(crate::registry::paths::registry_dir()?.join("models.yaml"))
}

fn project_path(project: &config::Config) -> PathBuf {
    project.project_root().join(".zforge").join("models.yaml")
}

fn read_models(path: &Path) -> ModelsConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_yaml::from_str(&t).ok())
        .unwrap_or_default()
}

/// One row of `zforge models`.
#[derive(Debug, serde::Serialize)]
struct Row {
    client: &'static str,
    phase: &'static str,
    model: Option<String>,
    /// `project`, `global`, `agent definition`, or `client default`.
    from: &'static str,
}

fn show(project: Option<&config::Config>, json: bool) -> Result<()> {
    let global = read_models(&global_path()?);
    let local = project.map(|p| read_models(&project_path(p)));
    let mut rows = Vec::new();
    for client in CLIENTS {
        for phase in PHASES {
            let (model, from) = match (
                local.as_ref().and_then(|m| m.for_assistant(client, phase)),
                global.for_assistant(client, phase),
            ) {
                (Some(m), _) => (Some(m.to_string()), "project"),
                (None, Some(m)) => (Some(m.to_string()), "global"),
                (None, None) => {
                    match project
                        .and_then(|p| crate::cli::init::model_in_definition(p, client, phase))
                    {
                        Some(m) => (Some(m), "agent definition"),
                        None => (None, "client default"),
                    }
                }
            };
            rows.push(Row {
                client,
                phase,
                model,
                from,
            });
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    for r in &rows {
        let model = match &r.model {
            Some(m) => m.bold().to_string(),
            None => "—".dimmed().to_string(),
        };
        println!(
            "{:<9} {:<9} {model}  {}",
            r.client,
            r.phase,
            r.from.dimmed()
        );
    }
    println!(
        "\n{}",
        "A phase without a choice runs on the client's own default. \
         Choose with `zforge models set <phase|all> <model> [--client C] [--global]`."
            .dimmed()
    );
    Ok(())
}

fn change(
    project: Option<&config::Config>,
    phase: &str,
    client: &str,
    model: Option<&str>,
    global: bool,
) -> Result<()> {
    validate_name("client", client)?;
    let phases: Vec<&str> = match phase {
        "all" => PHASES.to_vec(),
        p if PHASES.contains(&p) => vec![p],
        other => bail!(
            "unknown phase `{other}`; one of {}, or `all`",
            PHASES.join(", ")
        ),
    };
    if let Some(m) = model {
        if m.trim().is_empty() || m.contains('\n') {
            bail!("a model name is one non-empty line");
        }
    }
    let path = match (global, project) {
        (true, _) => global_path()?,
        (false, Some(p)) => project_path(p),
        (false, None) => {
            bail!("not in a zforge project; pass --global to choose for every project")
        }
    };
    let before = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let mut text = before;
    for p in &phases {
        text = set_in_yaml(&text, client, p, model)?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::state::write_atomic(&path, text.as_bytes())?;
    let what = model.map_or("the client's own default".to_string(), |m| format!("`{m}`"));
    println!(
        "{} {client} {} → {what} ({})",
        "✓".green(),
        phases.join(", "),
        path.display()
    );
    if let Some(p) = project {
        let root = p.project_root();
        let dirs = crate::cli::init::rerender_agents(&root, p)?;
        if !dirs.is_empty() {
            println!(
                "{} agent definitions rendered again: {}",
                "✓".green(),
                dirs.join(", ")
            );
        }
    }
    if global {
        println!(
            "{}",
            "Runs everywhere use it now; other projects' agent definitions update on \
             their next `zforge models` change or `zforge init --force`."
                .dimmed()
        );
    }
    Ok(())
}

fn validate_name(what: &str, name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    match ok {
        true => Ok(()),
        false => Err(anyhow!("invalid {what} `{name}`")),
    }
}

/// `text` with `client.phase` set to `model`, or removed for `None`,
/// everything else — comments included — as it was. The result is parsed
/// back and must say exactly that, or nothing is returned.
pub fn set_in_yaml(text: &str, client: &str, phase: &str, model: Option<&str>) -> Result<String> {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let header = format!("{client}:");
    let is_key = |l: &str, key: &str| {
        l.strip_prefix(key)
            .is_some_and(|rest| rest.trim().is_empty() || rest.trim_start().starts_with('#'))
    };
    let top = lines.iter().position(|l| is_key(l, &header));
    let entry = |m: &str| format!("  {phase}: {}", yaml_scalar(m));
    match (top, model) {
        (None, None) => {}
        (None, Some(m)) => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(header);
            lines.push(entry(m));
        }
        (Some(start), _) => {
            // The block: the indented lines after the header.
            let end = lines[start + 1..]
                .iter()
                .position(|l| !l.is_empty() && !l.starts_with(' ') && !l.starts_with('\t'))
                .map_or(lines.len(), |i| start + 1 + i);
            let key = format!("{phase}:");
            let found = (start + 1..end).find(|&i| {
                let t = lines[i].trim_start();
                t.strip_prefix(&key)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
            });
            match (found, model) {
                (Some(i), Some(m)) => lines[i] = entry(m),
                (Some(i), None) => {
                    lines.remove(i);
                    let left = (start + 1..end - 1).any(|j| {
                        let t = lines[j].trim();
                        !t.is_empty() && !t.starts_with('#')
                    });
                    if !left {
                        lines.remove(start);
                    }
                }
                (None, Some(m)) => {
                    let last = (start + 1..end)
                        .rev()
                        .find(|&j| !lines[j].trim().is_empty())
                        .unwrap_or(start);
                    lines.insert(last + 1, entry(m));
                }
                (None, None) => {}
            }
        }
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    let parsed: ModelsConfig = match serde_yaml::from_str::<serde_yaml::Value>(&out) {
        Ok(serde_yaml::Value::Null) => ModelsConfig::default(),
        _ => serde_yaml::from_str(&out).context("the edited models.yaml does not parse")?,
    };
    if parsed.for_assistant(client, phase) != model {
        bail!("could not set {client}.{phase} in models.yaml; edit it by hand");
    }
    Ok(out)
}

/// A model name as a YAML scalar: plain when it is safely plain.
fn yaml_scalar(m: &str) -> String {
    let plain = m
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
        && !m.starts_with(['-', '.']);
    match plain {
        true => m.to_string(),
        false => format!("\"{}\"", m.replace('\\', "\\\\").replace('"', "\\\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("../../templates/models.yaml");

    fn models(text: &str) -> ModelsConfig {
        match serde_yaml::from_str::<serde_yaml::Value>(text).unwrap() {
            serde_yaml::Value::Null => ModelsConfig::default(),
            _ => serde_yaml::from_str(text).unwrap(),
        }
    }

    #[test]
    fn setting_on_the_template_keeps_its_comments() {
        let out = set_in_yaml(TEMPLATE, "claude", "code", Some("sonnet")).unwrap();
        assert!(out.starts_with(TEMPLATE.lines().next().unwrap()));
        assert!(out.contains("# Example:"));
        assert!(out.ends_with("claude:\n  code: sonnet\n"), "{out}");
        assert_eq!(models(&out).for_assistant("claude", "code"), Some("sonnet"));
    }

    #[test]
    fn a_block_is_extended_replaced_and_emptied() {
        let text = "# mine\nclaude:\n  plan: opus # thinking\n\ncodex:\n  code: x\n";
        let out = set_in_yaml(text, "claude", "code", Some("claude-sonnet-5")).unwrap();
        assert_eq!(
            out,
            "# mine\nclaude:\n  plan: opus # thinking\n  code: claude-sonnet-5\n\ncodex:\n  code: x\n"
        );
        let out = set_in_yaml(&out, "claude", "plan", Some("opus-next")).unwrap();
        assert!(out.contains("  plan: opus-next\n"));
        let out = set_in_yaml(&out, "claude", "plan", None).unwrap();
        let out = set_in_yaml(&out, "claude", "code", None).unwrap();
        assert_eq!(out, "# mine\n\ncodex:\n  code: x\n");
        let m = models(&out);
        assert_eq!(m.for_assistant("claude", "plan"), None);
        assert_eq!(m.for_assistant("codex", "code"), Some("x"));
    }

    #[test]
    fn names_that_are_not_plain_are_quoted() {
        let out = set_in_yaml("", "opencode", "code", Some("prov/model: v2")).unwrap();
        assert_eq!(out, "opencode:\n  code: \"prov/model: v2\"\n");
        assert_eq!(
            models(&out).for_assistant("opencode", "code"),
            Some("prov/model: v2")
        );
        let out = set_in_yaml("", "opencode", "code", Some("opencode-go/qwen3.5-plus")).unwrap();
        assert_eq!(out, "opencode:\n  code: opencode-go/qwen3.5-plus\n");
    }

    #[test]
    fn unsetting_what_is_not_there_changes_nothing() {
        assert_eq!(
            set_in_yaml(TEMPLATE, "claude", "code", None).unwrap(),
            TEMPLATE
        );
    }

    #[test]
    fn names_are_checked() {
        assert!(validate_name("client", "claude").is_ok());
        assert!(validate_name("client", "my_agent-2").is_ok());
        assert!(validate_name("client", "a b").is_err());
        assert!(validate_name("client", "").is_err());
    }
}
