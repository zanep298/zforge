use crate::cost::schema::CostEntry;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

/// Aggregated stats per grouping key. Used for both `--by task` and
/// `--by phase` rollups.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CostRollup {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub spawns: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
    pub total_cost_usd: f64,
    pub total_duration_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Task,
    Phase,
    Agent,
    Model,
    Step,
}

impl GroupBy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "task" => Some(Self::Task),
            "phase" => Some(Self::Phase),
            "agent" => Some(Self::Agent),
            "model" => Some(Self::Model),
            "step" => Some(Self::Step),
            _ => None,
        }
    }
}

fn key_for(entry: &CostEntry, by: GroupBy) -> String {
    match by {
        GroupBy::Task => entry.task_id.clone(),
        GroupBy::Phase => entry.phase.clone(),
        GroupBy::Agent => entry.agent.clone(),
        GroupBy::Model => {
            // Include agent so unknown-model rows don't collapse into one
            // "(none)" bucket across different agents (claude/codex/etc).
            let model = entry.model.as_deref().unwrap_or("(none)");
            format!("{}/{model}", entry.agent)
        }
        GroupBy::Step => {
            let model = entry.model.as_deref().unwrap_or("(none)");
            format!(
                "{}/{}/{}/{}",
                entry.task_id, entry.phase, entry.agent, model
            )
        }
    }
}

/// Sum entries by key. Sorted by `total_cost_usd` descending so the
/// expensive items pop to the top.
pub fn rollup(entries: &[CostEntry], by: GroupBy) -> Vec<CostRollup> {
    let mut acc: BTreeMap<String, CostRollup> = BTreeMap::new();
    for e in entries {
        let key = key_for(e, by);
        let bucket = acc.entry(key.clone()).or_insert_with(|| CostRollup {
            key: key.clone(),
            task_id: (by == GroupBy::Step).then(|| e.task_id.clone()),
            phase: (by == GroupBy::Step).then(|| e.phase.clone()),
            agent: (by == GroupBy::Step).then(|| e.agent.clone()),
            model: (by == GroupBy::Step)
                .then(|| e.model.clone().unwrap_or_else(|| "(none)".into())),
            ..CostRollup::default()
        });
        bucket.spawns += 1;
        bucket.input_tokens += e.est_input_tokens;
        bucket.output_tokens += e.est_output_tokens;
        bucket.total_tokens += e
            .reported_total_tokens
            .map(|n| n as usize)
            .unwrap_or(e.est_input_tokens + e.est_output_tokens);
        bucket.total_cost_usd += e.est_cost_usd;
        bucket.total_duration_ms += e.duration_ms;
    }
    let mut out: Vec<CostRollup> = acc.into_values().collect();
    if by == GroupBy::Step {
        out.sort_by(|a, b| {
            a.task_id
                .cmp(&b.task_id)
                .then_with(|| phase_rank(a.phase.as_deref()).cmp(&phase_rank(b.phase.as_deref())))
                .then_with(|| a.agent.cmp(&b.agent))
                .then_with(|| a.model.cmp(&b.model))
        });
    } else {
        out.sort_by(|a, b| {
            b.total_cost_usd
                .partial_cmp(&a.total_cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    out
}

fn phase_rank(phase: Option<&str>) -> usize {
    match phase.unwrap_or_default() {
        "spec" => 0,
        "testspec" => 1,
        "plan" => 2,
        "code" => 3,
        "verify" => 4,
        "review" => 5,
        _ => usize::MAX,
    }
}

/// Filter to entries with timestamp >= `since`.
pub fn filter_since(entries: &[CostEntry], since: DateTime<Utc>) -> Vec<CostEntry> {
    entries
        .iter()
        .filter(|e| e.timestamp >= since)
        .cloned()
        .collect()
}

pub fn filter_task<'a>(entries: &'a [CostEntry], task_id: &str) -> Vec<&'a CostEntry> {
    entries.iter().filter(|e| e.task_id == task_id).collect()
}

/// Pretty table for `zforge cost report`. Caller already filtered + rolled
/// up; this just formats.
pub fn format_table(rollups: &[CostRollup], by: GroupBy) -> String {
    if by == GroupBy::Step {
        return format_step_table(rollups);
    }

    let header = match by {
        GroupBy::Task => "TASK",
        GroupBy::Phase => "PHASE",
        GroupBy::Agent => "AGENT",
        GroupBy::Model => "MODEL",
        GroupBy::Step => unreachable!(),
    };
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(
        out,
        "{:<24} {:>7} {:>12} {:>12} {:>12} {:>12} {:>10}",
        header, "SPAWNS", "IN_TOKENS", "OUT_TOKENS", "TOKENS", "COST_USD", "DUR_S"
    );
    let _ = writeln!(out, "{}", "-".repeat(94));
    let mut total = CostRollup {
        key: "(total)".into(),
        ..CostRollup::default()
    };
    for r in rollups {
        let _ = writeln!(
            out,
            "{:<24} {:>7} {:>12} {:>12} {:>12} {:>12.4} {:>10.1}",
            r.key,
            r.spawns,
            r.input_tokens,
            r.output_tokens,
            r.total_tokens,
            r.total_cost_usd,
            (r.total_duration_ms as f64) / 1000.0
        );
        total.spawns += r.spawns;
        total.input_tokens += r.input_tokens;
        total.output_tokens += r.output_tokens;
        total.total_tokens += r.total_tokens;
        total.total_cost_usd += r.total_cost_usd;
        total.total_duration_ms += r.total_duration_ms;
    }
    let _ = writeln!(out, "{}", "-".repeat(94));
    let _ = writeln!(
        out,
        "{:<24} {:>7} {:>12} {:>12} {:>12} {:>12.4} {:>10.1}",
        total.key,
        total.spawns,
        total.input_tokens,
        total.output_tokens,
        total.total_tokens,
        total.total_cost_usd,
        (total.total_duration_ms as f64) / 1000.0
    );
    out
}

fn format_step_table(rollups: &[CostRollup]) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(
        out,
        "{:<16} {:<10} {:<16} {:<24} {:>7} {:>12} {:>12} {:>12} {:>12} {:>10}",
        "TASK",
        "PHASE",
        "AGENT",
        "MODEL",
        "SPAWNS",
        "IN_TOKENS",
        "OUT_TOKENS",
        "TOKENS",
        "COST_USD",
        "DUR_S"
    );
    let _ = writeln!(out, "{}", "-".repeat(127));
    let mut total = CostRollup {
        key: "(total)".into(),
        ..CostRollup::default()
    };
    for r in rollups {
        let _ = writeln!(
            out,
            "{:<16} {:<10} {:<16} {:<24} {:>7} {:>12} {:>12} {:>12} {:>12.4} {:>10.1}",
            r.task_id.as_deref().unwrap_or("-"),
            r.phase.as_deref().unwrap_or("-"),
            r.agent.as_deref().unwrap_or("-"),
            r.model.as_deref().unwrap_or("(none)"),
            r.spawns,
            r.input_tokens,
            r.output_tokens,
            r.total_tokens,
            r.total_cost_usd,
            (r.total_duration_ms as f64) / 1000.0
        );
        total.spawns += r.spawns;
        total.input_tokens += r.input_tokens;
        total.output_tokens += r.output_tokens;
        total.total_tokens += r.total_tokens;
        total.total_cost_usd += r.total_cost_usd;
        total.total_duration_ms += r.total_duration_ms;
    }
    let _ = writeln!(out, "{}", "-".repeat(127));
    let _ = writeln!(
        out,
        "{:<16} {:<10} {:<16} {:<24} {:>7} {:>12} {:>12} {:>12} {:>12.4} {:>10.1}",
        "(total)",
        "",
        "",
        "",
        total.spawns,
        total.input_tokens,
        total.output_tokens,
        total.total_tokens,
        total.total_cost_usd,
        (total.total_duration_ms as f64) / 1000.0
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn entry(task: &str, phase: &str, cost: f64, in_tok: usize, out_tok: usize) -> CostEntry {
        CostEntry {
            timestamp: Utc::now(),
            task_id: task.into(),
            phase: phase.into(),
            agent: "claude".into(),
            model: Some("haiku".into()),
            prompt_chars: in_tok * 4,
            stdout_chars: out_tok * 4,
            stderr_chars: 0,
            duration_ms: 1000,
            exit_code: 0,
            timed_out: false,
            est_input_tokens: in_tok,
            est_output_tokens: out_tok,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            reported_total_tokens: None,
            tokens_source: "estimated".into(),
            est_cost_usd: cost,
        }
    }

    #[test]
    fn rollup_by_task_sums_per_task() {
        let entries = vec![
            entry("T1", "plan", 0.05, 100, 200),
            entry("T1", "code", 0.10, 300, 400),
            entry("T2", "plan", 0.02, 50, 60),
        ];
        let r = rollup(&entries, GroupBy::Task);
        assert_eq!(r.len(), 2);
        // T1 = 0.15, T2 = 0.02 → T1 first (sorted desc by cost).
        assert_eq!(r[0].key, "T1");
        assert!((r[0].total_cost_usd - 0.15).abs() < 1e-9);
        assert_eq!(r[0].spawns, 2);
        assert_eq!(r[1].key, "T2");
    }

    #[test]
    fn rollup_by_phase() {
        let entries = vec![
            entry("T1", "plan", 0.05, 100, 200),
            entry("T2", "plan", 0.10, 300, 400),
            entry("T1", "code", 0.02, 50, 60),
        ];
        let r = rollup(&entries, GroupBy::Phase);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].key, "plan");
        assert_eq!(r[0].spawns, 2);
    }

    #[test]
    fn rollup_by_step_keeps_task_phase_agent_model() {
        let entries = vec![
            entry("T1", "plan", 0.05, 100, 200),
            entry("T1", "code", 0.10, 300, 400),
        ];
        let r = rollup(&entries, GroupBy::Step);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].task_id.as_deref(), Some("T1"));
        assert_eq!(r[0].phase.as_deref(), Some("plan"));
        assert_eq!(r[0].agent.as_deref(), Some("claude"));
        assert_eq!(r[0].model.as_deref(), Some("haiku"));
        assert_eq!(r[0].total_tokens, 300);
    }

    #[test]
    fn reported_total_tokens_feed_total_column() {
        let mut e = entry("T1", "plan", 0.05, 100, 200);
        e.reported_total_tokens = Some(12345);
        let r = rollup(&[e], GroupBy::Step);
        assert_eq!(r[0].input_tokens, 100);
        assert_eq!(r[0].output_tokens, 200);
        assert_eq!(r[0].total_tokens, 12345);
    }

    #[test]
    fn filter_since_drops_older_entries() {
        let mut old = entry("T1", "plan", 0.05, 100, 200);
        old.timestamp = Utc::now() - Duration::days(2);
        let recent = entry("T1", "code", 0.10, 300, 400);
        let entries = vec![old, recent];
        let since = Utc::now() - Duration::hours(1);
        let kept = filter_since(&entries, since);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].phase, "code");
    }

    #[test]
    fn format_table_has_header_and_total() {
        let rollups = vec![CostRollup {
            key: "T1".into(),
            spawns: 2,
            input_tokens: 1000,
            output_tokens: 2000,
            total_tokens: 3000,
            total_cost_usd: 0.123,
            total_duration_ms: 5000,
            ..CostRollup::default()
        }];
        let out = format_table(&rollups, GroupBy::Task);
        assert!(out.contains("TASK"));
        assert!(out.contains("T1"));
        assert!(out.contains("(total)"));
        assert!(out.contains("0.1230"));
    }

    #[test]
    fn format_step_table_shows_agent_and_model_columns() {
        let rollups = rollup(
            &[
                entry("T1", "plan", 0.05, 100, 200),
                entry("T1", "code", 0.10, 300, 400),
            ],
            GroupBy::Step,
        );
        let out = format_table(&rollups, GroupBy::Step);
        assert!(out.contains("TASK"));
        assert!(out.contains("PHASE"));
        assert!(out.contains("AGENT"));
        assert!(out.contains("MODEL"));
        assert!(out.contains("haiku"));
    }
}
