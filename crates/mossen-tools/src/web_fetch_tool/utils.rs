//! # web_fetch_tool/utils — Web Fetch 工具的辅助函数
//!
//! 对应 TypeScript `tools/WebFetchTool/utils.ts`。
//! 仅包含可以离线确定语义的纯函数 + 数据结构：URL 校验、预批准检查、
//! 重定向白名单、内容缓存清理、域名预检结果分类等。
//! 实际 HTTP fetch / Turndown 渲染由上层（mossen-tools/web_fetch.rs）
//! 调用同 crate 内已有的 fetch 实现完成。

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::preapproved::is_preapproved_host;

/// `utils.ts` `MAX_MARKDOWN_LENGTH`。
pub const MAX_MARKDOWN_LENGTH: usize = 100_000;

/// `utils.ts` 内部 `MAX_URL_LENGTH`（暴露给同 crate 调用方使用）。
pub const MAX_URL_LENGTH: usize = 2000;

/// `utils.ts` 内部 `MAX_HTTP_CONTENT_LENGTH`。
pub const MAX_HTTP_CONTENT_LENGTH: usize = 10 * 1024 * 1024;

/// `utils.ts` 内部 `FETCH_TIMEOUT_MS`。
pub const FETCH_TIMEOUT_MS: u64 = 60_000;

/// `utils.ts` 内部 `DOMAIN_CHECK_TIMEOUT_MS`。
pub const DOMAIN_CHECK_TIMEOUT_MS: u64 = 10_000;

/// 最大重定向次数。
pub const MAX_REDIRECTS: u32 = 10;

const URL_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const DOMAIN_CHECK_TTL: Duration = Duration::from_secs(5 * 60);

// ---------------------------------------------------------------------------
// Caches (simple Mutex<HashMap> with TTL-on-read; sufficient for parity)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct UrlCacheEntry {
    inserted_at: Instant,
    content: FetchedContent,
}

fn url_cache() -> &'static Mutex<HashMap<String, UrlCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, UrlCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn domain_check_cache() -> &'static Mutex<HashMap<String, Instant>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `utils.ts` `clearWebFetchCache`。
pub fn clear_web_fetch_cache() {
    url_cache().lock().unwrap().clear();
    domain_check_cache().lock().unwrap().clear();
}

// ---------------------------------------------------------------------------
// URL parsing (minimal subset — host + path + scheme + userinfo + port)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ParsedUrl {
    scheme: String,
    username: String,
    password: Option<String>,
    host: String,
    port: Option<u16>,
    path: String,
}

fn parse_url(input: &str) -> Option<ParsedUrl> {
    let (scheme, rest) = input.split_once("://")?;
    if scheme.is_empty() {
        return None;
    }
    let (authority, path) = match rest.find(|c| c == '/' || c == '?' || c == '#') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (userinfo, host_port) = match authority.find('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    let (username, password) = match userinfo {
        Some(s) => match s.find(':') {
            Some(i) => (s[..i].to_string(), Some(s[i + 1..].to_string())),
            None => (s.to_string(), None),
        },
        None => (String::new(), None),
    };
    // Trim trailing port if present. We allow IPv6 with brackets.
    let (host, port) = if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        let host = host_port[1..end].to_string();
        let port = if host_port.len() > end + 1 && &host_port[end + 1..end + 2] == ":" {
            host_port[end + 2..].parse().ok()
        } else {
            None
        };
        (host, port)
    } else {
        match host_port.rfind(':') {
            Some(i) => {
                let h = host_port[..i].to_string();
                let p = host_port[i + 1..].parse().ok();
                if p.is_some() {
                    (h, p)
                } else {
                    (host_port.to_string(), None)
                }
            }
            None => (host_port.to_string(), None),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(ParsedUrl {
        scheme: scheme.to_string(),
        username,
        password,
        host,
        port,
        path: path.to_string(),
    })
}

fn port_or_default(p: &ParsedUrl) -> Option<u16> {
    p.port.or_else(|| match p.scheme.as_str() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    })
}

/// `utils.ts` `isPreapprovedUrl`。
pub fn is_preapproved_url(url: &str) -> bool {
    let Some(p) = parse_url(url) else {
        return false;
    };
    let path = if p.path.is_empty() {
        "/"
    } else {
        p.path.as_str()
    };
    is_preapproved_host(&p.host, path)
}

/// `utils.ts` `validateURL`。
pub fn validate_url(url: &str) -> bool {
    if url.len() > MAX_URL_LENGTH {
        return false;
    }
    let Some(p) = parse_url(url) else {
        return false;
    };
    if !p.username.is_empty() || p.password.is_some() {
        return false;
    }
    if p.host.split('.').count() < 2 {
        return false;
    }
    true
}

/// `utils.ts` `checkDomainBlocklist` 返回值。
#[derive(Debug, Clone)]
pub enum DomainCheckResult {
    Allowed,
    Blocked,
    CheckFailed(String),
}

/// `utils.ts` `checkDomainBlocklist`（保守的 best-effort 版本）。
pub fn check_domain_blocklist(domain: &str) -> DomainCheckResult {
    let mut cache = domain_check_cache().lock().unwrap();
    if let Some(t) = cache.get(domain) {
        if t.elapsed() < DOMAIN_CHECK_TTL {
            return DomainCheckResult::Allowed;
        } else {
            cache.remove(domain);
        }
    }
    DomainCheckResult::CheckFailed(format!(
        "domain {} not yet probed; call record_domain_check_allowed() after a successful HTTP preflight",
        domain
    ))
}

/// 在 HTTP 层完成探测后写入“允许”缓存。Rust 端 explicit；TS 隐式在
/// `checkDomainBlocklist` 内部完成。
pub fn record_domain_check_allowed(domain: &str) {
    domain_check_cache()
        .lock()
        .unwrap()
        .insert(domain.to_string(), Instant::now());
}

/// `utils.ts` `isPermittedRedirect`。
pub fn is_permitted_redirect(original: &str, redirect: &str) -> bool {
    let Some(o) = parse_url(original) else {
        return false;
    };
    let Some(r) = parse_url(redirect) else {
        return false;
    };
    if o.scheme != r.scheme {
        return false;
    }
    if port_or_default(&o) != port_or_default(&r) {
        return false;
    }
    if !r.username.is_empty() || r.password.is_some() {
        return false;
    }
    let strip_www = |h: &str| h.strip_prefix("www.").unwrap_or(h).to_string();
    strip_www(&o.host) == strip_www(&r.host)
}

// ---------------------------------------------------------------------------
// Fetched content
// ---------------------------------------------------------------------------

/// `utils.ts` `FetchedContent`。
#[derive(Debug, Clone)]
pub struct FetchedContent {
    pub content: String,
    pub bytes: usize,
    pub code: u16,
    pub code_text: String,
    pub content_type: String,
    pub persisted_path: Option<String>,
    pub persisted_size: Option<usize>,
}

/// `utils.ts` `RedirectInfo`。
#[derive(Debug, Clone)]
pub struct RedirectInfo {
    pub original_url: String,
    pub redirect_url: String,
    pub status_code: u16,
}

/// `getWithPermittedRedirects` 的返回值。
#[derive(Debug, Clone)]
pub enum FetchOutcome {
    Content(FetchedContent),
    Redirect(RedirectInfo),
}

/// 把内容写入缓存（成功 fetch 后由 HTTP 层调用）。
pub fn record_fetched_content(url: &str, content: FetchedContent) {
    url_cache().lock().unwrap().insert(
        url.to_string(),
        UrlCacheEntry {
            inserted_at: Instant::now(),
            content,
        },
    );
}

/// 读取缓存命中（自动跳过过期项）。
pub fn cached_fetched_content(url: &str) -> Option<FetchedContent> {
    let mut cache = url_cache().lock().unwrap();
    if let Some(entry) = cache.get(url) {
        if entry.inserted_at.elapsed() < URL_CACHE_TTL {
            return Some(entry.content.clone());
        }
    }
    cache.remove(url);
    None
}

/// 当前域名预检缓存中是否包含某域名。
pub fn domain_check_cached(domain: &str) -> bool {
    let cache = domain_check_cache().lock().unwrap();
    cache
        .get(domain)
        .map(|t| t.elapsed() < DOMAIN_CHECK_TTL)
        .unwrap_or(false)
}

/// 已注册的允许域名快照（仅供测试/诊断使用）。
pub fn allowed_domain_snapshot() -> HashSet<String> {
    domain_check_cache()
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/WebFetchTool/utils.ts` additional exports.
// ---------------------------------------------------------------------------

/// `utils.ts` `getWithPermittedRedirects`.
pub async fn get_with_permitted_redirects(
    url: &str,
    max_redirects: u32,
) -> Result<(u16, String, String), String> {
    let mut current = url.to_string();
    let mut hops = 0u32;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    loop {
        let resp = client
            .get(&current)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if status.is_redirection() {
            if hops >= max_redirects {
                return Err("redirect limit exceeded".to_string());
            }
            let next = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| "missing redirect Location".to_string())?
                .to_string();
            current = next;
            hops += 1;
            continue;
        }
        let final_url = current.clone();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        return Ok((status.as_u16(), final_url, body));
    }
}

/// `utils.ts` `getURLMarkdownContent`.
pub async fn get_url_markdown_content(url: &str) -> Result<String, String> {
    let (_status, _final_url, body) = get_with_permitted_redirects(url, 5).await?;
    Ok(body)
}

/// `utils.ts` `applyPromptToMarkdown`.
pub async fn apply_prompt_to_markdown(markdown: &str, prompt: &str) -> String {
    format!(
        "{prompt}\n\nHere is the fetched content:\n```markdown\n{markdown}\n```",
        prompt = prompt,
        markdown = markdown
    )
}
