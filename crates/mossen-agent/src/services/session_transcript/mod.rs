//! Session transcript — append messages to daily JSONL files.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

use regex::Regex;

/// A generic message with type and timestamp.
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub timestamp: Option<String>,
    pub content: Value,
}

fn get_transcript_dir() -> PathBuf {
    let config_home = std::env::var("MOSSEN_CONFIG_DIR").unwrap_or_else(|_| {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".mossen").to_string_lossy().to_string()
    });
    PathBuf::from(config_home).join("session-transcripts")
}

fn get_daily_transcript_path(date: &str) -> PathBuf {
    get_transcript_dir().join(format!("{}.jsonl", date))
}

fn get_message_date(message: &TranscriptMessage) -> Option<String> {
    let ts = message.timestamp.as_ref()?;
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap();
    let caps = re.find(ts)?;
    Some(caps.as_str().to_string())
}

async fn append_segment(date: &str, messages: &[TranscriptMessage]) -> Result<(), std::io::Error> {
    if messages.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(get_transcript_dir()).await?;
    let mut payload = String::new();
    for message in messages {
        let entry = serde_json::json!({
            "timestamp": message.timestamp,
            "type": message.msg_type,
            "message": message.content,
        });
        payload.push_str(&serde_json::to_string(&entry).unwrap_or_default());
        payload.push('\n');
    }
    let path = get_daily_transcript_path(date);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    use tokio::io::AsyncWriteExt;
    file.write_all(payload.as_bytes()).await?;
    Ok(())
}

/// Write a segment of messages to their respective daily transcript files.
pub async fn write_session_transcript_segment(
    messages: &[TranscriptMessage],
) -> Result<(), std::io::Error> {
    let mut buckets: HashMap<String, Vec<&TranscriptMessage>> = HashMap::new();
    for message in messages {
        if let Some(date) = get_message_date(message) {
            buckets.entry(date).or_default().push(message);
        }
    }
    for (date, bucket) in &buckets {
        let owned: Vec<TranscriptMessage> = bucket.iter().map(|m| (*m).clone()).collect();
        append_segment(date, &owned).await?;
    }
    Ok(())
}

/// Flush messages that belong to dates before `current_date`.
pub async fn flush_on_date_change(
    messages: &[TranscriptMessage],
    current_date: &str,
) -> Result<(), std::io::Error> {
    let prior: Vec<TranscriptMessage> = messages
        .iter()
        .filter(|m| {
            if let Some(date) = get_message_date(m) {
                date.as_str() < current_date
            } else {
                false
            }
        })
        .cloned()
        .collect();
    write_session_transcript_segment(&prior).await
}
