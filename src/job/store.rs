use crate::config::Config;
use crate::job::schema::{Job, JobKind, JobStatus};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `<project>/.zforge/jobs/`
pub fn jobs_dir(config: &Config) -> PathBuf {
    config.project_root().join(".zforge").join("jobs")
}

pub fn job_dir(config: &Config, job_id: &str) -> PathBuf {
    jobs_dir(config).join(job_id)
}

pub fn job_yaml_path(config: &Config, job_id: &str) -> PathBuf {
    job_dir(config, job_id).join("job.yaml")
}

pub fn job_log_path(config: &Config, job_id: &str) -> PathBuf {
    job_dir(config, job_id).join("log")
}

/// `J-YYYYMMDD-HHMMSS-XXXX` — chronological + collision-resistant within
/// the same second. The 4-char suffix is from the lower bits of a monotonic
/// counter seeded by the wall clock; avoids `rand` crate.
pub fn new_job_id() -> String {
    let now = Utc::now();
    let suffix: u32 = (now.timestamp_subsec_nanos() ^ std::process::id()) & 0xFFFF;
    format!("J-{}-{:04x}", now.format("%Y%m%d-%H%M%S"), suffix)
}

/// Allocate a fresh job record, scaffold its directory, and persist a
/// queued-status YAML. Returns the populated `Job`.
pub fn create_job(
    config: &Config,
    task_id: &str,
    kind: JobKind,
    command_override: Option<String>,
    timeout_secs: u64,
    max_iterations: u32,
) -> Result<Job> {
    let job_id = new_job_id();
    let job = Job {
        job_id: job_id.clone(),
        task_id: task_id.to_string(),
        kind,
        status: JobStatus::Queued,
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
        worker_pid: None,
        exit_code: None,
        error: None,
        command_override,
        timeout_secs,
        max_iterations,
    };
    let dir = job_dir(config, &job_id);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {dir:?}"))?;
    // Pre-create empty log so `tail` consumers don't see ENOENT before the
    // worker has its first chance to write.
    std::fs::File::create(job_log_path(config, &job_id)).context("create job log")?;
    save_atomic(config, &job)?;
    Ok(job)
}

pub fn load_job(config: &Config, job_id: &str) -> Result<Job> {
    let path = job_yaml_path(config, job_id);
    let raw = std::fs::read_to_string(&path).map_err(|e| anyhow!("read {path:?}: {e}"))?;
    serde_yaml::from_str(&raw).with_context(|| format!("parse {path:?}"))
}

/// Atomic write via tmp + rename. Caller holds responsibility for ordering;
/// no global lock — concurrent updates on the same job from two workers is
/// a bug (each job has exactly one worker by design).
pub fn save_atomic(config: &Config, job: &Job) -> Result<()> {
    let path = job_yaml_path(config, &job.job_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
    let tmp = path.with_extension("yaml.tmp");
    let yaml = serde_yaml::to_string(job).context("serialize job")?;
    {
        let mut f = std::fs::File::create(&tmp).with_context(|| format!("create {tmp:?}"))?;
        f.write_all(yaml.as_bytes()).context("write job tmp")?;
        f.sync_all().context("fsync job tmp")?;
    }
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {tmp:?} -> {path:?}"))?;
    Ok(())
}

/// Enumerate every `<project>/.zforge/jobs/J-*` entry that parses as a Job.
/// Sorted newest first by `created_at`.
pub fn list_jobs(config: &Config) -> Result<Vec<Job>> {
    let dir = jobs_dir(config);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut jobs = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {dir:?}"))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !id.starts_with("J-") {
            continue;
        }
        match load_job(config, &id) {
            Ok(j) => jobs.push(j),
            Err(_) => continue, // skip malformed
        }
    }
    jobs.sort_by_key(|j| std::cmp::Reverse(j.created_at));
    Ok(jobs)
}

#[allow(dead_code)]
pub fn read_log(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("read log {path:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_job_id_is_unique_within_process() {
        let a = new_job_id();
        std::thread::sleep(std::time::Duration::from_nanos(1));
        let b = new_job_id();
        assert_ne!(a, b);
        assert!(a.starts_with("J-"));
    }
}
