//! Deterministic fake agent for orchestrator integration tests.
//!
//! Two control surfaces:
//!   1. **JSON config** — `args[0]` or `FAKE_CONFIG` env var points at a JSON
//!      file `{exit_code, stderr, stdout, sleep_ms}`. Lets tests script
//!      per-attempt behavior by giving each agent its own config path.
//!   2. **Env vars** — `FAKE_EXIT`, `FAKE_STDERR`, `FAKE_STDOUT`, `FAKE_SLEEP_MS`.
//!      Lower priority than the JSON config. Useful for ad-hoc / shell tests.
//!
//! Always drains stdin (the orchestrator pipes the prompt there) so the writer
//! never blocks on a full pipe.

use std::io::{Read, Write};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Default, serde::Deserialize)]
struct FakeConfig {
    exit_code: Option<i32>,
    stderr: Option<String>,
    stdout: Option<String>,
    sleep_ms: Option<u64>,
}

fn load_config() -> FakeConfig {
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("FAKE_CONFIG").ok());
    let Some(p) = path else {
        return FakeConfig::default();
    };
    match std::fs::read_to_string(&p) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => FakeConfig::default(),
    }
}

fn env_override<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

fn main() -> ExitCode {
    // Drain stdin so the parent never blocks on the prompt pipe.
    let mut prompt = String::new();
    let _ = std::io::stdin().read_to_string(&mut prompt);

    // Optional argv dump — tests set FAKE_ARGV_DUMP to a path so they can
    // verify the exact args the orchestrator spawned with (e.g. that
    // `--model opus` got injected per models.yaml).
    if let Ok(dump_path) = std::env::var("FAKE_ARGV_DUMP") {
        let argv: Vec<String> = std::env::args().collect();
        let _ = std::fs::write(&dump_path, argv.join("\n"));
    }

    let cfg = load_config();

    let stderr_msg = cfg
        .stderr
        .clone()
        .or_else(|| std::env::var("FAKE_STDERR").ok())
        .unwrap_or_default();
    if !stderr_msg.is_empty() {
        let _ = writeln!(std::io::stderr(), "{stderr_msg}");
    }

    let stdout_msg = cfg
        .stdout
        .clone()
        .or_else(|| std::env::var("FAKE_STDOUT").ok());
    if let Some(s) = stdout_msg {
        let _ = writeln!(std::io::stdout(), "{s}");
    }

    let sleep_ms = cfg
        .sleep_ms
        .or_else(|| env_override::<u64>("FAKE_SLEEP_MS"));
    if let Some(ms) = sleep_ms {
        std::thread::sleep(Duration::from_millis(ms));
    }

    let exit_code = cfg
        .exit_code
        .or_else(|| env_override::<i32>("FAKE_EXIT"))
        .unwrap_or(0);

    // ExitCode is u8; clamp.
    ExitCode::from((exit_code & 0xFF) as u8)
}
