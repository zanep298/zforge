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
use crate::state::{Flow, State, TaskState};
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
        "project_list" => tool_project_list(&args),
        "project_add" => tool_project_add(&args),
        "project_remove" => tool_project_remove(&args),
        "switch_project" => tool_switch_project(&args),
        "global_status" => tool_global_status(&args),
        "ship_async" => tool_ship_async(&args),
        "job_status" => tool_job_status(&args),
        "job_wait" => tool_job_wait(&args),
        "job_log" => tool_job_log(&args),
        "job_cancel" => tool_job_cancel(&args),
        "job_list" => tool_job_list(&args),
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
    let flow = args
        .get("flow")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let agent = args
        .get("agent")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let fallback = args
        .get("fallback")
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
        flow.as_deref(),
        agent,
        fallback,
    )?;

    // Recommend the next MCP call based on the resolved flow.
    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let ts = TaskState::load(&config.tasks_dir(), &id)?;
    let next_phase = first_phase_of(&ts.flow).unwrap_or("spec");
    Ok(format!(
        "Task {id} created at .zforge/tasks/{id}/task.md (flow: {})\n\
         Fill in any missing details, then call get_prompt(phase=\"{next_phase}\", task_id=\"{id}\").",
        ts.flow.as_str()
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

    // Load flow so the "next" hint matches this task's pipeline preset.
    let ts = TaskState::load(&config.tasks_dir(), task_id)?;

    let mut ctx = build_context_for_phase(&config, task_id, prompt_phase)?;
    ctx.output_file = match phase {
        "code" => String::new(),
        "review" => format!(".zforge/tasks/{task_id}/review-summary.md"),
        _ => format!(".zforge/tasks/{task_id}/{phase}.md"),
    };
    ctx.next_command = next_cmd_with_flow(ts.flow, phase, task_id);

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

    let outcome = crate::cli::verify::run_with_outcome(task_id, command, timeout)?;

    // Surface failure as a tool-call error so the orchestrating LLM does not
    // mistake an internal test failure for a successful verify step.
    if !outcome.passed {
        anyhow::bail!(verify_failure_message(task_id, &outcome));
    }

    Ok(format!(
        "Tests passed for task {task_id} ({}/{}). See .zforge/tasks/{task_id}/verify.md for full results.",
        outcome.passed_tests, outcome.total_tests
    ))
}

/// MCP-side ship. Two modes:
///   - default (`max_iterations` absent or 1): orchestrator agent has already
///     written code; advance to Coded + run verify. One round trip.
///   - verifier loop (`max_iterations > 1`): MCP server itself drives the
///     code → verify cycle, re-prompting the assigned agent with failure
///     feedback until tests pass or budget runs out. The orchestrator agent
///     hands off control and gets a single success/failure result back.
fn tool_ship(args: &Value) -> Result<String> {
    let task_id = require_str(args, "task_id")?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(600);
    let max_iterations = args
        .get("max_iterations")
        .and_then(|v| v.as_u64())
        .map(|n| n.max(1) as u32)
        .unwrap_or(1);

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let tasks_dir = config.tasks_dir();
    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("task {task_id} not found"))?;

    crate::cli::ship::check_ship_gate(&ts.flow, &ts.state, task_id).map_err(|e| {
        // Rephrase for the MCP audience so the LLM gets the correct next-call hint.
        anyhow::anyhow!("{e}. Resolve the blocking step before retrying ship.")
    })?;

    if max_iterations == 1 {
        // Legacy: caller already wrote code. Just mark Coded + verify.
        crate::cli::ship::ensure_coded_state(&mut ts, &tasks_dir, "code phase complete (ship)")?;
        let outcome = crate::cli::verify::run_with_outcome(task_id, command, timeout)?;
        if !outcome.passed {
            anyhow::bail!(verify_failure_message(task_id, &outcome));
        }
        return Ok(format!(
            "Ship complete for task {task_id} ({} tests passed). See .zforge/tasks/{task_id}/verify.md for results. Next: get_prompt(phase=\"review\", task_id=\"{task_id}\"). Optionally call approve(task_id=\"{task_id}\", artifact=\"verify\") first to mark verify.md reviewed.",
            outcome.passed_tests
        ));
    }

    // Verifier loop: server drives code+verify iterations. CLI `ship` already
    // implements the loop with full agent dispatch; delegate to it so MCP and
    // CLI share the same code path.
    crate::cli::ship::run(task_id, command, timeout, max_iterations)?;
    Ok(format!(
        "Ship complete for task {task_id} after up to {max_iterations} verifier iteration(s). \
         See .zforge/tasks/{task_id}/verify.md. Next: get_prompt(phase=\"review\", task_id=\"{task_id}\")."
    ))
}

fn verify_failure_message(task_id: &str, outcome: &crate::cli::verify::VerifyOutcome) -> String {
    let failed_block = if outcome.failed_names.is_empty() {
        String::new()
    } else {
        let names = outcome
            .failed_names
            .iter()
            .map(|n| format!("  - {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\nFailed tests:\n{names}")
    };
    format!(
        "Tests FAILED for task {task_id} ({} failed / {} total). See .zforge/tasks/{task_id}/verify.md for full output.{failed_block}",
        outcome.failed_tests, outcome.total_tests
    )
}

fn tool_status(args: &Value) -> Result<String> {
    let task_id = args
        .get("task_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    crate::cli::status::render(task_id, false, false)
}

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

fn tool_global_status(args: &Value) -> Result<String> {
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(2000);
    let (rows, warnings) = crate::cli::status::collect_global(timeout_ms)?;
    let payload = json!({ "rows": rows, "warnings": warnings });
    Ok(serde_json::to_string_pretty(&payload)?)
}

// ─── job tools ────────────────────────────────────────────────────────────────

fn tool_ship_async(args: &Value) -> Result<String> {
    let task_id = require_str(args, "task_id")?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(600);
    let max_iterations = args
        .get("max_iterations")
        .and_then(|v| v.as_u64())
        .map(|n| n.max(1) as u32)
        .unwrap_or(1);

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let tasks_dir = config.tasks_dir();
    let ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("task {task_id} not found"))?;
    crate::cli::ship::check_ship_gate(&ts.flow, &ts.state, task_id)?;

    let job = crate::job::store::create_job(
        &config,
        task_id,
        crate::job::schema::JobKind::Ship,
        command,
        timeout,
        max_iterations,
    )?;
    let pid = crate::job::spawn::spawn_worker(&config, &job.job_id)?;
    Ok(format!(
        "Job {} spawned (pid {}). Poll: job_status(job_id=\"{}\"); block: job_wait(job_id=\"{}\").",
        job.job_id, pid, job.job_id, job.job_id
    ))
}

fn tool_job_status(args: &Value) -> Result<String> {
    let job_id = require_str(args, "job_id")?;
    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    crate::job::lifecycle::reconcile_dead_worker(&config, job_id)?;
    let job = crate::job::store::load_job(&config, job_id)?;
    Ok(serde_json::to_string_pretty(&job)?)
}

fn tool_job_wait(args: &Value) -> Result<String> {
    let job_id = require_str(args, "job_id")?;
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(60_000);
    let poll_ms = args.get("poll_ms").and_then(|v| v.as_u64()).unwrap_or(500);

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        crate::job::lifecycle::reconcile_dead_worker(&config, job_id)?;
        let job = crate::job::store::load_job(&config, job_id)?;
        if job.status.is_terminal() {
            return Ok(serde_json::to_string_pretty(&job)?);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "wait timeout after {}ms; job {} still {}",
                timeout_ms,
                job_id,
                job.status.as_str()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}

fn tool_job_log(args: &Value) -> Result<String> {
    let job_id = require_str(args, "job_id")?;
    let tail = args
        .get("tail")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);

    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let path = crate::job::store::job_log_path(&config, job_id);
    let content =
        std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("read log {path:?}: {e}"))?;
    let out = match tail {
        Some(n) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].join("\n")
        }
        None => content,
    };
    Ok(out)
}

fn tool_job_cancel(args: &Value) -> Result<String> {
    let job_id = require_str(args, "job_id")?;
    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let job = crate::job::store::load_job(&config, job_id)?;
    if job.status.is_terminal() {
        return Ok(format!(
            "job {job_id} already {}; no-op",
            job.status.as_str()
        ));
    }
    if let Some(pid) = job.worker_pid {
        if crate::job::lifecycle::pid_alive(pid) {
            // Negative PID = process group signal. Worker's pgrp leader is
            // its own PID (see job::spawn::spawn_worker). This kills the
            // worker AND any child agent process it spawned, preventing
            // orphan claude/codex subprocesses from outliving cancel.
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
    }
    crate::job::lifecycle::mark_cancelled(&config, job_id)?;
    Ok(format!("cancelled {job_id}"))
}

fn tool_job_list(_args: &Value) -> Result<String> {
    let config = config::load().map_err(|_| anyhow::anyhow!("config not found — run: zf init"))?;
    let jobs = crate::job::store::list_jobs(&config)?;
    Ok(serde_json::to_string_pretty(&jobs)?)
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
                    "figma_context": { "type": "string", "description": "Design context pre-fetched via Figma MCP (get_code/get_metadata output). Injected into spec and code prompts." },
                    "flow":          { "type": "string", "enum": ["full", "fixbug", "spike", "docs"], "description": "Pipeline preset. Defaults to 'full'. 'fixbug' skips plan/review, 'spike' skips tests, 'docs' goes straight from import to code." },
                    "agent":         { "type": "string", "description": "Primary agent name. Must exist in ~/.zforge/registry.yaml agents{} map. Persisted to .state.yaml as assigned_agent + active_agent." },
                    "fallback":      { "type": "string", "description": "Fallback agent for retryable failures (PR3 orchestrator). Must differ from agent and exist in the registry." }
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
            "description": "Default mode: after you've written code from get_prompt(phase=\"code\"), call this to advance to Coded + run tests in one step.\n\nVerifier-loop mode (max_iterations > 1): server drives the SWE-bench pattern itself — runs your task's assigned agent through code → verify → if fail, re-prompt agent with failed test names + verify.md feedback → repeat until pass or budget exhausted. Use this when you want the test suite to be ground truth and let the agent self-correct.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id":        { "type": "string" },
                    "command":        { "type": "string", "description": "Override test command" },
                    "timeout":        { "type": "number", "description": "Timeout in seconds (default 600)" },
                    "max_iterations": { "type": "number", "description": "Verifier loop iteration cap. 1 (default) = single-shot. > 1 = enable self-correcting loop. Requires task to have an assigned_agent." }
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
        },
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
        },
        {
            "name": "global_status",
            "description": "Aggregate active tasks across every registered project. Returns {rows, warnings}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "timeout_ms": { "type": "number", "description": "Per-project scan timeout in milliseconds (default 2000)" }
                }
            }
        },
        {
            "name": "ship_async",
            "description": "Detached ship. Spawns a background worker that runs code → verify (optionally the verifier loop when max_iterations > 1) and returns immediately with a job_id. Recommended for long-running tasks so the tool call doesn't block the orchestrator. Poll with job_status / job_wait.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id":        { "type": "string" },
                    "command":        { "type": "string", "description": "Override test command" },
                    "timeout":        { "type": "number", "description": "Test timeout in seconds (default 600)" },
                    "max_iterations": { "type": "number", "description": "Verifier loop budget. Default 1." }
                },
                "required": ["task_id"]
            }
        },
        {
            "name": "job_status",
            "description": "Read a job's current state. Detects dead workers (PID gone) and flips them to failed before returning.",
            "inputSchema": {
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
                "required": ["job_id"]
            }
        },
        {
            "name": "job_wait",
            "description": "Block until a job reaches a terminal state (success / failed / cancelled / timeout). Returns the final job record.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "job_id":     { "type": "string" },
                    "timeout_ms": { "type": "number", "description": "Maximum block time in ms. Default 60000." },
                    "poll_ms":    { "type": "number", "description": "Poll interval in ms. Default 500." }
                },
                "required": ["job_id"]
            }
        },
        {
            "name": "job_log",
            "description": "Read a job's combined stdout+stderr log. Optionally tail the last N lines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" },
                    "tail":   { "type": "number", "description": "Return only the last N lines" }
                },
                "required": ["job_id"]
            }
        },
        {
            "name": "job_cancel",
            "description": "SIGTERM the worker, mark the job cancelled. Idempotent on already-terminal jobs.",
            "inputSchema": {
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
                "required": ["job_id"]
            }
        },
        {
            "name": "job_list",
            "description": "List every job in the current project's .zforge/jobs/. Sorted newest first.",
            "inputSchema": {
                "type": "object",
                "properties": {}
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

#[cfg(test)]
fn next_cmd(phase: &str, task_id: &str) -> String {
    next_cmd_with_flow(Flow::Full, phase, task_id)
}

/// Flow-aware next-tool hint. Differs from the legacy `next_cmd` by
/// honoring short flows: e.g. `code` on a fixbug task points at `ship`,
/// `spec` on a docs task points straight at `ship`.
fn next_cmd_with_flow(flow: Flow, phase: &str, task_id: &str) -> String {
    // Map the just-rendered phase to the FSM state that completing it produces.
    let completed_state = match phase {
        "spec" => State::SpecDone,
        "testspec" => State::TestspecDone,
        "plan" => State::Planned,
        "code" => State::Coded,
        "review" => State::Reviewed,
        _ => return String::new(),
    };

    // The orchestrator hasn't yet advanced FSM state via MCP — so suggest
    // the tool that gates the *next* logical step, not literally the next
    // state after the current FSM position.
    let next_state = flow.next_after(&completed_state);
    match (phase, next_state) {
        ("code", _) => format!("ship(task_id=\"{task_id}\")"),
        ("review", _) | (_, None) => "Pipeline complete!".to_string(),
        (_, Some(State::TestspecReviewed)) => {
            format!("approve(task_id=\"{task_id}\", artifact=\"testspec\")")
        }
        (_, Some(State::PlanReviewed)) => {
            format!("approve(task_id=\"{task_id}\", artifact=\"plan\")")
        }
        (_, Some(State::TestspecDone)) => {
            format!("get_prompt(phase=\"testspec\", task_id=\"{task_id}\")")
        }
        (_, Some(State::Planned)) => format!("get_prompt(phase=\"plan\", task_id=\"{task_id}\")"),
        (_, Some(State::Coded)) => format!("get_prompt(phase=\"code\", task_id=\"{task_id}\")"),
        (_, Some(State::Verified)) => format!("verify(task_id=\"{task_id}\")"),
        (_, Some(State::Reviewed)) => {
            format!("get_prompt(phase=\"review\", task_id=\"{task_id}\")")
        }
        _ => String::new(),
    }
}

/// The first phase prompt that should be rendered for a freshly-imported
/// task on this flow. Used by `task_import` to point the LLM at the right
/// next tool call without hardcoding the legacy full-flow path.
fn first_phase_of(flow: &Flow) -> Option<&'static str> {
    // The flow's first non-Imported state determines the first phase prompt.
    let next = flow.next_after(&State::Imported)?;
    Some(match next {
        State::SpecDone => "spec",
        State::Coded => "code",
        _ => "spec",
    })
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
    fn verify_failure_message_includes_failed_names() {
        let outcome = crate::cli::verify::VerifyOutcome {
            passed: false,
            total_tests: 3,
            passed_tests: 1,
            failed_tests: 2,
            failed_names: vec!["mod::a".into(), "mod::b".into()],
        };
        let msg = verify_failure_message("TASK-1", &outcome);
        assert!(msg.contains("TASK-1"));
        assert!(msg.contains("2 failed / 3 total"));
        assert!(msg.contains("mod::a"));
        assert!(msg.contains("mod::b"));
    }

    #[test]
    fn verify_failure_message_omits_block_when_no_names() {
        let outcome = crate::cli::verify::VerifyOutcome {
            passed: false,
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            failed_names: vec![],
        };
        let msg = verify_failure_message("TASK-1", &outcome);
        assert!(!msg.contains("Failed tests:"));
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
