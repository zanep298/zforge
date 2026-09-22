//! One record per agent invocation, appended to `<task>/trace.jsonl`.
//!
//! `expected` is what zforge configured for the run; `observed` is what the
//! client reported while running it. They are kept apart on purpose: a
//! configured skill or MCP server is not evidence that it was available, and
//! a catalog entry is not evidence that it was used.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhaseTrace {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub phase: String,
    /// 1-based spawn attempt within this phase run (fallback swaps add one).
    pub attempt: u32,
    /// Registry name of the runner (`claude`, `codex`, …).
    pub runner: String,
    /// Command and arguments, without the prompt (it goes on stdin).
    pub command: Vec<String>,
    pub expected: Expected,
    /// `None` when the runner exposes no trace or the output was not
    /// captured; `unavailable` says why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<Observed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
    /// Problems found by comparing `expected` with `observed`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    /// What this runner cannot show, so nobody reads its absence as proof.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_observable: Vec<String>,
}

/// What zforge set up for the run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Expected {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Skills the phase agent preloads (`skills:` frontmatter).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// MCP servers the phase prompts rely on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
}

/// What the client reported during the run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Observed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Status of the servers in `expected.mcp_servers` (others omitted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
    /// Tool name → number of calls, subagents included.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_calls: BTreeMap<String, u32>,
    /// Skills invoked through the `Skill` tool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_calls: Vec<String>,
    /// `subagent_type` of every subagent started.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subagents: Vec<String>,
    /// Tools whose calls were refused by the permission system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_denials: Vec<String>,
    /// Client warnings on stderr that change what the run could do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RunResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RunResult {
    pub subtype: String,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_turns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// Who a finding points at, so a reviewer can tell a broken setup from a
/// poor choice by the agent from a problem in the product.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// Client, configuration, server or permission problem.
    Infrastructure,
    /// Something available that the agent did not use.
    AgentChoice,
    /// The run itself ended badly.
    Run,
}

impl FindingKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Infrastructure => "infrastructure",
            Self::AgentChoice => "agent choice",
            Self::Run => "run",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    pub message: String,
}

impl Finding {
    pub fn new(kind: FindingKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
