use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;
use serde_json::Value;

pub struct JiraTicket {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub components: Vec<String>,
}

pub fn extract_key_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"/browse/([A-Z][A-Z0-9]*-[0-9]+)").ok()?;
    re.captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_base_url(jira_url: &str) -> Option<String> {
    let re = Regex::new(r"(https?://[^/]+)").ok()?;
    re.captures(jira_url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn fetch_ticket(url: &str) -> Result<JiraTicket> {
    let key = extract_key_from_url(url).ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot extract Jira issue key from '{}'\n\
             Expected: https://COMPANY.atlassian.net/browse/PROJ-123",
            url
        )
    })?;

    let base = extract_base_url(url).ok_or_else(|| anyhow::anyhow!("Invalid URL: {}", url))?;

    let email =
        std::env::var("JIRA_EMAIL").context("JIRA_EMAIL not set — required for Jira import")?;
    let token = std::env::var("JIRA_API_TOKEN")
        .context("JIRA_API_TOKEN not set — required for Jira import")?;

    let auth = STANDARD.encode(format!("{email}:{token}"));
    let api_url = format!("{base}/rest/api/3/issue/{key}");

    let resp: Value = ureq::get(&api_url)
        .set("Authorization", &format!("Basic {auth}"))
        .set("Accept", "application/json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(401, _) => {
                anyhow::anyhow!("Jira auth failed — check JIRA_EMAIL and JIRA_API_TOKEN")
            }
            ureq::Error::Status(404, _) => {
                anyhow::anyhow!("Jira issue {} not found", key)
            }
            ureq::Error::Status(code, _) => anyhow::anyhow!("Jira returned HTTP {}", code),
            ureq::Error::Transport(t) => anyhow::anyhow!("Network error: {}", t),
        })?
        .into_json()
        .context("Failed to parse Jira API response")?;

    let fields = &resp["fields"];

    let summary = fields["summary"].as_str().unwrap_or(&key).to_string();

    let description = extract_description_text(&fields["description"]);

    let components: Vec<String> = fields["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok(JiraTicket {
        key,
        summary,
        description,
        components,
    })
}

fn extract_description_text(node: &Value) -> Option<String> {
    if node.is_null() {
        return None;
    }
    let mut buf = String::new();
    adf_to_text(node, &mut buf, 0);
    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn adf_to_text(node: &Value, buf: &mut String, depth: usize) {
    match node["type"].as_str().unwrap_or("") {
        "text" => {
            if let Some(t) = node["text"].as_str() {
                buf.push_str(t);
            }
        }
        "hardBreak" => buf.push('\n'),
        "paragraph" => {
            walk_content(node, buf, depth);
            buf.push('\n');
        }
        "heading" => {
            walk_content(node, buf, depth);
            buf.push('\n');
        }
        "listItem" => {
            buf.push_str(&"  ".repeat(depth));
            buf.push_str("- ");
            walk_content(node, buf, depth + 1);
        }
        _ => walk_content(node, buf, depth),
    }
}

fn walk_content(node: &Value, buf: &mut String, depth: usize) {
    if let Some(content) = node["content"].as_array() {
        for child in content {
            adf_to_text(child, buf, depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_key_from_browse_url() {
        let url = "https://company.atlassian.net/browse/PROJ-123";
        assert_eq!(extract_key_from_url(url), Some("PROJ-123".to_string()));
    }

    #[test]
    fn extracts_key_with_numbers_in_project() {
        let url = "https://company.atlassian.net/browse/ABC2-45";
        assert_eq!(extract_key_from_url(url), Some("ABC2-45".to_string()));
    }

    #[test]
    fn returns_none_for_non_browse_url() {
        assert!(extract_key_from_url("https://company.atlassian.net/issues").is_none());
    }

    #[test]
    fn extracts_base_url() {
        let url = "https://company.atlassian.net/browse/PROJ-1";
        assert_eq!(
            extract_base_url(url),
            Some("https://company.atlassian.net".to_string())
        );
    }

    #[test]
    fn adf_paragraph_to_text() {
        let node = serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "Hello world"}]
            }]
        });
        let result = extract_description_text(&node);
        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[test]
    fn null_description_returns_none() {
        assert!(extract_description_text(&Value::Null).is_none());
    }
}
