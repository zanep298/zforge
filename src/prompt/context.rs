use crate::config::Config;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct PromptContext {
    pub task_id: String,
    pub task_file: String,
    pub spec_file: String,
    pub testspec_file: String,
    pub plan_file: String,
    pub verify_file: String,
    pub patterns: String,
    pub domain_glossary: String,
    pub anti_patterns: String,
    pub context_files: Vec<String>,
    pub project_name: String,
    pub language: String,
    pub test_command: String,
    pub output_file: String,
    pub next_command: String,
    pub failed_tests: String,
    pub figma_context: String,
}

fn read_optional(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

pub fn build_context(config: &Config, task_id: &str) -> Result<PromptContext> {
    let tasks_dir = config.tasks_dir();
    let task_dir = tasks_dir.join(task_id);
    let memory_dir = config.memory_dir();

    let task_file = read_optional(&task_dir.join("task.md"));
    let spec_file = read_optional(&task_dir.join("spec.md"));
    let testspec_file = read_optional(&task_dir.join("testspec.md"));
    let plan_file = read_optional(&task_dir.join("plan.md"));
    let verify_file = read_optional(&task_dir.join("verify.md"));
    let figma_context = read_optional(&task_dir.join("figma.md"));
    let patterns = read_optional(&memory_dir.join("patterns.md"));
    let domain_glossary = read_optional(&memory_dir.join("domain-glossary.md"));
    let anti_patterns = read_optional(&memory_dir.join("anti-patterns.md"));

    let context_files: Vec<String> = config
        .opencode
        .context_files
        .iter()
        .map(|p| format!("/file {}", p.display()))
        .collect();

    Ok(PromptContext {
        task_id: task_id.to_string(),
        task_file,
        spec_file,
        testspec_file,
        plan_file,
        verify_file,
        patterns,
        domain_glossary,
        anti_patterns,
        context_files,
        project_name: config.project.name.clone(),
        language: config.project.language.clone(),
        test_command: config.project.test_command.clone(),
        output_file: String::new(),
        next_command: String::new(),
        failed_tests: String::new(),
        figma_context,
    })
}
