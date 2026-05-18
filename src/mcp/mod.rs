/// zforge MCP server — stdio JSON-RPC 2.0
///
/// Exposes zforge pipeline operations as MCP tools so OpenCode's AI can
/// orchestrate the full workflow from inside the TUI without leaving it.
///
/// Tools exposed:
///   task_import  — create a task
///   get_prompt   — render prompt for spec/testspec/plan/code/review
///   approve      — mark an artifact as reviewed
///   verify       — run the test suite
///   ship         — advance Coded + run verify in one call (saves a round trip)
///   status       — get task phase status
use crate::config;
use crate::prompt::{build_context_for_phase, Engine, PromptPhase};
use crate::state::{State, TaskState};
use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

pub fn run() -> Result<()> {
    // MCP responses are JSON over stdout — ANSI color escapes would corrupt the stream
    // and pollute the text content rendered to the AI caller.
    colored::control::set_override(false);

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
        "task_import" => tool_task_import(&args),
        "get_prompt" => tool_get_prompt(&args),
        "approve" => tool_approve(&args),
        "verify" => tool_verify(&args),
        "ship" => tool_ship(&args),
        "status" => tool_status(&args),
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
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

// ─── tool implementations ─────────────────────────────────────────────────────

fn tool_task_import(args: &Value) -> Result<String> {
    let task_id = args.get("task_id").and_then(|v| v.as_str());
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let domain = args
        .get("domain")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let jira_url = args
        .get("jira_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let figma_url = args
        .get("figma_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let figma_context = args
        .get("figma_context")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // Resolve task_id: explicit > extracted from jira_url > auto-generated
    let resolved_id_hint = task_id.map(str::to_string).or_else(|| {
        jira_url
            .as_deref()
            .and_then(crate::jira::extract_key_from_url)
    });

    let id = crate::cli::task::run_import(
        resolved_id_hint.as_deref(),
        title,
        domain,
        description,
        jira_url,
        figma_url,
        figma_context,
    )?;

    Ok(format!(
        "Task {id} created at .zforge/tasks/{id}/task.md\n\
         Fill in any missing details, then call get_prompt(phase=\"spec\", task_id=\"{id}\")."
    ))
}

fn tool_get_prompt(args: &Value) -> Result<String> {
    let phase = require_str(args, "phase")?;
    let task_id = require_str(args, "task_id")?;

    let valid = ["spec", "testspec", "plan", "code", "review"];
    if !valid.contains(&phase) {
        anyhow::bail!("phase must be one of: {}", valid.join(", "));
    }

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;

    let prompt_phase = match phase {
        "spec" => PromptPhase::Spec,
        "testspec" => PromptPhase::Testspec,
        "plan" => PromptPhase::Plan,
        "code" => PromptPhase::Code,
        "review" => PromptPhase::Review,
        _ => unreachable!(),
    };

    let mut ctx = build_context_for_phase(&config, task_id, prompt_phase)?;
    ctx.output_file = match phase {
        "code" => String::new(),
        "review" => format!(".zforge/tasks/{task_id}/review-summary.md"),
        _ => format!(".zforge/tasks/{task_id}/{phase}.md"),
    };
    ctx.next_command = next_cmd(phase, task_id);

    let engine = Engine::new(&config.agents_dir());
    let prompt = engine.render(phase, &ctx)?;

    let output_line = if ctx.output_file.is_empty() {
        String::new()
    } else {
        format!(
            "Process this prompt and write the output to `{}`.\n",
            ctx.output_file
        )
    };

    Ok(format!(
        "## Prompt for phase: {phase} / task: {task_id}\n\n\
         {output_line}\
         When done, follow the next step: {}.\n\n\
         ---\n\n{prompt}",
        ctx.next_command,
    ))
}

fn tool_approve(args: &Value) -> Result<String> {
    let task_id = require_str(args, "task_id")?;
    let artifact = require_str(args, "artifact")?;
    let note = args
        .get("note")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    crate::cli::approve::run(task_id, artifact, note, true)?;

    Ok(format!("{artifact} approved for task {task_id}."))
}

fn tool_verify(args: &Value) -> Result<String> {
    let task_id = require_str(args, "task_id")?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(600);

    crate::cli::verify::run(task_id, command, timeout)?;

    // verify::run prints results and writes verify.md; we report success
    Ok(format!(
        "Tests run for task {task_id}. See .zforge/tasks/{task_id}/verify.md for full results."
    ))
}

/// MCP-side ship: orchestrator has already written code. Advance PlanReviewed → Coded
/// (if not already) and run verify. Saves one tool round trip vs. separate calls.
fn tool_ship(args: &Value) -> Result<String> {
    let task_id = require_str(args, "task_id")?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(600);

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let tasks_dir = config.tasks_dir();
    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("task {task_id} not found"))?;

    if ts.state < State::PlanReviewed {
        anyhow::bail!(
            "BLOCKED: plan.md requires human review. Call approve(task_id=\"{task_id}\", artifact=\"plan\") first."
        );
    }

    if ts.state < State::Coded {
        ts.advance(State::Coded, "code phase complete (ship)")?;
        ts.save(&tasks_dir)?;
    }

    crate::cli::verify::run(task_id, command, timeout)?;

    Ok(format!(
        "Ship complete for task {task_id}. See .zforge/tasks/{task_id}/verify.md for results. Next: get_prompt(phase=\"review\", task_id=\"{task_id}\"). Optionally call approve(task_id=\"{task_id}\", artifact=\"verify\") first to mark verify.md reviewed."
    ))
}

fn tool_status(args: &Value) -> Result<String> {
    let task_id = args
        .get("task_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    crate::cli::status::render(task_id, false, false)
}

// ─── tool schema definitions ──────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!([
        {
            "name": "task_import",
            "description": "Create a new zf task.\n\
                            - For Jira tickets: use the Atlassian Rovo MCP to fetch the issue first \
                            (summary → title, description → description, components → domain/affected_services), \
                            then call this tool with those fields plus jira_url. task_id defaults to the Jira key.\n\
                            - For UI tasks with a Figma link: use the Figma MCP (get_code or get_metadata) \
                            to fetch the node first, then pass the result as figma_context and the node URL as figma_url. \
                            figma_context is injected into spec and code prompts automatically.\n\
                            - For manual tasks: pass task_id (or omit for auto-generate), title, and domain.\n\
                            After creating, call get_prompt(phase=\"spec\", task_id=<id>).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id":       { "type": "string", "description": "Task ID (e.g. TASK-001 or PROJ-123). Auto-generated if omitted." },
                    "title":         { "type": "string", "description": "One-line task title (Jira summary)" },
                    "domain":        { "type": "string", "description": "Bounded context / area (e.g. auth, payments, ui)" },
                    "description":   { "type": "string", "description": "Task description body (Jira description as plain text)" },
                    "jira_url":      { "type": "string", "description": "Full Jira ticket URL, e.g. https://company.atlassian.net/browse/PROJ-123" },
                    "figma_url":     { "type": "string", "description": "Figma node URL, e.g. https://figma.com/design/FILE/...?node-id=..." },
                    "figma_context": { "type": "string", "description": "Design context pre-fetched via Figma MCP (get_code/get_metadata output). Injected into spec and code prompts." }
                }
            }
        },
        {
            "name": "get_prompt",
            "description": "Render the AI prompt for a pipeline phase. Process the returned prompt yourself and write output to the specified file. Phases must run in order: spec → testspec → approve testspec → plan → approve plan → code → ship/verify → review. verify may also be approved to mark verify.md reviewed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "phase":   { "type": "string", "enum": ["spec", "testspec", "plan", "code", "review"] },
                    "task_id": { "type": "string" }
                },
                "required": ["phase", "task_id"]
            }
        },
        {
            "name": "approve",
            "description": "Approve an artifact. testspec unlocks planning, plan unlocks coding, and verify marks verify.md as reviewed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id":  { "type": "string" },
                    "artifact": { "type": "string", "enum": ["testspec", "plan", "verify"] },
                    "note":     { "type": "string", "description": "Optional approval note" }
                },
                "required": ["task_id", "artifact"]
            }
        },
        {
            "name": "verify",
            "description": "Run the project test suite and record pass/fail results.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "command": { "type": "string", "description": "Override test command" },
                    "timeout": { "type": "number", "description": "Timeout in seconds (default 600)" }
                },
                "required": ["task_id"]
            }
        },
        {
            "name": "ship",
            "description": "After you've written all code from get_prompt(phase=\"code\"), call this to advance state to Coded and run the test suite in one step. Saves a tool round trip vs. separate state-advance + verify calls.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "command": { "type": "string", "description": "Override test command" },
                    "timeout": { "type": "number", "description": "Timeout in seconds (default 600)" }
                },
                "required": ["task_id"]
            }
        },
        {
            "name": "status",
            "description": "Show current phase progress for a task or all tasks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Omit for all tasks" }
                }
            }
        }
    ])
}

// ─── helpers ──────────────────────────────────────────────────────────────────

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

fn next_cmd(phase: &str, task_id: &str) -> String {
    match phase {
        "spec" => format!("get_prompt(phase=\"testspec\", task_id=\"{task_id}\")"),
        "testspec" => format!("approve(task_id=\"{task_id}\", artifact=\"testspec\")"),
        "plan" => format!("approve(task_id=\"{task_id}\", artifact=\"plan\")"),
        "code" => format!("ship(task_id=\"{task_id}\")"),
        "review" => "Pipeline complete!".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_schema_matches_cli_artifacts() {
        let tools = tool_definitions();
        let approve_tool = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("approve"))
            .unwrap();

        let artifact_enum = approve_tool["inputSchema"]["properties"]["artifact"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(artifact_enum, vec!["testspec", "plan", "verify"]);
    }

    #[test]
    fn next_cmd_follows_current_state_machine() {
        let task_id = "TASK-001";

        assert_eq!(
            next_cmd("spec", task_id),
            "get_prompt(phase=\"testspec\", task_id=\"TASK-001\")"
        );
        assert_eq!(
            next_cmd("testspec", task_id),
            "approve(task_id=\"TASK-001\", artifact=\"testspec\")"
        );
        assert_eq!(
            next_cmd("plan", task_id),
            "approve(task_id=\"TASK-001\", artifact=\"plan\")"
        );
        assert_eq!(next_cmd("code", task_id), "ship(task_id=\"TASK-001\")");
        assert_eq!(next_cmd("review", task_id), "Pipeline complete!");
    }
}
