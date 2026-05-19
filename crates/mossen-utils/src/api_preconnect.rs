//! # api_preconnect — API 预连接
//!
//! 对应 TypeScript `utils/apiPreconnect.ts`。
//! 预连接到 Mossen API 以重叠 TCP+TLS 握手与启动流程。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static FIRED: AtomicBool = AtomicBool::new(false);

/// 预连接到 Mossen API。
///
/// TCP+TLS 握手通常约 100-200ms，会阻塞在首次 API 调用中。
/// 在 init 阶段触发一个 fire-and-forget 的 fetch，让握手与
/// action-handler 工作并行进行。
///
/// 跳过条件：
/// - 配置了代理/mTLS/unix socket（预连接会使用错误的传输层）
/// - Bedrock/Vertex/Foundry（不同端点、不同认证）
pub fn preconnect_mossen_api() {
    if FIRED.swap(true, Ordering::SeqCst) {
        return;
    }

    // 使用 Bedrock 时跳过——不同的端点和认证
    if is_env_truthy("MOSSEN_CODE_USE_BEDROCK")
        || is_env_truthy("MOSSEN_CODE_USE_VERTEX")
        || is_env_truthy("MOSSEN_CODE_USE_FOUNDRY")
    {
        return;
    }

    // 代理/mTLS/unix 时跳过——SDK 的自定义 dispatcher 不会复用此连接池
    if std::env::var("HTTPS_PROXY").is_ok()
        || std::env::var("https_proxy").is_ok()
        || std::env::var("HTTP_PROXY").is_ok()
        || std::env::var("http_proxy").is_ok()
        || std::env::var("MOSSEN_CODE_UNIX_SOCKET").is_ok()
        || std::env::var("MOSSEN_CODE_CLIENT_CERT").is_ok()
        || std::env::var("MOSSEN_CODE_CLIENT_KEY").is_ok()
    {
        return;
    }

    // 仅在配置了显式的 Mossen adapter/custom gateway 时预连接
    let base_url = if is_custom_backend_enabled() {
        get_custom_backend_base_url()
    } else {
        std::env::var("MOSSEN_CODE_API_BASE_URL").ok()
    };

    let base_url = match base_url {
        Some(url) if !url.is_empty() => url,
        _ => return,
    };

    // Fire and forget。HEAD 表示无响应体——连接在 headers 到达后
    // 立即可被 keep-alive 连接池复用。10s 超时防止慢网络挂住进程。
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let _ = client
            .head(&base_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;
    });
}

/// 检查环境变量是否为真值
fn is_env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 检查是否启用了自定义后端
fn is_custom_backend_enabled() -> bool {
    std::env::var("MOSSEN_CODE_CUSTOM_BACKEND")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 获取自定义后端基础 URL
fn get_custom_backend_base_url() -> Option<String> {
    std::env::var("MOSSEN_CODE_API_BASE_URL").ok()
}
