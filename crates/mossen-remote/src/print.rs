//! # print — 格式化打印工具
//!
//! 提供 SDK 消息的格式化输出和出站消息排序。
//! 对应 TS `cli/print.ts` 中的出站消息处理部分。

use crate::ndjson::ndjson_safe_line;
use crate::transport::StdoutMessage;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tracing;

/// 向 stdout 写入 NDJSON 行。
///
/// 等价于 TS 中的 `writeToStdout(ndjsonSafeStringify(msg) + '\n')`。
pub async fn write_to_stdout(message: &StdoutMessage) -> anyhow::Result<()> {
    let line = ndjson_safe_line(message)?;
    let mut stdout = tokio::io::stdout();
    stdout.write_all(line.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

/// 同步向 stdout 写入 NDJSON 行。
///
/// 用于不在 async 上下文中的输出。
pub fn write_to_stdout_sync(message: &StdoutMessage) -> anyhow::Result<()> {
    use std::io::Write;
    let line = ndjson_safe_line(message)?;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(line.as_bytes())?;
    handle.flush()?;
    Ok(())
}

/// 出站消息排水循环。
///
/// 从出站队列中取出消息并写入传输层或 stdout。
pub async fn drain_outbound(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<StdoutMessage>,
    writer: impl Fn(StdoutMessage) -> futures::future::BoxFuture<'static, anyhow::Result<()>>,
) {
    while let Some(message) = rx.recv().await {
        if let Err(e) = writer(message).await {
            tracing::error!("drain_outbound: write error: {}", e);
        }
    }
}

/// 构建 SDK 状态消息。
pub fn build_status_message(status: &str, session_id: &str) -> Value {
    serde_json::json!({
        "type": "status",
        "status": status,
        "session_id": session_id,
    })
}

/// 构建 keep-alive 消息。
pub fn build_keepalive_message() -> Value {
    serde_json::json!({
        "type": "keep_alive",
    })
}

/// 构建流式文本消息。
pub fn build_streamlined_text_message(text: &str, session_id: &str) -> Value {
    serde_json::json!({
        "type": "streamlined_text",
        "text": text,
        "session_id": session_id,
    })
}

/// 构建流式工具使用摘要消息。
pub fn build_streamlined_tool_use_summary(
    tool_name: &str,
    summary: &str,
    session_id: &str,
) -> Value {
    serde_json::json!({
        "type": "streamlined_tool_use_summary",
        "tool_name": tool_name,
        "summary": summary,
        "session_id": session_id,
    })
}

/// 构建错误消息。
pub fn build_error_message(error: &str, session_id: &str) -> Value {
    serde_json::json!({
        "type": "error",
        "error": error,
        "session_id": session_id,
    })
}
