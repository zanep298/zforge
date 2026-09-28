//! Remove keys from a block-style YAML file as text, so the user's comments
//! and layout survive. Callers parse the result and fall back to
//! re-serializing when a layout (flow style) is beyond these edits.

/// Whether `line` is `key:` at column `indent` (value, comment or nothing
/// after the colon).
fn is_key_at(line: &str, key: &str, indent: usize) -> bool {
    let lead = line.len() - line.trim_start().len();
    lead == indent
        && line[lead..]
            .strip_prefix(key)
            .and_then(|r| r.strip_prefix(':'))
            .is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t']))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// End (exclusive) of the block that starts at `start`: every following
/// line indented deeper than `start`'s, and the blank or comment lines
/// between them. Trailing blank and comment lines stay outside the block.
fn block_end(lines: &[&str], start: usize) -> usize {
    let own = indent_of(lines[start]);
    let mut end = start + 1;
    let mut i = start + 1;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() || t.starts_with('#') {
            i += 1;
            continue;
        }
        if indent_of(lines[i]) <= own {
            break;
        }
        i += 1;
        end = i;
    }
    end
}

fn join(lines: &[&str]) -> String {
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Remove the top-level `key` and its block. `None` when it is not there.
pub fn remove_top_key(text: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| is_key_at(l, key, 0))?;
    let end = block_end(&lines, start);
    let kept: Vec<&str> = lines[..start]
        .iter()
        .chain(&lines[end..])
        .copied()
        .collect();
    Some(join(&kept))
}

/// Remove `key` inside the top-level `section` (its direct child) and the
/// key's own block. When nothing is left in the section, the section goes
/// too. `None` when the key is not there.
pub fn remove_nested_key(text: &str, section: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let head = lines.iter().position(|l| is_key_at(l, section, 0))?;
    let sec_end = block_end(&lines, head);
    let child_indent = lines[head + 1..sec_end]
        .iter()
        .find(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(|l| indent_of(l))?;
    let start = (head + 1..sec_end).find(|&i| is_key_at(lines[i], key, child_indent))?;
    let end = block_end(&lines, start);
    let kept: Vec<&str> = lines[..start]
        .iter()
        .chain(&lines[end..])
        .copied()
        .collect();
    let out = join(&kept);
    let still_has_child = {
        let ls: Vec<&str> = out.lines().collect();
        let h = ls.iter().position(|l| is_key_at(l, section, 0))?;
        let e = block_end(&ls, h);
        ls[h + 1..e]
            .iter()
            .any(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
    };
    if still_has_child {
        Some(out)
    } else {
        remove_top_key(&out, section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "# mine\nproject:\n  name: app # the app\n  root_dir: \".\"\n\
                          opencode:\n  model: x\n  context_files: []\n\n\
                          paths:\n  tasks: ./t\n  agents: ./a # agents\n  memory: ./m\n\
                          review:\n  auto_approve: false\n# end\n";

    #[test]
    fn top_level_blocks_go_with_their_children_and_comments_stay() {
        let out = remove_top_key(CONFIG, "opencode").unwrap();
        let out = remove_top_key(&out, "review").unwrap();
        assert_eq!(
            out,
            "# mine\nproject:\n  name: app # the app\n  root_dir: \".\"\n\n\
             paths:\n  tasks: ./t\n  agents: ./a # agents\n  memory: ./m\n# end\n"
        );
        assert_eq!(remove_top_key(&out, "opencode"), None);
    }

    #[test]
    fn nested_keys_go_and_their_siblings_stay() {
        let out = remove_nested_key(CONFIG, "paths", "tasks").unwrap();
        let out = remove_nested_key(&out, "paths", "memory").unwrap();
        let out = remove_nested_key(&out, "project", "root_dir").unwrap();
        assert!(
            out.contains("paths:\n  agents: ./a # agents\nreview:"),
            "{out}"
        );
        assert!(
            out.contains("project:\n  name: app # the app\nopencode:"),
            "{out}"
        );
        assert_eq!(remove_nested_key(&out, "paths", "tasks"), None);
    }

    #[test]
    fn an_emptied_section_goes_too() {
        let out = remove_nested_key(
            "review:\n  auto_approve: true\nx: 1\n",
            "review",
            "auto_approve",
        )
        .unwrap();
        assert_eq!(out, "x: 1\n");
    }

    #[test]
    fn a_key_that_only_starts_the_same_is_left_alone() {
        assert_eq!(remove_top_key("reviewer: me\n", "review"), None);
    }
}
