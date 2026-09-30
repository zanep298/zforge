//! Read a Claude Code `--output-format stream-json --verbose` run.
//!
//! The stream opens with a `system/init` event (client version, model,
//! MCP server status, agent and skill catalogs), carries every tool call as a
//! `tool_use` block of an `assistant` event — subagents' calls included,
//! tagged with `parent_tool_use_id` — reports refusals as
//! `system/permission_denied`, and ends with a `result` event. A stream
//! without that `result` did not finish.
//!
//! Claude does not report which skills were injected into an agent through
//! its `skills:` frontmatter; only the catalog is visible. That gap is
//! recorded as not observable rather than guessed.

use super::schema::{Expected, Finding, FindingKind, McpServer, Observed, RunResult};
use serde_json::Value;

/// Stderr lines Claude prints when it drops configuration for the run.
const WARNING_MARKERS: [&str; 2] = ["Ignoring ", "has not been trusted"];

/// `permissionMode` of a run with `--dangerously-skip-permissions`.
const BYPASS_MODE: &str = "bypassPermissions";

/// What a trace cannot show for Claude.
pub const NOT_OBSERVABLE: [&str; 1] = [
    "whether the phase agent's `skills:` were preloaded — Claude reports the skill catalog, not what it injected",
];

#[derive(Debug, Default, PartialEq)]
pub struct Analysis {
    pub observed: Option<Observed>,
    pub unavailable: Option<String>,
    pub findings: Vec<Finding>,
}

/// Parse a captured run and compare it with what was configured.
pub fn analyze(stdout: &str, stderr: &str, expected: &Expected) -> Analysis {
    let events: Vec<Value> = stdout
        .lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .filter_map(|l| serde_json::from_str(l).ok())
        .filter(|v: &Value| v.get("type").is_some())
        .collect();
    if events.is_empty() {
        return Analysis {
            unavailable: Some(
                "no stream-json events on stdout; run claude with \
                 `--output-format stream-json --verbose` to trace it"
                    .into(),
            ),
            ..Analysis::default()
        };
    }

    let mut observed = Observed::default();
    let mut catalog = Catalog::default();
    for event in &events {
        read_event(event, expected, &mut observed, &mut catalog);
    }
    observed.warnings = stderr
        .lines()
        .map(str::trim)
        .filter(|l| WARNING_MARKERS.iter().any(|m| l.contains(m)))
        .map(str::to_string)
        .collect();

    let findings = compare(expected, &observed, &catalog);
    Analysis {
        observed: Some(observed),
        unavailable: None,
        findings,
    }
}

/// Catalogs from the init event: only needed to check expectations, too
/// large (every user agent and skill) to store in each record.
#[derive(Debug, Default)]
struct Catalog {
    seen_init: bool,
    agents: Vec<String>,
    skills: Vec<String>,
    mcp_servers: Vec<McpServer>,
}

fn read_event(event: &Value, expected: &Expected, observed: &mut Observed, catalog: &mut Catalog) {
    match (str_at(event, "type"), str_at(event, "subtype")) {
        (Some("system"), Some("init")) => read_init(event, expected, observed, catalog),
        (Some("system"), Some("permission_denied")) => {
            if let Some(tool) = str_at(event, "tool_name") {
                observed.permission_denials.push(tool.to_string());
            }
        }
        (Some("assistant"), _) => read_tool_uses(event, observed),
        (Some("result"), _) => observed.result = Some(read_result(event)),
        _ => {}
    }
}

fn read_init(event: &Value, expected: &Expected, observed: &mut Observed, catalog: &mut Catalog) {
    catalog.seen_init = true;
    observed.client_version = str_at(event, "claude_code_version").map(str::to_string);
    observed.model = str_at(event, "model").map(str::to_string);
    observed.session_id = str_at(event, "session_id").map(str::to_string);
    observed.cwd = str_at(event, "cwd").map(str::to_string);
    observed.permission_mode = str_at(event, "permissionMode").map(str::to_string);
    catalog.agents = strings_at(event, "agents");
    catalog.skills = strings_at(event, "skills");
    catalog.mcp_servers = event
        .get("mcp_servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(McpServer {
                name: str_at(m, "name")?.to_string(),
                status: str_at(m, "status").unwrap_or("unknown").to_string(),
            })
        })
        .collect();
    observed.mcp_servers = catalog
        .mcp_servers
        .iter()
        .filter(|m| expected.mcp_servers.contains(&m.name))
        .cloned()
        .collect();
}

fn read_tool_uses(event: &Value, observed: &mut Observed) {
    let blocks = event
        .pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| str_at(b, "type") == Some("tool_use"));
    for block in blocks {
        let Some(name) = str_at(block, "name") else {
            continue;
        };
        *observed.tool_calls.entry(name.to_string()).or_default() += 1;
        let input = block.get("input").unwrap_or(&Value::Null);
        match name {
            "Skill" => {
                if let Some(skill) = str_at(input, "skill") {
                    observed.skill_calls.push(skill.to_string());
                }
            }
            // The subagent tool was `Task` and is now also `Agent`.
            "Task" | "Agent" => {
                let kind = str_at(input, "subagent_type").unwrap_or("general-purpose");
                observed.subagents.push(kind.to_string());
            }
            _ => {}
        }
    }
}

/// Longest error message kept from a result.
const MESSAGE_LIMIT: usize = 500;

fn read_result(event: &Value) -> RunResult {
    let is_error = event
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Only an error's text is kept: a successful result is the agent's
    // whole answer, which the trace has no use for.
    let message = is_error
        .then(|| str_at(event, "result"))
        .flatten()
        .map(|m| m.chars().take(MESSAGE_LIMIT).collect());
    RunResult {
        subtype: str_at(event, "subtype").unwrap_or("unknown").to_string(),
        is_error,
        message,
        num_turns: event.get("num_turns").and_then(Value::as_u64),
        duration_ms: event.get("duration_ms").and_then(Value::as_u64),
        terminal_reason: str_at(event, "terminal_reason").map(str::to_string),
        cost_usd: event.get("total_cost_usd").and_then(Value::as_f64),
    }
}

fn compare(expected: &Expected, observed: &Observed, catalog: &Catalog) -> Vec<Finding> {
    use FindingKind::{AgentChoice, Infrastructure, Run};
    let mut findings = Vec::new();

    if !catalog.seen_init {
        findings.push(Finding::new(
            Infrastructure,
            "the client did not report its session (no init event); nothing it loaded can be confirmed",
        ));
    } else {
        if let Some(agent) = &expected.named_agent {
            if !catalog.agents.contains(agent) {
                findings.push(Finding::new(
                    Infrastructure,
                    format!("named agent `{agent}` is not in the client's agent catalog"),
                ));
            }
        }
        for skill in expected
            .skills
            .iter()
            .filter(|s| !catalog.skills.contains(s))
        {
            findings.push(Finding::new(
                Infrastructure,
                format!("skill `{skill}` is not in the client's skill catalog"),
            ));
        }
        for server in &expected.mcp_servers {
            match catalog.mcp_servers.iter().find(|m| &m.name == server) {
                None => findings.push(Finding::new(
                    Infrastructure,
                    format!("MCP server `{server}` is not configured for this project"),
                )),
                Some(m) if m.status != "connected" => findings.push(Finding::new(
                    Infrastructure,
                    format!("MCP server `{server}` is {}", m.status),
                )),
                Some(_) if !called_server(observed, server) => findings.push(Finding::new(
                    AgentChoice,
                    format!("MCP server `{server}` was connected but never called"),
                )),
                Some(_) => {}
            }
        }
        if let (Some(want), Some(got)) = (&expected.model, &observed.model) {
            if !model_matches(want, got) {
                findings.push(Finding::new(
                    Infrastructure,
                    format!("model `{want}` was configured but the run used `{got}`"),
                ));
            }
        }
    }

    // Dropped configuration matters only when permissions are enforced;
    // under `bypassPermissions` (headless runs) the warning stays in
    // `observed.warnings` but changes nothing the run could do.
    if observed.permission_mode.as_deref() != Some(BYPASS_MODE) {
        for warning in &observed.warnings {
            findings.push(Finding::new(Infrastructure, warning.clone()));
        }
    }
    if !observed.permission_denials.is_empty() {
        findings.push(Finding::new(
            Infrastructure,
            format!(
                "{} tool call(s) refused by the permission system: {}",
                observed.permission_denials.len(),
                count_list(&observed.permission_denials)
            ),
        ));
    }
    match &observed.result {
        None => findings.push(Finding::new(
            Run,
            "the run did not finish (no result event): interrupted, killed or crashed",
        )),
        Some(r) if r.is_error || r.subtype != "success" => findings.push(Finding::new(
            Run,
            format!(
                "the run ended with `{}`{}",
                r.subtype,
                r.terminal_reason
                    .as_deref()
                    .map(|t| format!(" ({t})"))
                    .unwrap_or_default()
            ),
        )),
        Some(_) => {}
    }
    findings
}

fn called_server(observed: &Observed, server: &str) -> bool {
    let prefix = format!("mcp__{server}__");
    observed.tool_calls.keys().any(|t| t.starts_with(&prefix))
}

/// `haiku` matches `claude-haiku-4-5-20251001`; a full id must match exactly.
fn model_matches(configured: &str, reported: &str) -> bool {
    configured == reported || reported.split('-').any(|part| part == configured)
}

/// `Bash×2, Edit` — occurrences in first-seen order.
fn count_list(items: &[String]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for item in items {
        match counts.iter_mut().find(|(name, _)| name == item) {
            Some((_, n)) => *n += 1,
            None => counts.push((item, 1)),
        }
    }
    counts
        .iter()
        .map(|(name, n)| {
            if *n > 1 {
                format!("{name}×{n}")
            } else {
                (*name).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn strings_at(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Claude Code 2.1.278 run of `code-agent` (haiku), trimmed and
    /// with paths replaced: ToolSearch, a CodeGraph search, two Bash calls
    /// refused, reads, an edit and a test run.
    const STREAM: &str = include_str!("../../tests/fixtures/claude/stream_code_agent.jsonl");

    fn expected() -> Expected {
        Expected {
            named_agent: Some("code-agent".into()),
            model: Some("haiku".into()),
            skills: vec![
                "zforge-write-tests-first".into(),
                "zforge-implement-minimal-patch".into(),
            ],
            mcp_servers: vec!["codegraph".into()],
            ..Default::default()
        }
    }

    fn messages(a: &Analysis) -> Vec<&str> {
        a.findings.iter().map(|f| f.message.as_str()).collect()
    }

    fn kinds(a: &Analysis) -> Vec<FindingKind> {
        a.findings.iter().map(|f| f.kind).collect()
    }

    #[test]
    fn reads_session_tools_and_result_from_a_real_stream() {
        let a = analyze(STREAM, "", &expected());
        let o = a.observed.expect("observed");
        assert_eq!(o.client_version.as_deref(), Some("2.1.278"));
        assert_eq!(o.model.as_deref(), Some("claude-haiku-4-5-20251001"));
        assert_eq!(
            o.mcp_servers,
            vec![McpServer {
                name: "codegraph".into(),
                status: "connected".into()
            }]
        );
        assert_eq!(
            o.tool_calls.get("mcp__codegraph__codegraph_search"),
            Some(&1)
        );
        assert_eq!(o.tool_calls.get("Read"), Some(&2));
        assert_eq!(o.tool_calls.get("Edit"), Some(&1));
        assert_eq!(o.tool_calls.get("Bash"), Some(&3));
        assert_eq!(o.permission_denials, vec!["Bash", "Bash"]);
        let r = o.result.expect("result");
        assert_eq!(r.subtype, "success");
        assert_eq!(r.num_turns, Some(9));
    }

    #[test]
    fn a_healthy_setup_reports_only_the_refused_calls() {
        let a = analyze(STREAM, "", &expected());
        assert_eq!(kinds(&a), vec![FindingKind::Infrastructure]);
        assert!(
            messages(&a)[0].contains("2 tool call(s) refused"),
            "{:?}",
            messages(&a)
        );
        assert!(messages(&a)[0].contains("Bash×2"));
    }

    /// Seen for real: in an untrusted workspace `claude -p` drops the
    /// project's allowlist — the cause of the refusals above.
    #[test]
    fn a_dropped_allowlist_on_stderr_is_a_finding() {
        let stderr = "Ignoring 17 permissions.allow entries from .claude/settings.json: \
                      this workspace has not been trusted.\n";
        let a = analyze(STREAM, stderr, &expected());
        let o = a.observed.as_ref().unwrap();
        assert_eq!(o.warnings.len(), 1);
        assert!(messages(&a)
            .iter()
            .any(|m| m.starts_with("Ignoring 17 permissions.allow")));
    }

    /// Seen in the benchmark: headless runs bypass permissions, so the
    /// dropped allowlist changes nothing — keep the warning, no finding.
    #[test]
    fn a_dropped_allowlist_under_bypass_is_only_a_warning() {
        let stream = STREAM.replacen(
            r#""permissionMode": "default""#,
            r#""permissionMode": "bypassPermissions""#,
            1,
        );
        assert_ne!(stream, STREAM, "fixture layout changed");
        let stderr = "Ignoring 17 permissions.allow entries from .claude/settings.json: \
                      this workspace has not been trusted.\n";
        let a = analyze(&stream, stderr, &expected());
        assert_eq!(a.observed.as_ref().unwrap().warnings.len(), 1);
        assert!(
            !messages(&a).iter().any(|m| m.starts_with("Ignoring")),
            "{:?}",
            messages(&a)
        );
    }

    #[test]
    fn a_server_that_is_not_connected_is_a_finding() {
        let stream = STREAM.replacen(
            r#"{"name": "codegraph", "status": "connected""#,
            r#"{"name": "codegraph", "status": "failed""#,
            1,
        );
        assert_ne!(stream, STREAM, "fixture layout changed");
        let a = analyze(&stream, "", &expected());
        assert!(messages(&a).contains(&"MCP server `codegraph` is failed"));
    }

    #[test]
    fn an_unconfigured_server_is_a_finding() {
        let mut e = expected();
        e.mcp_servers = vec!["other".into()];
        let a = analyze(STREAM, "", &e);
        assert!(messages(&a).contains(&"MCP server `other` is not configured for this project"));
    }

    #[test]
    fn a_connected_server_never_called_is_the_agents_choice() {
        let stream: String = STREAM
            .lines()
            .filter(|l| !l.contains(r#""name": "mcp__codegraph__codegraph_search""#))
            .map(|l| format!("{l}\n"))
            .collect();
        let a = analyze(&stream, "", &expected());
        assert!(a.findings.contains(&Finding::new(
            FindingKind::AgentChoice,
            "MCP server `codegraph` was connected but never called"
        )));
    }

    #[test]
    fn missing_agent_skill_and_wrong_model_are_findings() {
        let e = Expected {
            named_agent: Some("deploy-agent".into()),
            model: Some("opus".into()),
            skills: vec!["zforge-not-installed".into()],
            mcp_servers: vec![],
            ..Default::default()
        };
        let m = analyze(STREAM, "", &e)
            .findings
            .into_iter()
            .map(|f| f.message)
            .collect::<Vec<_>>();
        assert!(
            m.contains(&"named agent `deploy-agent` is not in the client's agent catalog".into())
        );
        assert!(
            m.contains(&"skill `zforge-not-installed` is not in the client's skill catalog".into())
        );
        assert!(m.contains(
            &"model `opus` was configured but the run used `claude-haiku-4-5-20251001`".into()
        ));
    }

    /// Killed or timed out mid-run: no `result`, which must never read as
    /// a finished run.
    #[test]
    fn a_stream_without_result_did_not_finish() {
        let cut: String = STREAM
            .lines()
            .filter(|l| !l.contains(r#""type": "result""#))
            .map(|l| format!("{l}\n"))
            .collect();
        let a = analyze(&cut, "", &expected());
        assert!(a.observed.as_ref().unwrap().result.is_none());
        assert!(a.findings.iter().any(|f| f.kind == FindingKind::Run));
    }

    #[test]
    fn plain_text_output_is_unavailable_not_empty() {
        let a = analyze("All done.\n", "", &expected());
        assert!(a.observed.is_none());
        assert!(a.unavailable.unwrap().contains("stream-json"));
    }

    #[test]
    fn skills_and_subagents_are_recorded() {
        let extra = r#"{"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Skill", "input": {"skill": "zforge-debug"}}, {"type": "tool_use", "id": "t2", "name": "Task", "input": {"subagent_type": "code-explorer", "prompt": "x"}}]}}"#;
        let stream = format!("{STREAM}{extra}\n");
        let o = analyze(&stream, "", &expected()).observed.unwrap();
        assert_eq!(o.skill_calls, vec!["zforge-debug"]);
        assert_eq!(o.subagents, vec!["code-explorer"]);
    }

    #[test]
    fn model_alias_matching() {
        assert!(model_matches("haiku", "claude-haiku-4-5-20251001"));
        assert!(model_matches("claude-opus-5", "claude-opus-5"));
        assert!(!model_matches("opus", "claude-haiku-4-5-20251001"));
    }
}
