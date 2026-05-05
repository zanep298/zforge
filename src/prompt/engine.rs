use super::context::PromptContext;
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

pub struct Engine {
    agents_dir: PathBuf,
}

impl Engine {
    pub fn new(agents_dir: &Path) -> Self {
        Self {
            agents_dir: agents_dir.to_path_buf(),
        }
    }

    pub fn render(&self, template_name: &str, ctx: &PromptContext) -> Result<String> {
        let path = self.agents_dir.join(format!("{}.tmpl", template_name));
        let template = std::fs::read_to_string(&path)
            .with_context(|| format!("template not found: {}", path.display()))?;
        Ok(render_template(&template, ctx))
    }

    pub fn render_and_print(
        &self,
        template_name: &str,
        ctx: &PromptContext,
    ) -> Result<()> {
        let rendered = self.render(template_name, ctx)?;
        let sep = "═".repeat(43);
        let thin = "─".repeat(43);
        println!("{}", sep.blue());
        println!("  {} › {} › {}", "ZFLOW".bold(), template_name, ctx.task_id);
        println!("{}", sep.blue());
        println!();
        println!("{}", rendered);
        println!();
        println!("{}", thin.dimmed());

        if !ctx.context_files.is_empty() {
            println!("{} Context files (load in opencode):", "📂".bold());
            for f in &ctx.context_files {
                println!("   {}", f);
            }
            println!();
        }

        if !ctx.output_file.is_empty() {
            println!("{} Output to: {}", "📄".bold(), ctx.output_file);
        }
        if !ctx.next_command.is_empty() {
            println!("{}  Next: {}", "⏭".bold(), ctx.next_command);
        }
        println!("{}", sep.blue());
        Ok(())
    }

    pub fn render_and_copy(
        &self,
        template_name: &str,
        ctx: &PromptContext,
    ) -> Result<()> {
        self.render_and_print(template_name, ctx)?;
        let rendered = self.render(template_name, ctx)?;
        match arboard::Clipboard::new() {
            Ok(mut cb) => match cb.set_text(rendered) {
                Ok(_) => println!("{}", "✓ Prompt copied to clipboard".green()),
                Err(e) => eprintln!("⚠ Could not copy to clipboard: {}", e),
            },
            Err(e) => eprintln!("⚠ Could not access clipboard: {}", e),
        }
        Ok(())
    }

    /// Auto-detect OpenCode. If `.opencode/agents/<name>-agent.md` exists and the
    /// `opencode` binary is reachable, run the prompt non-interactively via
    /// `opencode run`. Otherwise fall back to printing the prompt.
    pub fn dispatch(&self, template_name: &str, ctx: &PromptContext) -> Result<()> {
        let agent_name = format!("{}-agent", template_name);
        if let Some(bin) = find_opencode_bin() {
            let cwd = std::env::current_dir()?;
            let agent_file = cwd
                .join(".opencode")
                .join("agents")
                .join(format!("{}.md", agent_name));
            if agent_file.exists() {
                return self.render_and_run(template_name, ctx, &bin, &agent_name);
            }
        }
        self.render_and_print(template_name, ctx)
    }

    fn render_and_run(
        &self,
        template_name: &str,
        ctx: &PromptContext,
        opencode_bin: &Path,
        agent_name: &str,
    ) -> Result<()> {
        let rendered = self.render(template_name, ctx)?;
        let sep = "═".repeat(43);

        println!("{}", sep.blue());
        println!(
            "  {} › {} › {} {}",
            "ZFLOW".bold(),
            template_name,
            ctx.task_id,
            "→ opencode".dimmed()
        );
        println!("{}", sep.blue());
        println!("  {} agent:  {}", "▶".cyan().bold(), agent_name);
        println!("  {} output: {}", "📄".bold(), ctx.output_file);
        println!("{}", sep.blue());
        println!();

        let status = std::process::Command::new(opencode_bin)
            .arg("run")
            .arg(&rendered)
            .arg("--agent")
            .arg(agent_name)
            .status()
            .with_context(|| format!("failed to launch opencode at {}", opencode_bin.display()))?;

        println!();
        if status.success() {
            println!("{} OpenCode finished.", "✓".green().bold());
            if !ctx.next_command.is_empty() {
                println!("{}  Next: {}", "⏭".bold(), ctx.next_command);
            }
        } else {
            anyhow::bail!(
                "opencode exited with status {}",
                status.code().unwrap_or(-1)
            );
        }

        Ok(())
    }
}

/// Locate the `opencode` binary. Checks PATH first, then `~/.opencode/bin/`.
fn find_opencode_bin() -> Option<PathBuf> {
    // Check PATH
    if let Ok(output) = std::process::Command::new("which").arg("opencode").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    // Check ~/.opencode/bin/opencode (default install location)
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".opencode").join("bin").join("opencode");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn render_template(template: &str, ctx: &PromptContext) -> String {
    // Strip comment lines {{/* ... */}}
    let without_comments = {
        let mut out = String::new();
        for line in template.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("{{/*") && trimmed.ends_with("*/}}") {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    };

    // Process conditional blocks {{if var}}...{{end}}
    let after_conditionals = process_conditionals(&without_comments, ctx);

    // Replace {{variable}} tokens
    let result = replace_vars(&after_conditionals, ctx);

    // Trim trailing whitespace per line
    result
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn get_var(name: &str, ctx: &PromptContext) -> String {
    match name {
        "task_id" => ctx.task_id.clone(),
        "task_file" => ctx.task_file.clone(),
        "spec_file" => ctx.spec_file.clone(),
        "testspec_file" => ctx.testspec_file.clone(),
        "plan_file" => ctx.plan_file.clone(),
        "verify_file" => ctx.verify_file.clone(),
        "patterns" => ctx.patterns.clone(),
        "domain_glossary" => ctx.domain_glossary.clone(),
        "anti_patterns" => ctx.anti_patterns.clone(),
        "project_name" => ctx.project_name.clone(),
        "language" => ctx.language.clone(),
        "test_command" => ctx.test_command.clone(),
        "output_file" => ctx.output_file.clone(),
        "next_command" => ctx.next_command.clone(),
        "failed_tests" => ctx.failed_tests.clone(),
        "context_files" => ctx.context_files.join("\n"),
        "figma_context" => ctx.figma_context.clone(),
        _ => String::new(),
    }
}

fn replace_vars(s: &str, ctx: &PromptContext) -> String {
    let mut result = s.to_string();
    let variables = [
        "task_id",
        "task_file",
        "spec_file",
        "testspec_file",
        "plan_file",
        "verify_file",
        "patterns",
        "domain_glossary",
        "anti_patterns",
        "project_name",
        "language",
        "test_command",
        "output_file",
        "next_command",
        "failed_tests",
        "context_files",
        "figma_context",
    ];
    for var in variables {
        let token = format!("{{{{{}}}}}", var);
        result = result.replace(&token, &get_var(var, ctx));
    }
    result
}

fn process_conditionals(template: &str, ctx: &PromptContext) -> String {
    let mut result = template.to_string();

    let variables = [
        "patterns",
        "domain_glossary",
        "anti_patterns",
        "spec_file",
        "testspec_file",
        "plan_file",
        "verify_file",
        "context_files",
        "figma_context",
    ];

    for var in variables {
        let open_tag = format!("{{{{if {}}}}}", var);
        let close_tag = "{{end}}";

        while let Some(start) = result.find(&open_tag) {
            if let Some(end_offset) = result[start..].find(close_tag) {
                let end = start + end_offset + close_tag.len();
                let block_inner_start = start + open_tag.len();
                let block_inner_end = start + end_offset;
                let block_content = &result[block_inner_start..block_inner_end];

                let value = get_var(var, ctx);
                let replacement = if value.trim().is_empty() {
                    String::new()
                } else {
                    block_content.to_string()
                };

                result = format!("{}{}{}", &result[..start], replacement, &result[end..]);
            } else {
                break;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_ctx() -> PromptContext {
        PromptContext {
            task_id: "TASK-1".into(),
            task_file: "task content".into(),
            patterns: "some patterns".into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_render_simple_substitution() {
        let ctx = make_ctx();
        let tmpl = "Task: {{task_id}}";
        let result = render_template(tmpl, &ctx);
        assert_eq!(result.trim(), "Task: TASK-1");
    }

    #[test]
    fn test_render_conditional_block_present() {
        let ctx = make_ctx();
        let tmpl = "{{if patterns}}\nHas patterns: {{patterns}}\n{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("some patterns"));
    }

    #[test]
    fn test_render_conditional_block_empty() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            patterns: String::new(),
            ..Default::default()
        };
        let tmpl = "{{if patterns}}\nshould not appear\n{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(!result.contains("should not appear"));
    }

    #[test]
    fn test_render_missing_template() {
        let tmp = TempDir::new().unwrap();
        let engine = Engine::new(tmp.path());
        let ctx = make_ctx();
        let result = engine.render("nonexistent", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn figma_context_substituted_when_present() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: "## Frame\nsize: 375x812".into(),
            ..Default::default()
        };
        let tmpl = "{{if figma_context}}\n## Design\n{{figma_context}}\n{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("size: 375x812"));
    }

    #[test]
    fn figma_context_block_hidden_when_empty() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: String::new(),
            ..Default::default()
        };
        let tmpl = "{{if figma_context}}\nshould not appear\n{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(!result.contains("should not appear"));
    }
}
