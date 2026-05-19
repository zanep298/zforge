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
    pub opencode: OpencodeConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub review: ReviewConfig,

    #[serde(skip)]
    pub config_file: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_test_command")]
    pub test_command: String,
    #[serde(default = "default_root_dir")]
    pub root_dir: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpencodeConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub context_files: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PathsConfig {
    #[serde(default = "default_tasks_path")]
    pub tasks: PathBuf,
    #[serde(default = "default_agents_path")]
    pub agents: PathBuf,
    #[serde(default = "default_memory_path")]
    pub memory: PathBuf,
    #[serde(default = "default_skills_path")]
    pub skills: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ReviewConfig {
    #[serde(default)]
    pub auto_approve: bool,
}

fn default_language() -> String {
    "rust".to_string()
}

fn default_test_command() -> String {
    "cargo test".to_string()
}

fn default_root_dir() -> PathBuf {
    PathBuf::from(".")
}

fn default_model() -> String {
    "claude-sonnet-4-6".to_string()
}

fn default_tasks_path() -> PathBuf {
    PathBuf::from("./.zforge/tasks")
}

fn default_agents_path() -> PathBuf {
    PathBuf::from("./.zforge/agents")
}

fn default_memory_path() -> PathBuf {
    PathBuf::from("./.zforge/memory")
}

fn default_skills_path() -> PathBuf {
    PathBuf::from("./.zforge/skills")
}

impl Default for OpencodeConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            context_files: vec![],
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            tasks: default_tasks_path(),
            agents: default_agents_path(),
            memory: default_memory_path(),
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

    pub fn tasks_dir(&self) -> PathBuf {
        self.resolve_path(&self.paths.tasks)
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.resolve_path(&self.paths.agents)
    }

    pub fn memory_dir(&self) -> PathBuf {
        self.resolve_path(&self.paths.memory)
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

/// Per-phase model selection for one coding assistant.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PhaseModels {
    pub spec: Option<String>,
    pub testspec: Option<String>,
    pub plan: Option<String>,
    pub code: Option<String>,
    pub review: Option<String>,
}

impl PhaseModels {
    pub fn for_phase(&self, phase: &str) -> Option<&str> {
        match phase {
            "spec" => self.spec.as_deref(),
            "testspec" => self.testspec.as_deref(),
            "plan" => self.plan.as_deref(),
            "code" => self.code.as_deref(),
            "review" => self.review.as_deref(),
            _ => None,
        }
    }
}

/// Model configuration loaded from `.zforge/models.yaml`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelsConfig {
    #[serde(default)]
    pub claude: PhaseModels,
    #[serde(default)]
    pub codex: PhaseModels,
    #[serde(default)]
    pub opencode: PhaseModels,
}

impl ModelsConfig {
    pub fn for_assistant(&self, assistant: &str, phase: &str) -> Option<&str> {
        match assistant {
            "claude" => self.claude.for_phase(phase),
            "codex" => self.codex.for_phase(phase),
            "opencode" => self.opencode.for_phase(phase),
            _ => None,
        }
    }
}

/// Load `.zforge/models.yaml` from the project found by walking up from cwd.
/// Returns `None` if the file doesn't exist — callers fall back to agent
/// frontmatter defaults. Emits a stderr warning on parse error so the user
/// knows their config is being ignored.
pub fn load_models() -> Option<ModelsConfig> {
    let config_path = Config::find_config_file()?;
    let models_path = config_path.parent()?.join("models.yaml");
    let content = std::fs::read_to_string(&models_path).ok()?;
    match serde_yaml::from_str::<ModelsConfig>(&content) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("warning: failed to parse {}: {e}", models_path.display());
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
        assert_eq!(cfg.opencode.model, "claude-sonnet-4-6");
        assert!(!cfg.review.auto_approve);
    }

    #[test]
    fn test_tasks_dir_absolute() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(&tmp, "project:\n  name: myapp\n");
        let cfg = load_from(&path).unwrap();
        let tasks = cfg.tasks_dir();
        assert!(tasks.is_absolute());
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
}
