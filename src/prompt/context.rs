use crate::config::Config;
use anyhow::Result;
use std::env;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct PromptContext {
    pub task_id: String,
    pub task_file: String,
    pub spec_file: String,
    pub testspec_file: String,
    pub plan_file: String,
    pub verify_file: String,
    pub context_files: Vec<String>,
    pub project_name: String,
    pub language: String,
    pub test_command: String,
    pub output_file: String,
    pub next_command: String,
    pub failed_tests: String,
    pub figma_context: String,
    pub task_ref: String,
    pub spec_ref: String,
    pub testspec_ref: String,
    pub plan_ref: String,
    pub verify_ref: String,
    pub figma_ref: String,
    pub patterns_ref: String,
    pub domain_glossary_ref: String,
    pub anti_patterns_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPhase {
    Spec,
    Testspec,
    Plan,
    Code,
    VerifyAnalysis,
    Review,
}

fn read_optional(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn display_path(path: &Path) -> String {
    let cwd = env::current_dir().ok();
    if let Some(cwd) = cwd {
        if let Ok(relative) = path.strip_prefix(&cwd) {
            return relative.display().to_string();
        }
    }
    path.display().to_string()
}

fn file_ref(path: &Path) -> String {
    format!("/file {}", display_path(path))
}

fn has_content_ref(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let has_substance = content.lines().any(|l| {
        let trimmed = l.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    });
    has_substance.then(|| file_ref(path))
}

fn configured_context_refs(config: &Config) -> Vec<String> {
    config
        .opencode
        .context_files
        .iter()
        .map(|p| format!("/file {}", p.display()))
        .collect()
}

pub fn build_context_for_phase(
    config: &Config,
    task_id: &str,
    phase: PromptPhase,
) -> Result<PromptContext> {
    let tasks_dir = config.tasks_dir();
    let task_dir = tasks_dir.join(task_id);
    let memory_dir = config.memory_dir();

    let task_path = task_dir.join("task.md");
    let spec_path = task_dir.join("spec.md");
    let testspec_path = task_dir.join("testspec.md");
    let plan_path = task_dir.join("plan.md");
    let verify_path = task_dir.join("verify.md");
    let figma_path = task_dir.join("figma.md");
    let patterns_path = memory_dir.join("patterns.md");
    let domain_glossary_path = memory_dir.join("domain-glossary.md");
    let anti_patterns_path = memory_dir.join("anti-patterns.md");

    let task_ref = file_ref(&task_path);
    let spec_ref = file_ref(&spec_path);
    let testspec_ref = file_ref(&testspec_path);
    let plan_ref = file_ref(&plan_path);
    let verify_ref = file_ref(&verify_path);
    let figma_ref = file_ref(&figma_path);
    let patterns_ref = file_ref(&patterns_path);
    let domain_glossary_ref = file_ref(&domain_glossary_path);
    let anti_patterns_ref = file_ref(&anti_patterns_path);

    let task_file = match phase {
        PromptPhase::Spec | PromptPhase::Testspec => read_optional(&task_path),
        _ => String::new(),
    };
    let spec_file = match phase {
        PromptPhase::Testspec | PromptPhase::Plan => read_optional(&spec_path),
        _ => String::new(),
    };
    let testspec_file = match phase {
        PromptPhase::Plan
        | PromptPhase::Code
        | PromptPhase::VerifyAnalysis
        | PromptPhase::Review => read_optional(&testspec_path),
        _ => String::new(),
    };
    let plan_file = match phase {
        PromptPhase::Code | PromptPhase::Review => read_optional(&plan_path),
        _ => String::new(),
    };
    let verify_file = match phase {
        PromptPhase::VerifyAnalysis | PromptPhase::Review => read_optional(&verify_path),
        _ => String::new(),
    };
    let figma_context = match phase {
        PromptPhase::Spec | PromptPhase::Code => read_optional(&figma_path),
        _ => String::new(),
    };

    let mut context_files = Vec::new();
    match phase {
        PromptPhase::Spec => {
            context_files.push(task_ref.clone());
            if let Some(r) = has_content_ref(&figma_path) {
                context_files.push(r);
            }
            if let Some(r) = has_content_ref(&patterns_path) {
                context_files.push(r);
            }
            if let Some(r) = has_content_ref(&domain_glossary_path) {
                context_files.push(r);
            }
        }
        PromptPhase::Testspec => {
            context_files.push(spec_ref.clone());
            context_files.push(task_ref.clone());
            if let Some(r) = has_content_ref(&anti_patterns_path) {
                context_files.push(r);
            }
        }
        PromptPhase::Plan => {
            context_files.push(spec_ref.clone());
            context_files.push(testspec_ref.clone());
            if let Some(r) = has_content_ref(&patterns_path) {
                context_files.push(r);
            }
        }
        PromptPhase::Code => {
            context_files.push(plan_ref.clone());
            context_files.push(testspec_ref.clone());
            if let Some(r) = has_content_ref(&patterns_path) {
                context_files.push(r);
            }
            if let Some(r) = has_content_ref(&anti_patterns_path) {
                context_files.push(r);
            }
            context_files.extend(configured_context_refs(config));
        }
        PromptPhase::VerifyAnalysis => {
            context_files.push(verify_ref.clone());
            context_files.push(testspec_ref.clone());
        }
        PromptPhase::Review => {
            context_files.push(verify_ref.clone());
            context_files.push(plan_ref.clone());
            context_files.push(testspec_ref.clone());
        }
    }

    Ok(PromptContext {
        task_id: task_id.to_string(),
        task_file,
        spec_file,
        testspec_file,
        plan_file,
        verify_file,
        context_files,
        project_name: config.project.name.clone(),
        language: config.project.language.clone(),
        test_command: config.project.test_command.clone(),
        output_file: String::new(),
        next_command: String::new(),
        failed_tests: String::new(),
        figma_context,
        task_ref,
        spec_ref,
        testspec_ref,
        plan_ref,
        verify_ref,
        figma_ref,
        patterns_ref,
        domain_glossary_ref,
        anti_patterns_ref,
    })
}
