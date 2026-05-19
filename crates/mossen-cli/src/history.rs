//! 命令行历史 — 对应 TS 的 history.ts。
//!
//! 管理用户输入历史的持久化存储（JSONL 格式），支持粘贴内容引用、
//! 反向搜索和会话级 undo。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tracing::warn;

const MAX_HISTORY_ITEMS: usize = 100;
const MAX_PASTED_CONTENT_LENGTH: usize = 1024;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// 粘贴内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedContent {
    pub id: u32,
    #[serde(rename = "type")]
    pub content_type: String, // "text" | "image"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// 存储的粘贴内容（可含 hash 引用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPastedContent {
    id: u32,
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

/// 历史条目（用户可见）。
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub display: String,
    pub pasted_contents: HashMap<u32, PastedContent>,
}

/// 带时间戳的历史条目（用于 ctrl+r 选择器）。
#[derive(Debug, Clone)]
pub struct TimestampedHistoryEntry {
    pub display: String,
    pub timestamp: u64,
}

/// 日志条目（磁盘格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    display: String,
    #[serde(rename = "pastedContents")]
    pasted_contents: HashMap<u32, StoredPastedContent>,
    timestamp: u64,
    project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Reference parsing
// ---------------------------------------------------------------------------

/// 计算粘贴文本的换行数。
pub fn get_pasted_text_ref_num_lines(text: &str) -> usize {
    text.matches('\n').count() + text.matches('\r').count()
        - text.matches("\r\n").count()
}

/// 格式化粘贴文本引用标记。
pub fn format_pasted_text_ref(id: u32, num_lines: usize) -> String {
    if num_lines == 0 {
        format!("[Pasted text #{}]", id)
    } else {
        format!("[Pasted text #{} +{} lines]", id, num_lines)
    }
}

/// 格式化图片引用标记。
pub fn format_image_ref(id: u32) -> String {
    format!("[Image #{}]", id)
}

/// 解析的引用。
#[derive(Debug, Clone)]
pub struct ParsedReference {
    pub id: u32,
    pub matched: String,
    pub index: usize,
}

/// 解析输入中的引用标记。
pub fn parse_references(input: &str) -> Vec<ParsedReference> {
    let mut results = Vec::new();
    let mut search_start = 0;

    while let Some(bracket_start) = input[search_start..].find('[') {
        let abs_start = search_start + bracket_start;
        if let Some(bracket_end) = input[abs_start..].find(']') {
            let abs_end = abs_start + bracket_end + 1;
            let inner = &input[abs_start + 1..abs_end - 1];

            // 匹配 "Pasted text #N", "Image #N", "...Truncated text #N"
            let is_ref = inner.starts_with("Pasted text #")
                || inner.starts_with("Image #")
                || inner.starts_with("...Truncated text #");

            if is_ref {
                // 提取数字 ID
                if let Some(hash_pos) = inner.find('#') {
                    let after_hash = &inner[hash_pos + 1..];
                    let num_str: String = after_hash.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(id) = num_str.parse::<u32>() {
                        if id > 0 {
                            results.push(ParsedReference {
                                id,
                                matched: input[abs_start..abs_end].to_string(),
                                index: abs_start,
                            });
                        }
                    }
                }
            }
            search_start = abs_end;
        } else {
            break;
        }
    }

    results
}

/// 展开粘贴文本引用。图片引用不展开。
pub fn expand_pasted_text_refs(
    input: &str,
    pasted_contents: &HashMap<u32, PastedContent>,
) -> String {
    let refs = parse_references(input);
    let mut expanded = input.to_string();

    // 从后往前替换，保持偏移量有效
    for r in refs.iter().rev() {
        if let Some(content) = pasted_contents.get(&r.id) {
            if content.content_type == "text" {
                expanded = format!(
                    "{}{}{}",
                    &expanded[..r.index],
                    content.content,
                    &expanded[r.index + r.matched.len()..],
                );
            }
        }
    }

    expanded
}

// ---------------------------------------------------------------------------
// History manager
// ---------------------------------------------------------------------------

/// 历史管理器。
pub struct HistoryManager {
    config_home_dir: PathBuf,
    pending_entries: Mutex<Vec<LogEntry>>,
    last_added_entry: Mutex<Option<LogEntry>>,
    skipped_timestamps: Mutex<HashSet<u64>>,
}

impl HistoryManager {
    /// 创建新的历史管理器。
    pub fn new(config_home_dir: PathBuf) -> Self {
        Self {
            config_home_dir,
            pending_entries: Mutex::new(Vec::new()),
            last_added_entry: Mutex::new(None),
            skipped_timestamps: Mutex::new(HashSet::new()),
        }
    }

    fn history_path(&self) -> PathBuf {
        self.config_home_dir.join("history.jsonl")
    }

    /// 添加到历史记录。
    pub async fn add_to_history(
        &self,
        entry: HistoryEntry,
        project_root: &str,
        session_id: &str,
        skip_env: bool,
    ) {
        if skip_env {
            return;
        }

        let stored_pasted: HashMap<u32, StoredPastedContent> = entry
            .pasted_contents
            .iter()
            .filter(|(_, c)| c.content_type != "image")
            .map(|(&id, c)| {
                if c.content.len() <= MAX_PASTED_CONTENT_LENGTH {
                    (
                        id,
                        StoredPastedContent {
                            id: c.id,
                            content_type: c.content_type.clone(),
                            content: Some(c.content.clone()),
                            content_hash: None,
                            media_type: c.media_type.clone(),
                            filename: c.filename.clone(),
                        },
                    )
                } else {
                    let hash = hash_pasted_text(&c.content);
                    (
                        id,
                        StoredPastedContent {
                            id: c.id,
                            content_type: c.content_type.clone(),
                            content: None,
                            content_hash: Some(hash),
                            media_type: c.media_type.clone(),
                            filename: c.filename.clone(),
                        },
                    )
                }
            })
            .collect();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let log_entry = LogEntry {
            display: entry.display,
            pasted_contents: stored_pasted,
            timestamp: now,
            project: project_root.to_string(),
            session_id: Some(session_id.to_string()),
        };

        {
            let mut pending = self.pending_entries.lock().expect("lock poisoned");
            pending.push(log_entry.clone());
        }
        {
            let mut last = self.last_added_entry.lock().expect("lock poisoned");
            *last = Some(log_entry);
        }

        // 异步刷新
        let _ = self.flush_history().await;
    }

    /// 刷新待写入的历史条目到磁盘。
    pub async fn flush_history(&self) -> Result<(), std::io::Error> {
        let entries: Vec<LogEntry> = {
            let mut pending = self.pending_entries.lock().expect("lock poisoned");
            if pending.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *pending)
        };

        let history_path = self.history_path();

        // 确保目录存在
        if let Some(parent) = history_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&history_path)
            .await?;

        let mut data = String::new();
        for entry in &entries {
            if let Ok(json) = serde_json::to_string(entry) {
                data.push_str(&json);
                data.push('\n');
            }
        }

        file.write_all(data.as_bytes()).await?;
        Ok(())
    }

    /// 获取当前项目的历史条目（最新优先，当前会话优先）。
    pub async fn get_history(
        &self,
        current_project: &str,
        current_session: &str,
    ) -> Vec<HistoryEntry> {
        let mut entries = Vec::new();
        let mut other_session_entries = Vec::new();
        let mut yielded = 0usize;

        let skipped = self.skipped_timestamps.lock().expect("lock poisoned").clone();

        // 先收集 pending
        {
            let pending = self.pending_entries.lock().expect("lock poisoned");
            for entry in pending.iter().rev() {
                if entry.project != current_project {
                    continue;
                }
                if entry.session_id.as_deref() == Some(current_session)
                    && skipped.contains(&entry.timestamp)
                {
                    continue;
                }
                let he = log_entry_to_history_entry(entry);
                if entry.session_id.as_deref() == Some(current_session) {
                    entries.push(he);
                    yielded += 1;
                } else {
                    other_session_entries.push(he);
                }
                if yielded + other_session_entries.len() >= MAX_HISTORY_ITEMS {
                    break;
                }
            }
        }

        // 再从文件读取
        if yielded + other_session_entries.len() < MAX_HISTORY_ITEMS {
            if let Ok(file_entries) = self.read_history_file().await {
                for entry in file_entries.iter().rev() {
                    if entry.project != current_project {
                        continue;
                    }
                    if entry.session_id.as_deref() == Some(current_session)
                        && skipped.contains(&entry.timestamp)
                    {
                        continue;
                    }
                    let he = log_entry_to_history_entry(entry);
                    if entry.session_id.as_deref() == Some(current_session) {
                        entries.push(he);
                        yielded += 1;
                    } else {
                        other_session_entries.push(he);
                    }
                    if yielded + other_session_entries.len() >= MAX_HISTORY_ITEMS {
                        break;
                    }
                }
            }
        }

        // 追加其他会话的条目
        for he in other_session_entries {
            if yielded >= MAX_HISTORY_ITEMS {
                break;
            }
            entries.push(he);
            yielded += 1;
        }

        entries
    }

    /// 获取带时间戳的历史（用于 ctrl+r）。
    pub async fn get_timestamped_history(
        &self,
        current_project: &str,
    ) -> Vec<TimestampedHistoryEntry> {
        let mut results = Vec::new();
        let mut seen = HashSet::new();

        // 从 pending 收集
        {
            let pending = self.pending_entries.lock().expect("lock poisoned");
            for entry in pending.iter().rev() {
                if entry.project != current_project {
                    continue;
                }
                if seen.contains(&entry.display) {
                    continue;
                }
                seen.insert(entry.display.clone());
                results.push(TimestampedHistoryEntry {
                    display: entry.display.clone(),
                    timestamp: entry.timestamp,
                });
                if seen.len() >= MAX_HISTORY_ITEMS {
                    return results;
                }
            }
        }

        // 从文件收集
        if seen.len() < MAX_HISTORY_ITEMS {
            if let Ok(file_entries) = self.read_history_file().await {
                for entry in file_entries.iter().rev() {
                    if entry.project != current_project {
                        continue;
                    }
                    if seen.contains(&entry.display) {
                        continue;
                    }
                    seen.insert(entry.display.clone());
                    results.push(TimestampedHistoryEntry {
                        display: entry.display.clone(),
                        timestamp: entry.timestamp,
                    });
                    if seen.len() >= MAX_HISTORY_ITEMS {
                        break;
                    }
                }
            }
        }

        results
    }

    /// 清除所有待写入条目。
    pub fn clear_pending_history_entries(&self) {
        let mut pending = self.pending_entries.lock().expect("lock poisoned");
        pending.clear();
        let mut last = self.last_added_entry.lock().expect("lock poisoned");
        *last = None;
        let mut skipped = self.skipped_timestamps.lock().expect("lock poisoned");
        skipped.clear();
    }

    /// 撤销最近一次 add_to_history（用于 auto-restore-on-interrupt）。
    pub fn remove_last_from_history(&self) {
        let entry = {
            let mut last = self.last_added_entry.lock().expect("lock poisoned");
            match last.take() {
                Some(e) => e,
                None => return,
            }
        };

        let mut pending = self.pending_entries.lock().expect("lock poisoned");
        // 尝试从 pending 中移除
        if let Some(pos) = pending
            .iter()
            .rposition(|e| e.timestamp == entry.timestamp && e.display == entry.display)
        {
            pending.remove(pos);
        } else {
            // 已刷新到磁盘，加入跳过集合
            let mut skipped = self.skipped_timestamps.lock().expect("lock poisoned");
            skipped.insert(entry.timestamp);
        }
    }

    // ---- Internal ----

    async fn read_history_file(&self) -> Result<Vec<LogEntry>, std::io::Error> {
        let path = self.history_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path).await?;
        let mut entries = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<LogEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    warn!("failed to parse history line: {}", e);
                }
            }
        }
        Ok(entries)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn log_entry_to_history_entry(entry: &LogEntry) -> HistoryEntry {
    let pasted_contents: HashMap<u32, PastedContent> = entry
        .pasted_contents
        .iter()
        .filter_map(|(&id, stored)| {
            resolve_stored_pasted_content(stored).map(|c| (id, c))
        })
        .collect();

    HistoryEntry {
        display: entry.display.clone(),
        pasted_contents,
    }
}

fn resolve_stored_pasted_content(stored: &StoredPastedContent) -> Option<PastedContent> {
    if let Some(ref content) = stored.content {
        return Some(PastedContent {
            id: stored.id,
            content_type: stored.content_type.clone(),
            content: content.clone(),
            media_type: stored.media_type.clone(),
            filename: stored.filename.clone(),
        });
    }

    if let Some(ref hash) = stored.content_hash {
        // 从粘贴存储中检索内容（异步 API；同步上下文通过临时 runtime 桥接）。
        // paste_store::retrieve_pasted_text 是 async 接口；外部 history 解析
        // 一般运行于 tokio runtime 中，故优先复用 current handle。
        let hash = hash.clone();
        let content = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| {
                handle.block_on(async move {
                    mossen_utils::paste_store::retrieve_pasted_text(&hash).await
                })
            })
        } else {
            // 无 runtime — 创建一次性 current_thread runtime
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()
                .and_then(|rt| {
                    rt.block_on(async move {
                        mossen_utils::paste_store::retrieve_pasted_text(&hash).await
                    })
                })
        };
        return content.map(|c| PastedContent {
            id: stored.id,
            content_type: stored.content_type.clone(),
            content: c,
            media_type: stored.media_type.clone(),
            filename: stored.filename.clone(),
        });
    }

    None
}

/// 计算粘贴文本的 hash（用于大文本的外部存储）。
fn hash_pasted_text(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
