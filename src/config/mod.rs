use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("config file not found — run: zf init")]
    NotFound,
    #[error("invalid config: {field} is required")]
    #[allow(dead_code)]
    MissingField { field: String },
    #[error("failed to parse config: {0}")]
    ParseError(#[from] serde_yaml::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub project: ProjectConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub runner: RunnerConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub onboarding: OnboardingConfig,

    #[serde(skip)]
    pub config_file: PathBuf,
}

/// The client `zforge init` set up as the project's default (FIX-015).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RunnerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// v1.5 knowledge (decision D6): the branch "integrated" is judged
/// against. Each handover manifest records it with the commit it was made on.
///
/// `dir` is where the project's own knowledge (ONBOARD REQ-006) lives:
/// `domain.md`, `conventions.md`, `rules.md` and their `.records/`,
/// committed with the code.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KnowledgeConfig {
    #[serde(default = "default_baseline")]
    pub baseline: String,
    #[serde(default = "default_knowledge_dir")]
    pub dir: PathBuf,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            baseline: default_baseline(),
            dir: default_knowledge_dir(),
        }
    }
}

fn default_baseline() -> String {
    "main".into()
}

fn default_knowledge_dir() -> PathBuf {
    PathBuf::from("docs/knowledge")
}

/// v1.5 execution policy written into each handover manifest (workflow
/// §6.3): the retry limit and the spend a run may not exceed. A missing
/// budget is reported by readiness, never assumed.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_usd: Option<f64>,
    /// Globs of test files a run may not change (`run::guard`); absent
    /// means the conventional locations, an empty list protects nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_tests: Option<Vec<String>>,
    /// Have an agent review a run's passing work against its contract
    /// before it is verified (`run::review`). Off by default: each review
    /// is another agent call on the task's budget.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub review: bool,
    /// Wall-clock limit on one agent call of a run. A safety net for a hung
    /// agent: the budget (`--max-budget-usd`) is what should stop a working
    /// one, because Claude then reports what it spent — a call zforge kills
    /// counts its whole allotment.
    #[serde(default = "default_agent_timeout_secs")]
    pub agent_timeout_secs: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            budget_usd: None,
            protected_tests: None,
            review: false,
            agent_timeout_secs: default_agent_timeout_secs(),
        }
    }
}

/// One hour. A coding call on a real task routinely passes the ten minutes
/// that once applied to every spawn.
fn default_agent_timeout_secs() -> u64 {
    3600
}

/// Whether the project's own gaps (ONBOARD REQ-010) — not onboarded, a
/// stale knowledge item, a baseline not green or known — block a handover
/// instead of only warning. Off by default: a project that has never
/// onboarded still reaches handover, as it always could.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct OnboardingConfig {
    #[serde(default)]
    pub required: bool,
}

fn default_max_iterations() -> u32 {
    3
}

/// Runner used when neither the task nor `runner.default` names one — only
/// reachable with a hand-written config, since `init` always sets it.
pub const FALLBACK_RUNNER: &str = "claude";

impl Config {
    /// The project's default runner (`runner.default`), or
    /// [`FALLBACK_RUNNER`] when the config does not set one.
    pub fn default_runner(&self) -> &str {
        self.runner
            .default
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(FALLBACK_RUNNER)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_test_command")]
    pub test_command: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PathsConfig {
    #[serde(default = "default_agents_path")]
    pub agents: PathBuf,
    #[serde(default = "default_skills_path")]
    pub skills: PathBuf,
}

fn default_language() -> String {
    "rust".to_string()
}

fn default_test_command() -> String {
    "cargo test".to_string()
}

fn default_agents_path() -> PathBuf {
    PathBuf::from("./.zforge/agents")
}

fn default_skills_path() -> PathBuf {
    PathBuf::from("./.zforge/skills")
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            agents: default_agents_path(),
            skills: default_skills_path(),
        }
    }
}

impl Config {
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<()> {
        if self.project.name.trim().is_empty() {
            return Err(ConfigError::MissingField {
                field: "project.name".to_string(),
            }
            .into());
        }
        Ok(())
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.resolve_path(&self.paths.agents)
    }

    /// The skills store this project uses (`paths.skills`), absolute.
    pub fn skills_dir(&self) -> PathBuf {
        self.resolve_path(&self.paths.skills)
    }

    /// Where the project's knowledge lives (`knowledge.dir`), absolute.
    pub fn knowledge_dir(&self) -> PathBuf {
        self.resolve_path(&self.knowledge.dir)
    }

    #[allow(dead_code)]
    pub fn project_root(&self) -> PathBuf {
        self.config_file
            .parent()
            .and_then(|d| d.parent())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }

    fn resolve_path(&self, p: &Path) -> PathBuf {
        // Expand `~` / `~/...` against $HOME so config values like
        // `~/.zforge/agents` work without forcing users to hardcode absolute
        // paths per machine.
        if let Some(expanded) = expand_tilde(p) {
            return expanded;
        }
        if p.is_absolute() {
            return p.to_path_buf();
        }
        // config_file is <project>/.zforge/config.yaml
        // parent()  → <project>/.zforge/
        // parent()  → <project>/          ← project root (what we want)
        let base = self
            .config_file
            .parent()
            .and_then(|d| d.parent())
            .unwrap_or_else(|| Path::new("."));
        base.join(p)
    }

    pub fn find_config_file() -> Option<PathBuf> {
        let mut dir = env::current_dir().ok()?;
        loop {
            let candidate = dir.join(".zforge").join("config.yaml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
        None
    }
}

/// Expand a leading `~` (alone or as `~/...`) against `$HOME`. Returns
/// `None` if the path doesn't start with `~` or `$HOME` is unavailable —
/// leaving the caller's existing relative/absolute logic in charge.
fn expand_tilde(p: &Path) -> Option<PathBuf> {
    let s = p.to_str()?;
    let rest = if s == "~" { "" } else { s.strip_prefix("~/")? };
    let home = dirs::home_dir()?;
    if rest.is_empty() {
        Some(home)
    } else {
        Some(home.join(rest))
    }
}

/// The phases a run calls an agent for: `code` writes the change, `review`
/// (optional, `execution.review`) checks it against the contract.
pub const PHASES: [&str; 2] = ["code", "review"];

/// Per-phase model selection for one coding assistant. Keys of phases that
/// no longer exist (`spec`, `testspec`, `plan`) are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PhaseModels {
    pub code: Option<String>,
    pub review: Option<String>,
}

impl PhaseModels {
    pub fn for_phase(&self, phase: &str) -> Option<&str> {
        match phase {
            "code" => self.code.as_deref(),
            "review" => self.review.as_deref(),
            _ => None,
        }
    }
}

/// A model value meaning "the client's own default": written as-is into
/// Claude's agent file, and no `--model` is passed.
pub const INHERIT: &str = "inherit";

/// zforge's default for `(client, phase)` when the user chose nothing: a
/// *tier* — Anthropic's aliases, which follow the newest models — never a
/// model name. Claude only; other clients have no stable tiers, so they run
/// on their own default. Override with `zforge models set`.
pub fn default_tier(client: &str, phase: &str) -> Option<&'static str> {
    match (client, phase) {
        ("claude", "review") => Some("opus"),
        ("claude", "code") => Some("sonnet"),
        _ => None,
    }
}

/// Model configuration loaded from `models.yaml`. Top-level YAML keys are
/// agent names (e.g. `claude`, `codex`, `opencode`, `agy`, or any custom name
/// the user adds). Backward-compatible with the previous fixed-field shape:
/// existing YAML files with `claude:`, `codex:`, `opencode:` keys parse
/// without change.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelsConfig {
    #[serde(flatten)]
    pub agents: std::collections::BTreeMap<String, PhaseModels>,
}

impl ModelsConfig {
    pub fn for_assistant(&self, assistant: &str, phase: &str) -> Option<&str> {
        self.agents.get(assistant)?.for_phase(phase)
    }

    /// Overlay `top` onto `self` — every `Some` field in `top` replaces the
    /// corresponding field in `self`. Used to layer project-local
    /// `.zforge/models.yaml` on top of global `~/.zforge/models.yaml`.
    pub fn overlay(mut self, top: ModelsConfig) -> Self {
        for (agent, top_phases) in top.agents {
            let base = self.agents.entry(agent).or_default();
            overlay_phases(base, top_phases);
        }
        self
    }
}

fn overlay_phases(base: &mut PhaseModels, top: PhaseModels) {
    if top.code.is_some() {
        base.code = top.code;
    }
    if top.review.is_some() {
        base.review = top.review;
    }
}

/// Resolve `models.yaml` with two-layer precedence: project-local overrides
/// global, per (agent, phase) field. Both layers optional.
///
/// - Global: `$ZFORGE_HOME/models.yaml` (default `~/.zforge/models.yaml`)
/// - Local: `<project>/.zforge/models.yaml`
///
/// Returns `None` only when neither file exists. Parse errors on either
/// layer emit a stderr warning and that layer is skipped — the other layer
/// still wins.
pub fn load_models() -> Option<ModelsConfig> {
    let local_path =
        Config::find_config_file().and_then(|p| p.parent().map(|d| d.join("models.yaml")));
    load_models_layered(local_path.as_deref())
}

/// Same as [`load_models`] but uses an explicit `project_root` for the local
/// layer instead of walking up from cwd. Used by paths handed `project_root`
/// directly (e.g. the orchestrator).
pub fn load_models_from_root(project_root: &Path) -> Option<ModelsConfig> {
    let local_path = project_root.join(".zforge").join("models.yaml");
    load_models_layered(Some(&local_path))
}

fn load_models_layered(local_path: Option<&Path>) -> Option<ModelsConfig> {
    let global = global_models_path().and_then(|p| load_models_from_file(&p));
    let local = local_path.and_then(load_models_from_file);
    match (global, local) {
        (None, None) => None,
        (Some(g), None) => Some(g),
        (None, Some(l)) => Some(l),
        (Some(g), Some(l)) => Some(g.overlay(l)),
    }
}

/// `$ZFORGE_HOME/models.yaml` — reuses the registry's home resolver so
/// integration tests can redirect via `ZFORGE_HOME`.
fn global_models_path() -> Option<PathBuf> {
    crate::registry::paths::registry_dir()
        .ok()
        .map(|d| d.join("models.yaml"))
}

fn load_models_from_file(path: &Path) -> Option<ModelsConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    parse_models(&content, path)
}

/// A file with nothing but comments — the template, before any choice — is
/// an empty choice, not an error.
fn parse_models(content: &str, path: &Path) -> Option<ModelsConfig> {
    if matches!(
        serde_yaml::from_str::<serde_yaml::Value>(content),
        Ok(serde_yaml::Value::Null)
    ) {
        return Some(ModelsConfig::default());
    }
    match serde_yaml::from_str::<ModelsConfig>(content) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("warning: failed to parse {}: {e}", path.display());
            None
        }
    }
}

pub fn load() -> Result<Config> {
    let path = Config::find_config_file().ok_or(ConfigError::NotFound)?;
    load_from(&path)
}

pub fn load_from(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path).map_err(|_| ConfigError::NotFound)?;
    let mut config: Config = serde_yaml::from_str(&content).map_err(ConfigError::ParseError)?;
    config.config_file = path.to_path_buf();
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use assert_fs::TempDir;

    fn write_config(dir: &TempDir, content: &str) -> PathBuf {
        let zforge_dir = dir.child(".zforge");
        zforge_dir.create_dir_all().unwrap();
        let cfg = zforge_dir.child("config.yaml");
        cfg.write_str(content).unwrap();
        cfg.path().to_path_buf()
    }

    #[test]
    fn test_load_success() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            &tmp,
            "project:\n  name: myapp\n  language: rust\n  test_command: cargo test\n  root_dir: .\n",
        );
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.name, "myapp");
        assert_eq!(cfg.project.language, "rust");
    }

    #[test]
    fn test_load_not_found() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".zforge").join("config.yaml");
        let result = load_from(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_missing_name() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(&tmp, "project:\n  name: ''\n");
        let cfg = load_from(&path).unwrap();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(&tmp, "project:\n  name: myapp\n");
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.language, "rust");
        assert_eq!(cfg.project.test_command, "cargo test");
        assert_eq!(cfg.execution.max_iterations, 3);
        assert_eq!(cfg.execution.budget_usd, None);
        assert_eq!(cfg.execution.agent_timeout_secs, 3600);
    }

    /// Keys of the removed task pipeline (`opencode`, `review`,
    /// `paths.tasks`, `paths.memory`) are ignored, not an error.
    #[test]
    fn a_config_from_the_task_pipeline_still_loads() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            &tmp,
            "project:\n  name: myapp\n  root_dir: .\nopencode:\n  model: x\n\
             paths:\n  tasks: ./.zforge/tasks\n  memory: ./.zforge/memory\n\
             review:\n  auto_approve: false\n",
        );
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.name, "myapp");
        assert!(cfg.skills_dir().is_absolute());
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = dirs::home_dir().expect("home dir available in test env");
        let expanded = expand_tilde(Path::new("~/.zforge/agents")).unwrap();
        assert_eq!(expanded, home.join(".zforge/agents"));
    }

    #[test]
    fn bare_tilde_expands_to_home() {
        let home = dirs::home_dir().expect("home dir available in test env");
        let expanded = expand_tilde(Path::new("~")).unwrap();
        assert_eq!(expanded, home);
    }

    #[test]
    fn models_config_parses_arbitrary_agent_keys() {
        // BTreeMap-flattened — any top-level key becomes an agent entry,
        // including custom names the user adds.
        let yaml = "claude:\n  review: opus\nmystery_agent:\n  code: gpt-5\n";
        let cfg: ModelsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.for_assistant("claude", "review"), Some("opus"));
        assert_eq!(cfg.for_assistant("mystery_agent", "code"), Some("gpt-5"));
        assert_eq!(cfg.for_assistant("missing", "review"), None);
    }

    #[test]
    fn models_yaml_with_removed_phases_still_loads() {
        // `spec`, `testspec` and `plan` were phases once; a file that still
        // names them loads, and they are ignored.
        let yaml = "claude:\n  plan: opus\n  code: sonnet\ncodex:\n  code: zforge_code\n\
                    opencode:\n  review: haiku\n";
        let cfg: ModelsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.for_assistant("claude", "plan"), None);
        assert_eq!(cfg.for_assistant("claude", "code"), Some("sonnet"));
        assert_eq!(cfg.for_assistant("codex", "code"), Some("zforge_code"));
        assert_eq!(cfg.for_assistant("opencode", "review"), Some("haiku"));
    }

    #[test]
    fn overlay_local_wins_per_field() {
        let mut global = ModelsConfig::default();
        global.agents.insert(
            "claude".into(),
            PhaseModels {
                review: Some("opus".into()),
                code: Some("sonnet".into()),
            },
        );
        let mut local = ModelsConfig::default();
        local.agents.insert(
            "claude".into(),
            PhaseModels {
                code: Some("haiku".into()), // override only `code`
                ..PhaseModels::default()
            },
        );
        let merged = global.overlay(local);
        // review inherited from global; code overridden by local.
        assert_eq!(merged.for_assistant("claude", "review"), Some("opus"));
        assert_eq!(merged.for_assistant("claude", "code"), Some("haiku"));
    }

    #[test]
    fn overlay_adds_agent_missing_from_base() {
        let global = ModelsConfig::default();
        let mut local = ModelsConfig::default();
        local.agents.insert(
            "agy".into(),
            PhaseModels {
                code: Some("agy-mini".into()),
                ..PhaseModels::default()
            },
        );
        let merged = global.overlay(local);
        assert_eq!(merged.for_assistant("agy", "code"), Some("agy-mini"));
    }

    #[test]
    fn non_tilde_path_returns_none() {
        assert!(expand_tilde(Path::new("./foo")).is_none());
        assert!(expand_tilde(Path::new("/abs/path")).is_none());
        // `~user` is intentionally not supported — only `~` and `~/`.
        assert!(expand_tilde(Path::new("~user/foo")).is_none());
    }

    #[test]
    fn resolve_path_expands_tilde_in_paths_config() {
        // A config with paths.agents = "~/.zforge/agents" must resolve to an
        // absolute path under $HOME (not a project-relative one).
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            &tmp,
            "project:\n  name: myapp\npaths:\n  agents: \"~/.zforge/agents\"\n",
        );
        let cfg = load_from(&path).unwrap();
        let resolved = cfg.agents_dir();
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolved, home.join(".zforge/agents"));
    }

    #[test]
    fn test_load_walk_up() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(&tmp, "project:\n  name: walk-up-test\n");
        let subdir = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&subdir).unwrap();

        // Simulate finding config by verifying load_from works with given path
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.project.name, "walk-up-test");
    }

    #[test]
    fn a_models_file_of_comments_only_is_an_empty_choice() {
        let template = include_str!("../../templates/models.yaml");
        let m = parse_models(template, Path::new("models.yaml")).unwrap();
        assert!(m.agents.is_empty(), "the template chooses no model");
        assert!(parse_models("", Path::new("m")).unwrap().agents.is_empty());
    }
}
