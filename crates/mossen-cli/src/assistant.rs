//! Assistant 模块 — 翻译自 assistant/ 目录
//!
//! 包含：
//! - sessionDiscovery.ts → 会话发现
//! - gate.ts → 功能门控
//! - sessionHistory.ts → 会话历史获取
//! - index.ts → 助手模式管理

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

// ─── 类型定义 (sessionDiscovery.ts) ──────────────────────────────────────────

/// 助手会话描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantSession {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// 发现可恢复的助手会话。
///
/// 从 `default_transcript_dir()` 列出所有持久化的 transcript，
/// 解析 session_id / updated 时间戳并按时间倒序返回。
/// 对应 TS `discoverAssistantSessions()`。
pub async fn discover_assistant_sessions() -> Vec<AssistantSession> {
    use mossen_agent::transcript::{default_transcript_dir, list_transcripts};

    let dir = default_transcript_dir();
    let transcripts = match list_transcripts(&dir).await {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut sessions: Vec<AssistantSession> = transcripts
        .into_iter()
        .map(|t| AssistantSession {
            id: t.session_id,
            // transcript 没有显式 title 字段；使用第一条消息文本前缀作为 title 兜底
            title: t
                .messages
                .first()
                .and_then(|m| m.content.first())
                .and_then(|c| match c {
                    mossen_types::ContentBlock::Text(t) => {
                        Some(t.text.chars().take(60).collect::<String>())
                    }
                    _ => None,
                }),
            updated_at: Some(t.updated),
        })
        .collect();

    // 按 updated_at 倒序
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
}

// ─── 功能门控 (gate.ts) ─────────────────────────────────────────────────────

/// 检查 Kairos (助手) 功能是否启用。
pub async fn is_kairos_enabled() -> bool {
    true
}

// ─── 会话历史 (sessionHistory.ts) ────────────────────────────────────────────

/// 每次获取的事件上限。
pub const HISTORY_PAGE_SIZE: usize = 100;

/// 一页历史事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPage {
    /// 按时间顺序排列的事件列表。
    pub events: Vec<serde_json::Value>,
    /// 本页最老事件 ID，用作下一页的 before_id 游标。
    pub first_id: Option<String>,
    /// 是否存在更早的事件。
    pub has_more: bool,
}

/// API 返回的原始结构。
#[derive(Debug, Clone, Deserialize)]
struct SessionEventsResponse {
    data: Vec<serde_json::Value>,
    has_more: bool,
    first_id: Option<String>,
    #[allow(dead_code)]
    last_id: Option<String>,
}

/// 用于请求历史事件的认证上下文。
#[derive(Debug, Clone)]
pub struct HistoryAuthCtx {
    pub base_url: String,
    pub headers: HashMap<String, String>,
}

/// 创建可复用的认证上下文。
///
/// 准备 access_token + org UUID + beta headers，供多次分页复用。
pub async fn create_history_auth_ctx(session_id: &str) -> anyhow::Result<HistoryAuthCtx> {
    let config = mossen_utils::config::get_global_config();
    let base_api_url = std::env::var("MOSSEN_API_BASE_URL")
        .unwrap_or_else(|_| "https://api.mossen.ai".to_string());

    let mut headers = HashMap::new();
    // 从配置获取 OAuth token
    if let Some(ref account) = config.oauth_account {
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", account.account_uuid),
        );
    }
    headers.insert(
        "mossen-beta".to_string(),
        "ccr-byoc-2025-07-29".to_string(),
    );
    if let Some(ref account) = config.oauth_account {
        if let Some(ref org) = account.organization_uuid {
            headers.insert("x-organization-uuid".to_string(), org.clone());
        }
    }

    Ok(HistoryAuthCtx {
        base_url: format!("{}/v1/sessions/{}/events", base_api_url, session_id),
        headers,
    })
}

/// 内部：获取一页事件。
async fn fetch_page(
    ctx: &HistoryAuthCtx,
    params: &[(&str, String)],
    label: &str,
) -> Option<HistoryPage> {
    let client = reqwest::Client::new();
    let mut req = client.get(&ctx.base_url).timeout(std::time::Duration::from_secs(15));

    for (key, value) in &ctx.headers {
        req = req.header(key.as_str(), value.as_str());
    }
    for (key, value) in params {
        req = req.query(&[(*key, value.as_str())]);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("[{}] HTTP error: {}", label, e);
            return None;
        }
    };

    if resp.status() != reqwest::StatusCode::OK {
        tracing::debug!("[{}] HTTP {}", label, resp.status());
        return None;
    }

    let body: SessionEventsResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!("[{}] parse error: {}", label, e);
            return None;
        }
    };

    Some(HistoryPage {
        events: body.data,
        first_id: body.first_id,
        has_more: body.has_more,
    })
}

/// 获取最新事件页。
///
/// 使用 anchor_to_latest=true，获取最后 `limit` 条事件（按时间升序）。
/// has_more=true 表示存在更早的事件。
pub async fn fetch_latest_events(
    ctx: &HistoryAuthCtx,
    limit: usize,
) -> Option<HistoryPage> {
    fetch_page(
        ctx,
        &[
            ("limit", limit.to_string()),
            ("anchor_to_latest", "true".to_string()),
        ],
        "fetchLatestEvents",
    )
    .await
}

/// 获取更早的事件页。
///
/// 获取 before_id 之前的 `limit` 条事件。
pub async fn fetch_older_events(
    ctx: &HistoryAuthCtx,
    before_id: &str,
    limit: usize,
) -> Option<HistoryPage> {
    fetch_page(
        ctx,
        &[
            ("limit", limit.to_string()),
            ("before_id", before_id.to_string()),
        ],
        "fetchOlderEvents",
    )
    .await
}

// ─── 助手模式管理 (index.ts) ────────────────────────────────────────────────

/// 全局标记：是否强制进入助手模式。
static ASSISTANT_FORCED: AtomicBool = AtomicBool::new(false);

/// 标记强制进入助手模式。
pub fn mark_assistant_forced() {
    ASSISTANT_FORCED.store(true, Ordering::Relaxed);
}

/// 查询是否被强制进入助手模式。
pub fn is_assistant_forced() -> bool {
    ASSISTANT_FORCED.load(Ordering::Relaxed)
}

/// 查询当前是否处于助手模式。
///
/// 如果 `ASSISTANT_FORCED` 或环境变量 `MOSSENSRC_ASSISTANT_MODE=1`。
pub fn is_assistant_mode() -> bool {
    if is_assistant_forced() {
        return true;
    }
    std::env::var("MOSSENSRC_ASSISTANT_MODE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// 初始化助手团队（当前为空操作）。
pub async fn initialize_assistant_team() {
    // no-op — 与 TS 一致
}

/// 获取助手模式的系统提示词附录。
pub fn get_assistant_system_prompt_addendum() -> &'static str {
    "\n# Assistant Mode\n\nYou are running in assistant mode. \
     Prefer autonomous progress, preserve context between wake-ups, \
     and use user-facing messaging tools when communicating outward."
}
