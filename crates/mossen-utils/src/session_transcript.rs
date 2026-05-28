//! # session_transcript — 会话 Transcript 持久化
//!
//! 对应 TS `services/sessionTranscript/sessionTranscript.ts`。
//! 按日期分桶写入 JSONL 文件。

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use serde::Serialize;
use tokio::fs;

use crate::env::get_mossen_config_home_dir;

// ---------------------------------------------------------------------------
// 路径辅助
// ---------------------------------------------------------------------------

/// 获取 transcript 目录。
fn get_transcript_dir() -> PathBuf {
    get_mossen_config_home_dir().join("session-transcripts")
}

/// 获取按日期命名的 transcript 文件路径。
fn get_daily_transcript_path(date: &str) -> PathBuf {
    get_transcript_dir().join(format!("{}.jsonl", date))
}

// ---------------------------------------------------------------------------
// Transcript 条目
// ---------------------------------------------------------------------------

/// Transcript 条目（写入 JSONL 的单行）。
#[derive(Debug, Serialize)]
struct TranscriptEntry<'a> {
    timestamp: Option<&'a str>,
    #[serde(rename = "type")]
    msg_type: &'a str,
    message: &'a serde_json::Value,
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 将消息片段按日期分桶写入 transcript 文件。
///
/// 对应 TS `writeSessionTranscriptSegment()`。
/// 每条消息必须有 `timestamp` 字段（ISO 8601 格式），
/// 前 10 个字符为日期（YYYY-MM-DD）。
pub async fn write_session_transcript_segment(
    messages: &[serde_json::Value],
) -> anyhow::Result<()> {
    let mut buckets: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();

    for msg in messages {
        if let Some(date) = extract_message_date(msg) {
            buckets.entry(date).or_default().push(msg);
        }
    }

    for (date, bucket) in &buckets {
        append_segment(date, bucket).await?;
    }

    Ok(())
}

/// 在日期变更时刷新先前日期的消息。
///
/// 对应 TS `flushOnDateChange()`。
pub async fn flush_on_date_change(
    messages: &[serde_json::Value],
    current_date: &str,
) -> anyhow::Result<()> {
    let prior: Vec<&serde_json::Value> = messages
        .iter()
        .filter(|m| {
            extract_message_date(m)
                .map(|d| d.as_str() < current_date)
                .unwrap_or(false)
        })
        .collect();

    if prior.is_empty() {
        return Ok(());
    }

    let mut buckets: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    for msg in prior {
        if let Some(date) = extract_message_date(msg) {
            buckets.entry(date).or_default().push(msg);
        }
    }

    for (date, bucket) in &buckets {
        append_segment(date, bucket).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

/// 从消息中提取日期（YYYY-MM-DD）。
fn extract_message_date(msg: &serde_json::Value) -> Option<String> {
    let ts = msg.get("timestamp")?.as_str()?;
    if ts.len() >= 10 {
        let date_part = &ts[..10];
        // 简单验证格式
        if NaiveDate::parse_from_str(date_part, "%Y-%m-%d").is_ok() {
            return Some(date_part.to_string());
        }
    }
    None
}

/// 追加消息到日期对应的 transcript 文件。
async fn append_segment(date: &str, messages: &[&serde_json::Value]) -> anyhow::Result<()> {
    if messages.is_empty() {
        return Ok(());
    }

    let dir = get_transcript_dir();
    fs::create_dir_all(&dir).await?;

    let mut payload = String::new();
    for msg in messages {
        let entry = TranscriptEntry {
            timestamp: msg.get("timestamp").and_then(|v| v.as_str()),
            msg_type: msg
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
            message: msg,
        };
        if let Ok(line) = serde_json::to_string(&entry) {
            payload.push_str(&line);
            payload.push('\n');
        }
    }

    let path = get_daily_transcript_path(date);
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    file.write_all(payload.as_bytes()).await?;

    Ok(())
}
