//! # transcript — Transcript 持久化
//!
//! 对应 TS `sessionStorage.ts` 中的 `recordTranscript` 等，
//! 负责将消息历史写入 JSON 文件并支持增量更新。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use mossen_types::Message;

// ---------------------------------------------------------------------------
// Transcript 文件格式
// ---------------------------------------------------------------------------

/// Transcript 文件内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptFile {
    /// 会话 ID。
    pub session_id: String,
    /// 消息列表。
    pub messages: Vec<Message>,
    /// 消息数量。
    pub message_count: usize,
    /// 创建时间。
    pub created: String,
    /// 最后更新时间。
    pub updated: String,
    /// 模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 当前工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

// ---------------------------------------------------------------------------
// Transcript 管理器
// ---------------------------------------------------------------------------

/// Transcript 持久化管理器。
pub struct TranscriptManager {
    /// 会话 ID。
    session_id: String,
    /// 存储目录。
    storage_dir: PathBuf,
    /// 已写入的消息前缀长度（用于增量更新）。
    written_prefix_len: usize,
}

impl TranscriptManager {
    /// 创建新的 Transcript 管理器。
    pub fn new(session_id: String, storage_dir: PathBuf) -> Self {
        Self {
            session_id,
            storage_dir,
            written_prefix_len: 0,
        }
    }

    /// 获取 transcript 文件路径。
    pub fn file_path(&self) -> PathBuf {
        self.storage_dir
            .join(&self.session_id)
            .with_extension("json")
    }

    /// 记录 transcript——增量写入。
    ///
    /// 对应 TS `recordTranscript()`，使用 prefix-tracking 保证去重。
    pub async fn record(
        &mut self,
        messages: &[Message],
        model: Option<&str>,
        cwd: Option<&str>,
    ) -> anyhow::Result<()> {
        // 只有新消息时才写入
        if messages.len() <= self.written_prefix_len {
            return Ok(());
        }

        let file_path = self.file_path();

        // 确保目录存在
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        let transcript = TranscriptFile {
            session_id: self.session_id.clone(),
            messages: messages.to_vec(),
            message_count: messages.len(),
            created: if self.written_prefix_len == 0 {
                now.clone()
            } else {
                // 保留原始创建时间——如果文件已存在则读取
                self.read_created_time(&file_path)
                    .await
                    .unwrap_or(now.clone())
            },
            updated: now,
            model: model.map(|s| s.to_string()),
            cwd: cwd.map(|s| s.to_string()),
        };

        let json = serde_json::to_string_pretty(&transcript)?;
        tokio::fs::write(&file_path, json).await?;

        self.written_prefix_len = messages.len();
        debug!(
            session_id = %self.session_id,
            message_count = messages.len(),
            "Transcript recorded"
        );

        Ok(())
    }

    /// 从文件中读取已有的创建时间。
    async fn read_created_time(&self, path: &Path) -> Option<String> {
        let content = tokio::fs::read_to_string(path).await.ok()?;
        let transcript: TranscriptFile = serde_json::from_str(&content).ok()?;
        Some(transcript.created)
    }

    /// 加载已有的 transcript。
    pub async fn load(&self) -> anyhow::Result<Option<TranscriptFile>> {
        let path = self.file_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path).await?;
        let transcript: TranscriptFile = serde_json::from_str(&content)?;
        Ok(Some(transcript))
    }

    /// 删除 transcript 文件。
    pub async fn delete(&self) -> anyhow::Result<()> {
        let path = self.file_path();
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 获取默认的 transcript 存储目录。
pub fn default_transcript_dir() -> PathBuf {
    // 使用 XDG 或平台标准目录
    let home = dirs_fallback();
    home.join(".mossen").join("transcripts")
}

/// 获取用户主目录的回退实现。
fn dirs_fallback() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// 列出所有 transcript 文件。
pub async fn list_transcripts(storage_dir: &Path) -> anyhow::Result<Vec<TranscriptFile>> {
    let mut transcripts = Vec::new();

    if !storage_dir.exists() {
        return Ok(transcripts);
    }

    let mut entries = tokio::fs::read_dir(storage_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    if let Ok(transcript) = serde_json::from_str::<TranscriptFile>(&content) {
                        transcripts.push(transcript);
                    }
                }
                Err(e) => {
                    error!(path = %path.display(), error = %e, "Failed to read transcript");
                }
            }
        }
    }

    // 按更新时间排序（最新的在前）
    transcripts.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(transcripts)
}
