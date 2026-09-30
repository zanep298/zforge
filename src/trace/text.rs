//! Plain-text view of one agent call's trace, for `zforge run status`.

use super::{FindingKind, PhaseTrace};
use std::fmt::Write as _;

/// One phase attempt as text, for other commands' reports.
pub fn render_phase_text(p: &PhaseTrace) -> String {
    let mut out = String::new();
    render_phase(&mut out, p);
    out
}

fn render_phase(out: &mut String, p: &PhaseTrace) {
    let _ = writeln!(out, "{} · attempt {} · {}", p.phase, p.attempt, p.runner);
    match &p.observed {
        None => {
            let why = p.unavailable.as_deref().unwrap_or("no trace");
            let _ = writeln!(out, "  no trace: {why}");
        }
        Some(o) => {
            let version = o.client_version.as_deref().unwrap_or("?");
            let model = o.model.as_deref().unwrap_or("?");
            let expected_model = p.expected.model.as_deref().unwrap_or("default");
            let _ = writeln!(
                out,
                "  client {version}, model {model} (configured {expected_model})"
            );
            if let Some(agent) = &p.expected.named_agent {
                let _ = writeln!(out, "  agent {agent}");
            }
            if !p.expected.skills.is_empty() {
                let _ = writeln!(out, "  preload skills: {}", p.expected.skills.join(", "));
            }
            if !p.expected.knowledge.is_empty() {
                let files: Vec<String> = p
                    .expected
                    .knowledge
                    .iter()
                    .map(|k| format!("{} rev {}", k.file, k.revision))
                    .collect();
                let _ = writeln!(out, "  knowledge: {}", files.join(", "));
                if !p.expected.knowledge_items.is_empty() {
                    let _ = writeln!(
                        out,
                        "  knowledge items: {}",
                        p.expected.knowledge_items.join(", ")
                    );
                }
            }
            if !o.mcp_servers.is_empty() {
                let servers: Vec<String> = o
                    .mcp_servers
                    .iter()
                    .map(|m| format!("{} {}", m.name, m.status))
                    .collect();
                let _ = writeln!(out, "  mcp: {}", servers.join(", "));
            }
            let tools: Vec<String> = o
                .tool_calls
                .iter()
                .map(|(name, n)| format!("{name}×{n}"))
                .collect();
            let tools = if tools.is_empty() {
                "none".to_string()
            } else {
                tools.join(" ")
            };
            let _ = writeln!(out, "  tools: {tools}");
            if !o.skill_calls.is_empty() {
                let _ = writeln!(out, "  skills called: {}", o.skill_calls.join(", "));
            }
            if !o.subagents.is_empty() {
                let _ = writeln!(out, "  subagents: {}", o.subagents.join(", "));
            }
            match &o.result {
                Some(res) => {
                    let _ = writeln!(
                        out,
                        "  result: {}, {} turn(s), {}, {}",
                        res.subtype,
                        res.num_turns.map_or("?".into(), |n| n.to_string()),
                        res.duration_ms
                            .map_or("?".into(), |ms| format!("{:.1}s", ms as f64 / 1000.0)),
                        res.cost_usd.map_or("cost ?".into(), |c| format!("${c:.2}")),
                    );
                }
                None => {
                    let _ = writeln!(out, "  result: none");
                }
            }
        }
    }
    for f in &p.findings {
        let mark = if f.kind == FindingKind::AgentChoice {
            "?"
        } else {
            "!"
        };
        let _ = writeln!(out, "  {mark} {}: {}", f.kind.label(), f.message);
    }
    for n in &p.not_observable {
        let _ = writeln!(out, "  · not observable: {n}");
    }
    out.push('\n');
}
