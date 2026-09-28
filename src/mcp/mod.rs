//! zforge MCP server — stdio JSON-RPC 2.0.
//!
//! Tools exposed:
//!   project_list / project_add / project_remove / switch_project — the
//!     global project registry
//!   intake_* / change_new / readiness / knowledge_index / run_* — v1.5
//!     intake and runs (`v15`): preparation and observation only; accepting
//!     a revision or handing over stays on the CLI, where a terminal proves
//!     a human decided.

pub mod v15;

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

pub fn run() -> Result<()> {
    // MCP responses are JSON over stdout — ANSI color escapes would corrupt the stream
    // and pollute the text content rendered to the AI caller.
    colored::control::set_override(false);

    // Library code may print progress with `note!`; here stdout is the
    // JSON-RPC transport, so all of it goes to stderr.
    crate::cli::output::divert_to_stderr();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                respond(&mut out, parse_error(e))?;
                continue;
            }
        };

        // Notifications have no "id" — send no response
        let is_notification = req.get("id").is_none();
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if is_notification {
            continue;
        }

        let response = match method.as_str() {
            "initialize" => on_initialize(id),
            "tools/list" => on_tools_list(id),
            "tools/call" => on_tools_call(id, &req),
            other => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {other}") }
            }),
        };

        respond(&mut out, response)?;
    }

    Ok(())
}

// ─── protocol handlers ────────────────────────────────────────────────────────

fn on_initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "zforge", "version": env!("CARGO_PKG_VERSION") }
        }
    })
}

fn on_tools_list(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "tools": tool_definitions() }
    })
}

fn on_tools_call(id: Value, req: &Value) -> Value {
    let params = req.get("params").unwrap_or(&Value::Null);
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let result = match name.as_str() {
        "project_list" => tool_project_list(&args),
        "project_add" => tool_project_add(&args),
        "project_remove" => tool_project_remove(&args),
        "switch_project" => tool_switch_project(&args),
        // v1.5 intake and runs (preparation and observation only; the
        // user's decisions stay on the CLI — see `v15::FORBIDDEN`).
        other => match v15::dispatch(other, &args) {
            Some(result) => result,
            None => Err(anyhow::anyhow!("unknown tool: {other}")),
        },
    };

    match result {
        Ok(content) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": content }],
                "isError": false
            }
        }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!("Error: {e}") }],
                "isError": true
            }
        }),
    }
}

// ─── project registry tools ───────────────────────────────────────────────────

fn tool_project_list(_args: &Value) -> Result<String> {
    let r = crate::cli::project::list_data()?;
    Ok(serde_json::to_string_pretty(&r)?)
}

fn tool_project_add(args: &Value) -> Result<String> {
    let name = require_str(args, "name")?;
    let path = require_str(args, "path")?;
    let canon = crate::cli::project::add_entry(name, std::path::Path::new(path))?;
    Ok(format!("registered {name} -> {}", canon.display()))
}

fn tool_project_remove(args: &Value) -> Result<String> {
    let name = require_str(args, "name")?;
    let purge = args.get("purge").and_then(|v| v.as_bool()).unwrap_or(false);
    let removed_path = crate::cli::project::remove_entry(name)?;
    if purge {
        let zf = removed_path.join(".zforge");
        if crate::cli::project::purge_zforge_dir(&removed_path)? {
            Ok(format!("removed {name}; purged {}", zf.display()))
        } else {
            Ok(format!(
                "removed {name}; note: {} did not exist",
                zf.display()
            ))
        }
    } else {
        Ok(format!("removed {name}"))
    }
}

fn tool_switch_project(args: &Value) -> Result<String> {
    let name = require_str(args, "name")?;
    crate::cli::project::switch_to(name)?;
    Ok(format!("current_project: {name}"))
}

// ─── tool schemas ─────────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    let mut tools = registry_tool_definitions();
    if let Some(list) = tools.as_array_mut() {
        list.extend(v15::definitions());
    }
    tools
}

fn registry_tool_definitions() -> Value {
    json!([
        {
            "name": "project_list",
            "description": "List every project registered in ~/.zforge/registry.yaml.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "project_add",
            "description": "Register an existing zforge project (one whose .zforge/ already exists) in ~/.zforge/registry.yaml.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Unique project name (regex ^[a-zA-Z0-9_-]{1,64}$)" },
                    "path": { "type": "string", "description": "Absolute or relative path to the project root (must contain .zforge/)" }
                },
                "required": ["name", "path"]
            }
        },
        {
            "name": "project_remove",
            "description": "Remove a project from the registry. With purge=true, also delete its on-disk .zforge/ directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name":  { "type": "string" },
                    "purge": { "type": "boolean", "description": "Also delete the project's .zforge/ directory (irreversible)" }
                },
                "required": ["name"]
            }
        },
        {
            "name": "switch_project",
            "description": "Set the registry's current_project pointer.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            }
        }
    ])
}

fn respond(out: &mut impl Write, value: Value) -> Result<()> {
    writeln!(out, "{}", value)?;
    out.flush()?;
    Ok(())
}

fn parse_error(e: impl std::fmt::Display) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": { "code": -32700, "message": format!("parse error: {e}") }
    })
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {key}"))
}
