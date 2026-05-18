use super::context::PromptContext;
use crate::fs::tokens;
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

    pub fn render_and_print(&self, template_name: &str, ctx: &PromptContext) -> Result<()> {
        let rendered = self.render(template_name, ctx)?;
        self.print_render_header(template_name, ctx, &rendered);
        Ok(())
    }

    pub fn render_and_copy(&self, template_name: &str, ctx: &PromptContext) -> Result<()> {
        let rendered = self.render(template_name, ctx)?;
        self.print_render_header(template_name, ctx, &rendered);
        match arboard::Clipboard::new() {
            Ok(mut cb) => match cb.set_text(rendered) {
                Ok(_) => println!("{}", "✓ Prompt copied to clipboard".green()),
                Err(e) => eprintln!("⚠ Could not copy to clipboard: {}", e),
            },
            Err(e) => eprintln!("⚠ Could not access clipboard: {}", e),
        }
        Ok(())
    }

    fn print_render_header(&self, template_name: &str, ctx: &PromptContext, rendered: &str) {
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
            println!("{} Context files (load in your agent):", "📂".bold());
            for f in &ctx.context_files {
                println!("   {}", f);
            }
            println!();
        }

        if !ctx.output_file.is_empty() {
            println!("{} Output to: {}", "📄".bold(), ctx.output_file);
        }
        println!(
            "  {} prompt:  {} tokens",
            "📊".bold(),
            tokens::fmt(tokens::estimate(rendered))
        );
        if !ctx.next_command.is_empty() {
            println!("{}  Next: {}", "⏭".bold(), ctx.next_command);
        }
        println!("{}", sep.blue());
    }

    /// Auto-detect executor. Try Claude Code first (default), then OpenCode as
    /// fallback. For each, require both the binary and the corresponding agent
    /// file (`.claude/agents/<name>-agent.md` or `.opencode/agents/<name>-agent.md`).
    /// Otherwise fall back to printing the prompt.
    pub fn dispatch(&self, template_name: &str, ctx: &PromptContext) -> Result<()> {
        let agent_name = format!("{}-agent", template_name);
        let cwd = std::env::current_dir()?;

        // Prefer Claude Code
        if let Some(bin) = find_claude_bin() {
            let agent_file = cwd
                .join(".claude")
                .join("agents")
                .join(format!("{}.md", agent_name));
            if agent_file.exists() {
                return self.render_and_run_claude(template_name, ctx, &bin, &agent_name);
            }
        }

        // Fallback: OpenCode
        if let Some(bin) = find_opencode_bin() {
            let agent_file = cwd
                .join(".opencode")
                .join("agents")
                .join(format!("{}.md", agent_name));
            if agent_file.exists() {
                return self.render_and_run_opencode(template_name, ctx, &bin, &agent_name);
            }
        }

        self.render_and_print(template_name, ctx)
    }

    fn render_and_run_claude(
        &self,
        template_name: &str,
        ctx: &PromptContext,
        claude_bin: &Path,
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
            "→ claude".dimmed()
        );
        println!("{}", sep.blue());
        println!("  {} agent:  {}", "▶".cyan().bold(), agent_name);
        println!("  {} output: {}", "📄".bold(), ctx.output_file);
        println!(
            "  {} prompt:  {} tokens",
            "📊".bold(),
            tokens::fmt(tokens::estimate(&rendered))
        );
        println!("{}", sep.blue());
        println!();

        let status = std::process::Command::new(claude_bin)
            .arg("-p")
            .arg(&rendered)
            .status()
            .with_context(|| format!("failed to launch claude at {}", claude_bin.display()))?;

        println!();
        if status.success() {
            println!("{} Claude finished.", "✓".green().bold());
            if !ctx.next_command.is_empty() {
                println!("{}  Next: {}", "⏭".bold(), ctx.next_command);
            }
        } else {
            anyhow::bail!("claude exited with status {}", status.code().unwrap_or(-1));
        }

        Ok(())
    }

    fn render_and_run_opencode(
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
        println!(
            "  {} prompt:  {} tokens",
            "📊".bold(),
            tokens::fmt(tokens::estimate(&rendered))
        );
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

/// Locate the `claude` binary. Checks PATH first (cross-platform via the
/// `which` crate — no `which` subprocess and no Unix-only assumptions), then
/// falls back to common install paths under `$HOME`.
fn find_claude_bin() -> Option<PathBuf> {
    find_bin("claude", &[".claude/local/claude", ".claude/bin/claude"])
}

/// Locate the `opencode` binary. Checks PATH first, then `~/.opencode/bin/`.
fn find_opencode_bin() -> Option<PathBuf> {
    find_bin("opencode", &[".opencode/bin/opencode"])
}

/// Look up `bin` on PATH (works on Windows: respects PATHEXT, returns the
/// resolved executable path), falling back to `$HOME/<sub>` for each `sub`.
fn find_bin(bin: &str, home_fallbacks: &[&str]) -> Option<PathBuf> {
    if let Ok(path) = which::which(bin) {
        return Some(path);
    }
    let home = dirs::home_dir()?;
    for sub in home_fallbacks {
        let candidate = home.join(sub);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn render_template(template: &str, ctx: &PromptContext) -> String {
    let stripped = strip_comment_lines(template);
    let tokens = tokenize(&stripped);
    let nodes = parse(&tokens);
    let mut out = String::new();
    render_nodes(&nodes, ctx, &mut out);

    // Trim trailing whitespace per line so blank conditional blocks don't
    // leave straggling spaces in the rendered output.
    out.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_comment_lines(template: &str) -> String {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    Text(&'a str),
    Var(&'a str),
    If(&'a str),
    End,
}

/// Scans the template once, emitting tokens. Unrecognized `{{...}}` runs are
/// preserved as text so the template author sees the unsubstituted token in
/// the output (loud failure) rather than having it silently disappear.
fn tokenize(template: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut cursor = 0;

    while cursor < template.len() {
        match template[cursor..].find("{{") {
            None => {
                tokens.push(Token::Text(&template[cursor..]));
                break;
            }
            Some(rel_open) => {
                let open = cursor + rel_open;
                if open > cursor {
                    tokens.push(Token::Text(&template[cursor..open]));
                }
                // Locate matching `}}`. Stay on the same template; allow
                // arbitrary characters in between (we restrict to ASCII tag
                // names below).
                let search_from = open + 2;
                let close_rel = template[search_from..].find("}}");
                match close_rel {
                    None => {
                        // No closing brace — treat the unterminated tag as text
                        // and stop scanning further.
                        tokens.push(Token::Text(&template[open..]));
                        break;
                    }
                    Some(rel_close) => {
                        let close = search_from + rel_close;
                        let inner = template[search_from..close].trim();
                        let next = close + 2;

                        if inner == "end" {
                            tokens.push(Token::End);
                        } else if let Some(rest) = inner.strip_prefix("if ") {
                            let name = rest.trim();
                            if is_ident(name) {
                                tokens.push(Token::If(name));
                            } else {
                                tokens.push(Token::Text(&template[open..next]));
                            }
                        } else if is_ident(inner) {
                            // Look up the position of `inner` inside the
                            // original slice so we can hand back a &str
                            // reference. The trim above may have shifted byte
                            // offsets; recompute against the source.
                            let inner_start =
                                search_from + template[search_from..close].find(inner).unwrap_or(0);
                            let inner_end = inner_start + inner.len();
                            tokens.push(Token::Var(&template[inner_start..inner_end]));
                        } else {
                            tokens.push(Token::Text(&template[open..next]));
                        }
                        cursor = next;
                    }
                }
            }
        }
    }

    tokens
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[derive(Debug, Clone)]
enum Node<'a> {
    Text(&'a str),
    Var(&'a str),
    If(&'a str, Vec<Node<'a>>),
}

/// Parses the token stream into a node tree. Unmatched `{{end}}` is dropped;
/// an `{{if}}` without a matching `{{end}}` consumes everything to EOF (its
/// contents still render — that is the safer failure mode for prompt
/// templates than silently truncating downstream variables).
fn parse<'a>(tokens: &'a [Token<'a>]) -> Vec<Node<'a>> {
    let mut iter = tokens.iter().peekable();
    parse_until(&mut iter, false)
}

fn parse_until<'a, I>(iter: &mut std::iter::Peekable<I>, stop_on_end: bool) -> Vec<Node<'a>>
where
    I: Iterator<Item = &'a Token<'a>>,
{
    let mut out = Vec::new();
    while let Some(tok) = iter.peek() {
        match tok {
            Token::End => {
                if stop_on_end {
                    iter.next();
                    return out;
                }
                // Stray `{{end}}` — drop it; it has no opening tag.
                iter.next();
            }
            Token::If(name) => {
                let name = *name;
                iter.next();
                let inner = parse_until(iter, true);
                out.push(Node::If(name, inner));
            }
            Token::Var(name) => {
                let name = *name;
                iter.next();
                out.push(Node::Var(name));
            }
            Token::Text(s) => {
                let s = *s;
                iter.next();
                out.push(Node::Text(s));
            }
        }
    }
    out
}

fn render_nodes(nodes: &[Node<'_>], ctx: &PromptContext, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(s) => out.push_str(s),
            Node::Var(name) => out.push_str(&get_var(name, ctx)),
            Node::If(name, inner) => {
                if !get_var_for_condition(name, ctx).trim().is_empty() {
                    render_nodes(inner, ctx, out);
                }
            }
        }
    }
}

/// Resolves a template variable. Unknown variables render as `{{name}}` so
/// typos in templates are visible in the rendered output instead of silently
/// becoming empty strings.
fn get_var(name: &str, ctx: &PromptContext) -> String {
    match name {
        "task_id" => ctx.task_id.clone(),
        "task_file" => ctx.task_file.clone(),
        "spec_file" => ctx.spec_file.clone(),
        "testspec_file" => ctx.testspec_file.clone(),
        "plan_file" => ctx.plan_file.clone(),
        "verify_file" => ctx.verify_file.clone(),
        "project_name" => ctx.project_name.clone(),
        "language" => ctx.language.clone(),
        "test_command" => ctx.test_command.clone(),
        "output_file" => ctx.output_file.clone(),
        "next_command" => ctx.next_command.clone(),
        "failed_tests" => ctx.failed_tests.clone(),
        "context_files" => ctx.context_files.join("\n"),
        "figma_context" => ctx.figma_context.clone(),
        "task_ref" => ctx.task_ref.clone(),
        "spec_ref" => ctx.spec_ref.clone(),
        "testspec_ref" => ctx.testspec_ref.clone(),
        "plan_ref" => ctx.plan_ref.clone(),
        "verify_ref" => ctx.verify_ref.clone(),
        "figma_ref" => ctx.figma_ref.clone(),
        "patterns_ref" => ctx.patterns_ref.clone(),
        "domain_glossary_ref" => ctx.domain_glossary_ref.clone(),
        "anti_patterns_ref" => ctx.anti_patterns_ref.clone(),
        _ => format!("{{{{{}}}}}", name),
    }
}

/// Same as `get_var` but used by the `{{if}}` block check: unknown vars are
/// treated as empty (not the loud `{{name}}` string), so a stray `{{if foo}}`
/// in a template hides its block instead of rendering it spuriously.
fn get_var_for_condition(name: &str, ctx: &PromptContext) -> String {
    match name {
        "task_id" => ctx.task_id.clone(),
        "task_file" => ctx.task_file.clone(),
        "spec_file" => ctx.spec_file.clone(),
        "testspec_file" => ctx.testspec_file.clone(),
        "plan_file" => ctx.plan_file.clone(),
        "verify_file" => ctx.verify_file.clone(),
        "project_name" => ctx.project_name.clone(),
        "language" => ctx.language.clone(),
        "test_command" => ctx.test_command.clone(),
        "output_file" => ctx.output_file.clone(),
        "next_command" => ctx.next_command.clone(),
        "failed_tests" => ctx.failed_tests.clone(),
        "context_files" => ctx.context_files.join("\n"),
        "figma_context" => ctx.figma_context.clone(),
        "task_ref" => ctx.task_ref.clone(),
        "spec_ref" => ctx.spec_ref.clone(),
        "testspec_ref" => ctx.testspec_ref.clone(),
        "plan_ref" => ctx.plan_ref.clone(),
        "verify_ref" => ctx.verify_ref.clone(),
        "figma_ref" => ctx.figma_ref.clone(),
        "patterns_ref" => ctx.patterns_ref.clone(),
        "domain_glossary_ref" => ctx.domain_glossary_ref.clone(),
        "anti_patterns_ref" => ctx.anti_patterns_ref.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_ctx() -> PromptContext {
        PromptContext {
            task_id: "TASK-1".into(),
            task_file: "task content".into(),
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
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: "some content".into(),
            ..Default::default()
        };
        let tmpl = "{{if figma_context}}\nHas content: {{figma_context}}\n{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("some content"));
    }

    #[test]
    fn test_render_conditional_block_empty() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: String::new(),
            ..Default::default()
        };
        let tmpl = "{{if figma_context}}\nshould not appear\n{{end}}";
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

    // Regression: the old string-scanning renderer paired the outer `{{if}}`
    // with the first `{{end}}` it could find, dropping content after the inner
    // close tag. The single-pass parser must respect block nesting.
    #[test]
    fn nested_if_blocks_match_their_own_end() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: "outer".into(),
            spec_file: "inner".into(),
            ..Default::default()
        };
        let tmpl = "\
{{if figma_context}}OUTER-START
{{if spec_file}}INNER{{end}}
OUTER-END{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("OUTER-START"));
        assert!(result.contains("INNER"));
        assert!(result.contains("OUTER-END"));
    }

    // Regression: two `{{if}}` blocks for the same variable could leak across
    // each other when the renderer matched openings to closings in document
    // order instead of by nesting depth.
    #[test]
    fn two_adjacent_if_blocks_for_same_var() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: "yes".into(),
            ..Default::default()
        };
        let tmpl = "{{if figma_context}}first{{end}} sep {{if figma_context}}second{{end}}";
        let result = render_template(tmpl, &ctx);
        assert_eq!(result.trim(), "first sep second");
    }

    #[test]
    fn nested_block_hidden_when_outer_empty() {
        let ctx = PromptContext {
            task_id: "TASK-1".into(),
            figma_context: String::new(),
            spec_file: "inner".into(),
            ..Default::default()
        };
        let tmpl = "{{if figma_context}}{{if spec_file}}should not appear{{end}}{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(!result.contains("should not appear"));
    }

    #[test]
    fn stray_end_token_is_dropped() {
        let ctx = make_ctx();
        let tmpl = "before {{end}} after";
        let result = render_template(tmpl, &ctx);
        assert_eq!(result.trim(), "before  after");
    }

    #[test]
    fn unknown_variable_renders_as_loud_placeholder() {
        let ctx = make_ctx();
        let tmpl = "value: {{not_a_real_var}}";
        let result = render_template(tmpl, &ctx);
        assert!(
            result.contains("{{not_a_real_var}}"),
            "expected loud placeholder, got: {result:?}"
        );
    }

    #[test]
    fn unknown_if_variable_hides_block() {
        let ctx = make_ctx();
        let tmpl = "{{if not_a_real_var}}should not appear{{end}}";
        let result = render_template(tmpl, &ctx);
        assert!(!result.contains("should not appear"));
    }

    #[test]
    fn malformed_open_tag_passes_through_as_text() {
        // No closing `}}` — must not panic; must not eat the rest of the template.
        let ctx = make_ctx();
        let tmpl = "before {{task_id and then text never ends";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("before"));
        assert!(result.contains("{{task_id"));
    }

    #[test]
    fn comment_lines_are_stripped() {
        let ctx = make_ctx();
        let tmpl = "keep me\n{{/* hidden */}}\nalso me";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("keep me"));
        assert!(result.contains("also me"));
        assert!(!result.contains("hidden"));
    }

    #[test]
    fn whitespace_in_tags_is_tolerated() {
        let ctx = make_ctx();
        let tmpl = "{{ task_id }} and {{if  task_id  }}yes{{ end }}";
        let result = render_template(tmpl, &ctx);
        assert!(result.contains("TASK-1"));
        assert!(result.contains("yes"));
    }
}
