use crate::config::load_models_from_root;
use crate::cost::{
    log as cost_log,
    prices::cost_usd,
    schema::{estimate_tokens, CostEntry},
    usage::{parse_claude_usage, parse_codex_tokens, UsageReport},
};
use crate::fs::reader;
use crate::orchestrator::{
    fallback::CompiledPolicy,
    headless_args::headless_args_for_agent,
    history::{fallback_count, record_fallback},
    model_args::{model_args_for_agent, profile_args_for_agent},
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
pub fn run_phase(task_id: &str, phase: &str, project_root: &Path, prompt: &str) -> Result<()> {
    let registry = registry::io::load()?;
    let policy = CompiledPolicy::compile(&registry.fallback_policy)?;
    let spawn_timeout_secs = registry.fallback_policy.spawn_timeout_secs;
    // models.yaml is optional — None → no model injection, args stay as
    // registered. Loaded once outside the loop because file IO between
    // retry attempts is wasted work.
    let models = load_models_from_root(project_root);
    let agents_dir = crate::config::load_from(&project_root.join(".zforge").join("config.yaml"))
        .map(|c| c.agents_dir())
        .unwrap_or_else(|_| project_root.join(".zforge").join("agents"));

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

        let configured_model = reader::agent_model_for_phase_with_models(
            &agents_dir,
            &agent_name,
            phase,
            models.as_ref(),
        );

        let spec = with_profile_args(&base_spec, &agent_name, phase);
        let spec = with_model_args(&spec, &agent_name, configured_model.as_deref());
        let spec = with_headless_args(spec, &agent_name, is_headless());

        // Model resolution precedence for telemetry:
        //   1. `--model X` baked into the (possibly model-args-augmented)
        //      AgentSpec.args — covers users who hardcode model in registry
        //   2. configured model from models.yaml or agent frontmatter
        //   3. None — price table lookup will return 0 for unknown agent/model
        let resolved_model = sniff_model_from_args(&spec.args).or(configured_model);

        let outcome = spawn_agent(&spec, prompt, spawn_timeout_secs)?;

        // Telemetry: append a CostEntry per spawn. Best-effort — a log
        // failure must not block orchestration, but DO emit a warning so
        // disk-full / corrupt-path bugs are visible during debugging.
        if let Err(e) = record_cost(
            project_root,
            task_id,
            phase,
            &agent_name,
            resolved_model.as_deref(),
            prompt,
            &outcome,
        ) {
            eprintln!("warning: cost log append failed: {e}");
        }

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
fn with_model_args(base: &AgentSpec, agent_name: &str, model: Option<&str>) -> AgentSpec {
    let Some(model) = model else {
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

/// Prepend profile-selection args. Codex's `--profile zforge_<phase>` must
/// come BEFORE the `exec` subcommand or codex parses it as an arg to exec
/// and errors. Other agents return an empty profile arg list so this is a
/// no-op.
///
/// Prepending is safe relative to user-registered args: even if the user
/// hardcoded `--profile something_else`, the orchestrator's value still
/// lands first; codex's CLI takes the LAST occurrence so user override
/// continues to win — matching `with_model_args`'s precedence story.
fn with_profile_args(base: &AgentSpec, agent_name: &str, phase: &str) -> AgentSpec {
    let extra = profile_args_for_agent(agent_name, phase);
    if extra.is_empty() {
        return base.clone();
    }
    let mut spec = base.clone();
    let mut combined = extra;
    combined.append(&mut spec.args);
    spec.args = combined;
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

/// Build + append one CostEntry.
///
/// Token accounting precedence:
///   1. If the agent emitted a parseable usage block (claude JSON `usage`),
///      use those real counts AND attribute cache_read / cache_creation
///      separately so prompt-caching discounts flow into cost.
///   2. If codex emitted `tokens used N` (total, no input/output split),
///      record as `reported_total_tokens`; reports use it for total-token
///      columns. Cost still uses estimates because we can't split the total,
///      but codex price is $0/token on ChatGPT Plus so the inaccuracy is moot.
///   3. Otherwise estimate from byte length. stdout only — stderr is
///      banner / debug / error output, not LLM tokens. Counting it as
///      output charges the user for every verbose error trace.
///
/// `prompt_chars` / `stdout_chars` / `stderr_chars` stay in BYTES (not
/// char count) so non-ASCII prompts don't underreport. The field names
/// keep "chars" for backward compat with already-written log lines.
fn record_cost(
    project_root: &Path,
    task_id: &str,
    phase: &str,
    agent: &str,
    model: Option<&str>,
    prompt: &str,
    outcome: &SpawnOutcome,
) -> Result<()> {
    let prompt_bytes = prompt.len();
    let stdout_bytes = outcome.stdout.len();
    let stderr_bytes = outcome.stderr.len();

    let claude_usage = if agent == "claude" {
        parse_claude_usage(&outcome.stdout)
    } else {
        None
    };
    let reported_total_tokens = parse_codex_tokens(&outcome.stdout, &outcome.stderr);

    let (est_input_tokens, est_output_tokens, mut tokens_source, cache_read, cache_create) =
        derive_token_counts(prompt_bytes, stdout_bytes, claude_usage.as_ref());
    if claude_usage.is_none() && reported_total_tokens.is_some() {
        tokens_source = "reported-total";
    }

    let est_cost_usd = cost_usd(
        agent,
        model,
        est_input_tokens,
        est_output_tokens,
        cache_read.unwrap_or(0),
        cache_create.unwrap_or(0),
    );

    let entry = CostEntry {
        timestamp: Utc::now(),
        task_id: task_id.to_string(),
        phase: phase.to_string(),
        agent: agent.to_string(),
        model: model.map(str::to_string),
        prompt_chars: prompt_bytes,
        stdout_chars: stdout_bytes,
        stderr_chars: stderr_bytes,
        duration_ms: outcome.duration_ms,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        est_input_tokens,
        est_output_tokens,
        cache_read_input_tokens: cache_read,
        cache_creation_input_tokens: cache_create,
        reported_total_tokens,
        tokens_source: tokens_source.to_string(),
        est_cost_usd,
    };
    cost_log::record(project_root, &entry)
}

/// Pick the best available token counts. Reported claude `usage` block
/// wins per-field; missing fields fall back to estimates so partial
/// reports still produce non-zero rows.
fn derive_token_counts(
    prompt_bytes: usize,
    stdout_bytes: usize,
    reported: Option<&UsageReport>,
) -> (usize, usize, &'static str, Option<u64>, Option<u64>) {
    let input_estimate = estimate_tokens(prompt_bytes);
    let output_estimate = estimate_tokens(stdout_bytes);
    match reported {
        Some(u) => {
            let input = u.input_tokens.map(|n| n as usize).unwrap_or(input_estimate);
            let output = u
                .output_tokens
                .map(|n| n as usize)
                .unwrap_or(output_estimate);
            let source = if u.input_tokens.is_some() || u.output_tokens.is_some() {
                "reported"
            } else {
                "estimated"
            };
            (
                input,
                output,
                source,
                u.cache_read_input_tokens,
                u.cache_creation_input_tokens,
            )
        }
        None => (input_estimate, output_estimate, "estimated", None, None),
    }
}

/// Scan `--model X`, `--model=X`, `-m X`, and `-m=X` out of an AgentSpec's args. Last
/// occurrence wins to match left-to-right CLI parsing — `with_model_args`
/// appends models.yaml's value LAST so it overrides any earlier hardcoded
/// flag. Returns None when no model flag is present.
fn sniff_model_from_args(args: &[String]) -> Option<String> {
    let mut found: Option<String> = None;
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--model" || arg == "-m" {
            if let Some(val) = iter.next() {
                found = Some(val.clone());
            }
        } else if let Some(rest) = arg.strip_prefix("--model=") {
            found = Some(rest.to_string());
        } else if let Some(rest) = arg.strip_prefix("-m=") {
            found = Some(rest.to_string());
        }
    }
    found
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

    fn base_spec() -> AgentSpec {
        AgentSpec {
            command: "claude".into(),
            args: vec!["-p".into()],
        }
    }

    #[test]
    fn no_configured_model_returns_base_spec_unchanged() {
        let result = with_model_args(&base_spec(), "claude", None);
        assert_eq!(result.args, vec!["-p"]);
    }

    #[test]
    fn matching_assistant_appends_model_args() {
        let result = with_model_args(&base_spec(), "claude", Some("opus"));
        assert_eq!(result.args, vec!["-p", "--model", "opus"]);
    }

    #[test]
    fn unmatched_assistant_leaves_args_alone() {
        let result = with_model_args(&base_spec(), "mystery", Some("opus"));
        assert_eq!(result.args, vec!["-p"]);
    }

    #[test]
    fn codex_phase_match_still_skips_injection() {
        // Codex uses profile-based config — `--model X` would conflict.
        let spec = AgentSpec {
            command: "codex".into(),
            args: vec!["--profile".into(), "zforge_code".into()],
        };
        let result = with_model_args(&spec, "codex", Some("gpt-5-codex"));
        // Args unchanged; user-set profile arg preserved as-is.
        assert_eq!(result.args, vec!["--profile", "zforge_code"]);
    }

    #[test]
    fn codex_profile_args_prepend_before_existing() {
        let spec = AgentSpec {
            command: "codex".into(),
            args: vec!["exec".into()],
        };
        let result = with_profile_args(&spec, "codex", "plan");
        assert_eq!(
            result.args,
            vec!["--profile", "zforge_plan", "exec"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn non_codex_agent_profile_args_no_op() {
        let spec = AgentSpec {
            command: "claude".into(),
            args: vec!["-p".into()],
        };
        let result = with_profile_args(&spec, "claude", "plan");
        assert_eq!(result.args, vec!["-p"]);
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
    fn sniff_model_finds_spaced_form() {
        let args = vec!["-p".into(), "--model".into(), "haiku".into()];
        assert_eq!(sniff_model_from_args(&args).as_deref(), Some("haiku"));
    }

    #[test]
    fn sniff_model_finds_equals_form() {
        let args = vec!["--model=sonnet".into(), "-p".into()];
        assert_eq!(sniff_model_from_args(&args).as_deref(), Some("sonnet"));
    }

    #[test]
    fn sniff_model_finds_short_form() {
        let args = vec!["exec".into(), "-m".into(), "gpt-5-codex".into()];
        assert_eq!(sniff_model_from_args(&args).as_deref(), Some("gpt-5-codex"));
    }

    #[test]
    fn sniff_model_finds_short_equals_form() {
        let args = vec!["exec".into(), "-m=gpt-5".into()];
        assert_eq!(sniff_model_from_args(&args).as_deref(), Some("gpt-5"));
    }

    #[test]
    fn sniff_model_last_occurrence_wins() {
        let args = vec![
            "--model".into(),
            "haiku".into(),
            "--model".into(),
            "opus".into(),
        ];
        assert_eq!(sniff_model_from_args(&args).as_deref(), Some("opus"));
    }

    #[test]
    fn sniff_model_returns_none_when_absent() {
        let args = vec!["-p".into(), "--verbose".into()];
        assert_eq!(sniff_model_from_args(&args), None);
    }

    #[test]
    fn derive_uses_reported_when_present() {
        let report = UsageReport {
            input_tokens: Some(100),
            output_tokens: Some(200),
            cache_read_input_tokens: Some(50),
            cache_creation_input_tokens: Some(10),
            total_tokens: None,
        };
        let (i, o, src, cr, cc) = derive_token_counts(4_000, 8_000, Some(&report));
        assert_eq!(i, 100);
        assert_eq!(o, 200);
        assert_eq!(src, "reported");
        assert_eq!(cr, Some(50));
        assert_eq!(cc, Some(10));
    }

    #[test]
    fn derive_falls_back_to_estimate_when_no_report() {
        let (i, o, src, cr, cc) = derive_token_counts(8, 16, None);
        // 8 bytes / 4 = 2 input tokens; 16/4 = 4 output tokens.
        assert_eq!(i, 2);
        assert_eq!(o, 4);
        assert_eq!(src, "estimated");
        assert!(cr.is_none() && cc.is_none());
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
        let spec = AgentSpec {
            command: "claude".into(),
            args: vec!["-p".into(), "--verbose".into()],
        };
        let result = with_model_args(&spec, "claude", Some("opus"));
        // User flags stay first; model args last so duplicates resolve to
        // models.yaml-supplied value under left-to-right CLI parsing.
        assert_eq!(result.args, vec!["-p", "--verbose", "--model", "opus"]);
    }
}
