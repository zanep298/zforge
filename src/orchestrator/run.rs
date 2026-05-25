use crate::config::{load_models_from_root, ModelsConfig};
use crate::cost::{
    log as cost_log,
    prices::cost_usd,
    schema::{estimate_tokens, CostEntry},
};
use crate::orchestrator::{
    fallback::CompiledPolicy,
    headless_args::headless_args_for_agent,
    history::{fallback_count, record_fallback},
    model_args::model_args_for_agent,
    spawn::{spawn_agent, SpawnOutcome},
};
use crate::registry;
use crate::registry::schema::AgentSpec;
use crate::state::TaskState;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

/// Env var set by `job::worker::run` at the entry of a background worker.
/// Presence (any value) means "no human present — inject bypass flags
/// onto the agent so it doesn't block on interactive prompts".
const HEADLESS_ENV: &str = "ZFORGE_HEADLESS";

fn is_headless() -> bool {
    std::env::var(HEADLESS_ENV)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Main orchestration loop. Resolves `effective_agent`, spawns it, decides
/// fallback per `CompiledPolicy`, persists `active_agent` + history between
/// attempts. `assigned_agent` is never mutated.
///
/// Returns `Ok(())` on the first successful spawn (exit 0). Non-retryable
/// failures propagate immediately. Retryable failures consume the policy's
/// retry budget; exhaustion is a terminal `Err`.
pub fn run_phase(
    task_id: &str,
    phase: &str,
    project_root: &Path,
    prompt: &str,
) -> Result<()> {
    let registry = registry::io::load()?;
    let policy = CompiledPolicy::compile(&registry.fallback_policy)?;
    let spawn_timeout_secs = registry.fallback_policy.spawn_timeout_secs;
    // models.yaml is optional — None → no model injection, args stay as
    // registered. Loaded once outside the loop because file IO between
    // retry attempts is wasted work.
    let models = load_models_from_root(project_root);

    let state_path = project_root
        .join(".zforge/tasks")
        .join(task_id)
        .join(".state.yaml");
    let mut state = load_state(&state_path)?;

    loop {
        let agent_name = state
            .effective_agent()
            .ok_or_else(|| anyhow!("task {task_id} has no agent assigned"))?
            .to_string();

        let base_spec = registry
            .resolved_agent(&agent_name, project_root)
            .ok_or_else(|| {
                anyhow!(
                    "agent {agent_name:?} not in registry agents{{}}. Add it to ~/.zforge/registry.yaml."
                )
            })?;

        let spec = with_model_args(&base_spec, &agent_name, phase, models.as_ref());
        let spec = with_headless_args(spec, &agent_name, is_headless());

        let resolved_model = models
            .as_ref()
            .and_then(|m| m.for_assistant(&agent_name, phase))
            .map(|s| s.to_string());

        let outcome = spawn_agent(&spec, prompt, spawn_timeout_secs)?;

        // Telemetry: append a CostEntry per spawn. Best-effort — a log
        // failure must not block orchestration.
        let _ = record_cost(
            project_root,
            task_id,
            phase,
            &agent_name,
            resolved_model.as_deref(),
            prompt,
            &outcome,
        );

        // Real binaries (claude, codex) print failure messages to stdout
        // — not stderr. Codex also exits 0 even on API errors. Concatenate
        // both streams so retryable-pattern matching sees the whole picture.
        // PR9 contract verification documented in docs/agent-contracts.md.
        let combined_output = format!("{}\n{}", outcome.stderr, outcome.stdout);
        let policy_reason = policy.should_fallback(outcome.exit_code, &combined_output);

        // Three outcomes:
        //   (exit 0, no retryable match)  → clean success
        //   (exit 0, retryable match)     → API error visible in output even
        //                                    though exit was zero — swap agent
        //   (exit != 0, retryable match)  → normal fallback
        //   (exit != 0, no retryable match) → non-retryable failure, propagate
        let reason = match (outcome.exit_code, policy_reason) {
            (0, None) => return Ok(()),
            (_, Some(r)) => r,
            (code, None) => {
                anyhow::bail!(
                    "agent {agent_name} failed in phase {phase} (exit={code})\n\
                     stderr:\n{stderr}\nstdout:\n{stdout}",
                    stderr = outcome.stderr.trim_end(),
                    stdout = outcome.stdout.trim_end(),
                );
            }
        };

        if fallback_count(&state) as u32 >= policy.max_retries {
            anyhow::bail!(
                "fallback budget exhausted ({} retries) for task {task_id} in phase {phase}; last error: {}",
                policy.max_retries,
                reason.as_log_str(),
            );
        }

        record_fallback(&mut state, &reason, phase).context("record fallback")?;
        save_state_atomic(&state_path, &state)?;

        if policy.cooldown_ms > 0 {
            sleep(Duration::from_millis(policy.cooldown_ms));
        }
    }
}

/// Return a copy of `base` with model-selection args appended when
/// `models.yaml` defines a model for `(agent_name, phase)` AND the agent's
/// CLI convention is known. Otherwise returns `base` unchanged.
///
/// The model args go AFTER the user-registered `args` so user-supplied flags
/// stay earlier in argv (claude/opencode parse left-to-right; later flags
/// win on duplicates — desirable so models.yaml overrides any model arg the
/// user accidentally hardcoded into the registry).
fn with_model_args(
    base: &AgentSpec,
    agent_name: &str,
    phase: &str,
    models: Option<&ModelsConfig>,
) -> AgentSpec {
    let Some(models) = models else {
        return base.clone();
    };
    let Some(model) = models.for_assistant(agent_name, phase) else {
        return base.clone();
    };
    let extra = model_args_for_agent(agent_name, model);
    if extra.is_empty() {
        return base.clone();
    }
    let mut spec = base.clone();
    spec.args.extend(extra);
    spec
}

/// Prepend per-agent permission-bypass flags when running headless.
/// Foreground invocations leave the spec untouched so users can answer
/// prompts. Unknown agents get no injection — they configure bypass via
/// registry `AgentSpec.args` themselves.
///
/// We PREPEND (not append) because some agents (codex) require global
/// top-level flags before the subcommand: `codex -a never exec <prompt>`,
/// not `codex exec -a never <prompt>` (latter is parsed as exec arg and
/// errors out). For agents where order doesn't matter (claude), prepending
/// is equivalent.
fn with_headless_args(mut spec: AgentSpec, agent_name: &str, headless: bool) -> AgentSpec {
    if !headless {
        return spec;
    }
    let extra = headless_args_for_agent(agent_name);
    if extra.is_empty() {
        return spec;
    }
    let mut combined = extra;
    combined.append(&mut spec.args);
    spec.args = combined;
    spec
}

/// Build + append one CostEntry. Codex `tokens used N` line in stdout (if
/// present) becomes `reported_total_tokens`; otherwise we estimate from
/// char counts. Failure is logged-and-swallowed — telemetry must never
/// block the orchestrator's hot path.
fn record_cost(
    project_root: &Path,
    task_id: &str,
    phase: &str,
    agent: &str,
    model: Option<&str>,
    prompt: &str,
    outcome: &SpawnOutcome,
) -> Result<()> {
    let prompt_chars = prompt.chars().count();
    let stdout_chars = outcome.stdout.chars().count();
    let stderr_chars = outcome.stderr.chars().count();
    let est_input_tokens = estimate_tokens(prompt_chars);
    let est_output_tokens = estimate_tokens(stdout_chars + stderr_chars);

    // Codex emits `tokens used N` in its non-interactive output. Parse if
    // present so reports show real totals for codex; estimates stay for
    // claude / others.
    let reported_total_tokens = parse_codex_tokens(&outcome.stdout, &outcome.stderr);

    let est_cost_usd = cost_usd(agent, model, est_input_tokens, est_output_tokens);

    let entry = CostEntry {
        timestamp: Utc::now(),
        task_id: task_id.to_string(),
        phase: phase.to_string(),
        agent: agent.to_string(),
        model: model.map(str::to_string),
        prompt_chars,
        stdout_chars,
        stderr_chars,
        duration_ms: outcome.duration_ms,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        est_input_tokens,
        est_output_tokens,
        reported_total_tokens,
        est_cost_usd,
    };
    cost_log::record(project_root, &entry)
}

/// Codex prints a line like `tokens used 9963` after the conversation.
/// Returns the total when found in either stream. Regex-free for hot path:
/// substring scan + parse.
fn parse_codex_tokens(stdout: &str, stderr: &str) -> Option<u64> {
    for stream in [stdout, stderr] {
        for line in stream.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("tokens used ") {
                if let Ok(n) = rest.trim().parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn load_state(path: &Path) -> Result<TaskState> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path:?}"))?;
    serde_yaml::from_str(&raw).with_context(|| format!("parse {path:?}"))
}

fn save_state_atomic(path: &Path, state: &TaskState) -> Result<()> {
    let tmp = path.with_extension("yaml.tmp");
    let yaml = serde_yaml::to_string(state)?;
    std::fs::write(&tmp, yaml)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PhaseModels;

    fn base_spec() -> AgentSpec {
        AgentSpec {
            command: "claude".into(),
            args: vec!["-p".into()],
        }
    }

    fn models_with_claude_plan(model: &str) -> ModelsConfig {
        let mut m = ModelsConfig::default();
        m.agents.insert(
            "claude".into(),
            PhaseModels {
                plan: Some(model.to_string()),
                ..PhaseModels::default()
            },
        );
        m
    }

    #[test]
    fn no_models_yaml_returns_base_spec_unchanged() {
        let result = with_model_args(&base_spec(), "claude", "plan", None);
        assert_eq!(result.args, vec!["-p"]);
    }

    #[test]
    fn matching_assistant_and_phase_appends_model_args() {
        let models = models_with_claude_plan("opus");
        let result = with_model_args(&base_spec(), "claude", "plan", Some(&models));
        assert_eq!(result.args, vec!["-p", "--model", "opus"]);
    }

    #[test]
    fn unmatched_phase_leaves_args_alone() {
        let models = models_with_claude_plan("opus");
        // models.yaml only configured `plan`; asking for `code` finds nothing.
        let result = with_model_args(&base_spec(), "claude", "code", Some(&models));
        assert_eq!(result.args, vec!["-p"]);
    }

    #[test]
    fn unmatched_assistant_leaves_args_alone() {
        let models = models_with_claude_plan("opus");
        // models.yaml has no `mystery` entry → no injection.
        let result = with_model_args(&base_spec(), "mystery", "plan", Some(&models));
        assert_eq!(result.args, vec!["-p"]);
    }

    #[test]
    fn codex_phase_match_still_skips_injection() {
        // Codex uses profile-based config — `--model X` would conflict.
        let mut models = ModelsConfig::default();
        models.agents.insert(
            "codex".into(),
            PhaseModels {
                code: Some("zforge_code".into()),
                ..PhaseModels::default()
            },
        );
        let spec = AgentSpec {
            command: "codex".into(),
            args: vec!["--profile".into(), "zforge_code".into()],
        };
        let result = with_model_args(&spec, "codex", "code", Some(&models));
        // Args unchanged; user-set profile arg preserved as-is.
        assert_eq!(result.args, vec!["--profile", "zforge_code"]);
    }

    #[test]
    fn headless_off_leaves_spec_unchanged() {
        let base = base_spec();
        let result = with_headless_args(base.clone(), "claude", false);
        assert_eq!(result.args, base.args);
    }

    #[test]
    fn headless_on_appends_claude_bypass_flag() {
        let base = base_spec();
        let result = with_headless_args(base, "claude", true);
        assert!(result
            .args
            .iter()
            .any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn headless_on_appends_codex_no_approval_flags() {
        let spec = AgentSpec {
            command: "codex".into(),
            args: vec!["exec".into()],
        };
        let result = with_headless_args(spec, "codex", true);
        assert!(result.args.iter().any(|a| a == "-a"));
        assert!(result.args.iter().any(|a| a == "never"));
    }

    #[test]
    fn headless_on_unknown_agent_leaves_spec_unchanged() {
        let base = AgentSpec {
            command: "mystery".into(),
            args: vec!["--frob".into()],
        };
        let result = with_headless_args(base.clone(), "mystery", true);
        assert_eq!(result.args, base.args);
    }

    #[test]
    fn model_args_appended_after_user_args() {
        let models = models_with_claude_plan("opus");
        let spec = AgentSpec {
            command: "claude".into(),
            args: vec!["-p".into(), "--verbose".into()],
        };
        let result = with_model_args(&spec, "claude", "plan", Some(&models));
        // User flags stay first; model args last so duplicates resolve to
        // models.yaml-supplied value under left-to-right CLI parsing.
        assert_eq!(result.args, vec!["-p", "--verbose", "--model", "opus"]);
    }
}
