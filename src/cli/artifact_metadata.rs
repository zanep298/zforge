use crate::config::Config;
use crate::fs::{reader, tokens, writer};
use crate::state::TaskState;
use anyhow::Result;
use std::path::Path;

/// Write common LLM-produced artifact metadata and return the token estimate.
pub fn set_llm_metadata(
    config: &Config,
    ts: &TaskState,
    phase: &str,
    path: &Path,
    content: &str,
) -> Result<usize> {
    let token_count = tokens::estimate(content);
    writer::set_frontmatter(
        path,
        "tokens",
        serde_yaml::Value::Number(token_count.into()),
    )?;

    let agent = ts
        .effective_agent()
        .map(str::to_string)
        .unwrap_or_else(|| "(legacy-dispatch)".to_string());
    writer::set_frontmatter(path, "agent", serde_yaml::Value::String(agent.clone()))?;

    let model = reader::agent_model_for_dispatch(&config.agents_dir(), &agent, phase)
        .unwrap_or_else(|| "unknown".to_string());
    writer::set_frontmatter(path, "model", serde_yaml::Value::String(model))?;

    Ok(token_count)
}
