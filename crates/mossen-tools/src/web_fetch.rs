//! # web_fetch — NetRetriever 工具
//!
//! 对应 TS `WebFetchTool`（319 行）。抓取 URL 内容并处理。

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

use crate::web_fetch_tool::utils::apply_prompt_to_markdown;
use crate::web_fetch_tool::{
    cached_fetched_content, is_permitted_redirect, is_preapproved_url, record_domain_check_allowed,
    record_fetched_content, validate_url, FetchedContent, FETCH_TIMEOUT_MS,
    MAX_HTTP_CONTENT_LENGTH, MAX_MARKDOWN_LENGTH, MAX_REDIRECTS,
};

/// 网络检索器 — 获取 URL 页面内容并应用 prompt 提取。
pub struct NetRetriever;

#[derive(Debug, Clone, Deserialize)]
pub struct NetRetrieverInput {
    /// 要获取内容的 URL。
    pub url: String,
    /// 用于处理获取内容的 prompt。
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetRetrieverOutput {
    pub bytes: u64,
    pub code: u16,
    #[serde(rename = "codeText")]
    pub code_text: String,
    pub result: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "url".to_string(),
        serde_json::json!({
            "type": "string",
            "format": "uri",
            "description": "The URL to fetch content from"
        }),
    );
    properties.insert(
        "prompt".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The prompt to run on the fetched content"
        }),
    );
    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["url".to_string(), "prompt".to_string()]),
        extra: HashMap::new(),
    }
}

fn is_blocked_host(host: &str) -> bool {
    let lower = host.trim().trim_matches('.').to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return true;
    }
    match lower.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 0
        }
        Ok(IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
        Err(_) => false,
    }
}

fn html_to_text_lossy(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(MAX_MARKDOWN_LENGTH));
    let mut in_tag = false;
    let mut last_space = false;
    for ch in input.chars() {
        match ch {
            '<' => {
                in_tag = true;
                if !last_space {
                    out.push(' ');
                    last_space = true;
                }
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            ch if ch.is_whitespace() => {
                if !last_space {
                    out.push(' ');
                    last_space = true;
                }
            }
            ch => {
                out.push(ch);
                last_space = false;
            }
        }
        if out.len() >= MAX_MARKDOWN_LENGTH {
            out.truncate(MAX_MARKDOWN_LENGTH);
            break;
        }
    }
    out.trim().to_string()
}

fn body_to_markdownish(body: &str, content_type: &str) -> String {
    let mut text = if content_type.to_ascii_lowercase().contains("html") {
        html_to_text_lossy(body)
    } else {
        body.to_string()
    };
    if text.len() > MAX_MARKDOWN_LENGTH {
        text.truncate(MAX_MARKDOWN_LENGTH);
    }
    text
}

fn parse_input(input: Value) -> Result<NetRetrieverInput, String> {
    match input {
        Value::Null => {
            Err("WebFetch requires a JSON object with `url` and `prompt`; received null."
                .to_string())
        }
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("WebFetch received invalid input: {error}. Expected object: {{\"url\":\"https://...\",\"prompt\":\"...\"}}.")
        }),
        other => Err(format!(
            "WebFetch requires a JSON object with `url` and `prompt`; received {}.",
            other
        )),
    }
}

fn error_output(url: String, message: impl Into<String>, start: Instant) -> NetRetrieverOutput {
    let message = message.into();
    NetRetrieverOutput {
        bytes: 0,
        code: 0,
        code_text: "fetch_error".to_string(),
        result: message.clone(),
        duration_ms: start.elapsed().as_millis() as u64,
        url,
        error: Some(message),
    }
}

async fn fetch_content(url: &str) -> anyhow::Result<FetchedContent> {
    if let Some(content) = cached_fetched_content(url) {
        return Ok(content);
    }
    if !validate_url(url) {
        anyhow::bail!("invalid or unsupported URL");
    }

    let mut current = reqwest::Url::parse(url)?;
    if !matches!(current.scheme(), "http" | "https") {
        anyhow::bail!("WebFetch only supports http and https URLs");
    }
    let original = current.to_string();
    let host = current.host_str().unwrap_or_default().to_string();
    if is_blocked_host(&host) && !is_preapproved_url(&original) {
        anyhow::bail!("blocked local or private URL host");
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(FETCH_TIMEOUT_MS))
        .user_agent("mossen-webfetch/0.1")
        .build()?;

    for _ in 0..=MAX_REDIRECTS {
        let response = client.get(current.clone()).send().await?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("redirect missing Location header"))?;
            let next = current.join(location)?;
            if !is_permitted_redirect(current.as_str(), next.as_str()) {
                anyhow::bail!("blocked cross-origin redirect");
            }
            if let Some(host) = next.host_str() {
                if is_blocked_host(host) && !is_preapproved_url(next.as_str()) {
                    anyhow::bail!("blocked redirect to local or private host");
                }
            }
            current = next;
            continue;
        }

        if let Some(len) = response.content_length() {
            if len as usize > MAX_HTTP_CONTENT_LENGTH {
                anyhow::bail!("response too large");
            }
        }
        let code = status.as_u16();
        let code_text = status.canonical_reason().unwrap_or("").to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_HTTP_CONTENT_LENGTH {
            anyhow::bail!("response too large");
        }
        let body = String::from_utf8_lossy(&bytes).to_string();
        let content = FetchedContent {
            content: body_to_markdownish(&body, &content_type),
            bytes: bytes.len(),
            code,
            code_text,
            content_type,
            persisted_path: None,
            persisted_size: None,
        };
        if let Some(host) = current.host_str() {
            record_domain_check_allowed(host);
        }
        record_fetched_content(url, content.clone());
        return Ok(content);
    }

    anyhow::bail!("redirect limit exceeded")
}

#[async_trait]
impl Tool for NetRetriever {
    fn name(&self) -> &str {
        "WebFetch"
    }
    fn description(&self) -> &str {
        "Fetch and extract content from a URL"
    }
    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }
    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, _context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let start = Instant::now();
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => {
                let output = error_output(String::new(), message, start);
                return Ok(ToolResult {
                    output: serde_json::to_string(&output)?,
                    is_error: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    metadata: HashMap::new(),
                });
            }
        };
        if inp.url.trim().is_empty() {
            let output = error_output(
                inp.url,
                "WebFetch requires a non-empty `url` string.",
                start,
            );
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: true,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }
        let output = match fetch_content(&inp.url).await {
            Ok(content) => {
                let result = apply_prompt_to_markdown(&content.content, &inp.prompt).await;
                NetRetrieverOutput {
                    bytes: content.bytes as u64,
                    code: content.code,
                    code_text: content.code_text,
                    result,
                    duration_ms: start.elapsed().as_millis() as u64,
                    url: inp.url,
                    error: None,
                }
            }
            Err(err) => error_output(inp.url, err.to_string(), start),
        };
        let is_error = output.code == 0 || output.code >= 400;
        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::NetRetriever;
    use crate::web_fetch_tool::{clear_web_fetch_cache, record_fetched_content, FetchedContent};
    use mossen_agent::tool_registry::Tool;
    use mossen_types::ToolUseContext;
    use serde_json::Value;
    use std::collections::HashMap;

    #[tokio::test]
    async fn web_fetch_uses_cached_content_and_applies_prompt() {
        clear_web_fetch_cache();
        let url = "https://docs.rust-lang.org/book/";
        record_fetched_content(
            url,
            FetchedContent {
                content: "Rust Book cached content".to_string(),
                bytes: 24,
                code: 200,
                code_text: "OK".to_string(),
                content_type: "text/markdown".to_string(),
                persisted_path: None,
                persisted_size: None,
            },
        );
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        let result = NetRetriever
            .execute(
                serde_json::json!({
                    "url": url,
                    "prompt": "Summarize this page"
                }),
                &context,
            )
            .await
            .expect("web fetch result");
        assert!(!result.is_error);
        let output: Value = serde_json::from_str(&result.output).expect("json");
        assert_eq!(output["code"], 200);
        assert!(output["result"]
            .as_str()
            .unwrap_or_default()
            .contains("Rust Book cached content"));
    }

    #[tokio::test]
    async fn web_fetch_null_input_returns_structured_tool_error() {
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = NetRetriever
            .execute(serde_json::Value::Null, &context)
            .await
            .expect("web fetch result");
        let output: Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert_eq!(output["codeText"], "fetch_error");
        assert!(output["error"].as_str().unwrap_or_default().contains("url"));
    }

    #[tokio::test]
    async fn web_fetch_empty_url_returns_structured_tool_error() {
        let context = ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };

        let result = NetRetriever
            .execute(
                serde_json::json!({
                    "url": "",
                    "prompt": "summarize"
                }),
                &context,
            )
            .await
            .expect("web fetch result");
        let output: Value = serde_json::from_str(&result.output).expect("json");

        assert!(result.is_error);
        assert_eq!(output["codeText"], "fetch_error");
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("non-empty"));
    }
}
