//! Root 模块 — 翻译自根目录 TS 文件
//!
//! 包含：
//! - context.ts → 系统上下文和用户上下文
//! - history.ts → 命令历史记录
//! - cost-tracker.ts → 费用跟踪
//! - Task.ts → 任务类型与 ID 生成
//! - tasks.ts → 任务注册表
//! - ink.ts → 终端 UI 抽象（渲染入口）
//! - commands.ts → 命令注册表
//! - tools.ts → 工具注册表
//! - moreright/ → MoreRight hooks (stub)

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════════
// context.ts
// ═══════════════════════════════════════════════════════════════════════════════

const MAX_STATUS_CHARS: usize = 2000;

/// 系统提示注入（用于缓存破坏，内部调试用）。
static SYSTEM_PROMPT_INJECTION: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// 获取当前的系统提示注入值。
pub fn get_system_prompt_injection() -> Option<String> {
    SYSTEM_PROMPT_INJECTION.lock().unwrap().clone()
}

/// 设置系统提示注入值。
pub fn set_system_prompt_injection(value: Option<String>) {
    *SYSTEM_PROMPT_INJECTION.lock().unwrap() = value;
}

/// 获取 git status 摘要。
pub async fn get_git_status() -> Option<String> {
    if std::env::var("NODE_ENV").ok().as_deref() == Some("test") {
        return None;
    }

    let git_exe = which::which("git").ok()?;

    // 检查是否是 git 仓库
    let is_git = tokio::process::Command::new(&git_exe)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .await
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_git {
        return None;
    }

    // 并行获取多个 git 信息
    let (branch_out, main_branch_out, status_out, log_out, user_out) = tokio::join!(
        exec_git(&git_exe, &["branch", "--show-current"]),
        exec_git(&git_exe, &["config", "init.defaultBranch"]),
        exec_git(&git_exe, &["--no-optional-locks", "status", "--short"]),
        exec_git(&git_exe, &["--no-optional-locks", "log", "--oneline", "-n", "5"]),
        exec_git(&git_exe, &["config", "user.name"]),
    );

    let branch = branch_out.unwrap_or_else(|| "HEAD".to_string());
    let main_branch = main_branch_out.unwrap_or_else(|| "main".to_string());
    let status = status_out.unwrap_or_default();
    let log = log_out.unwrap_or_default();
    let user_name = user_out;

    // 截断过长的 status
    let truncated_status = if status.len() > MAX_STATUS_CHARS {
        format!(
            "{}\n... (truncated because it exceeds 2k characters. \
             If you need more information, run \"git status\" using BashTool)",
            &status[..MAX_STATUS_CHARS]
        )
    } else {
        status
    };

    let mut parts = vec![
        "This is the git status at the start of the conversation. \
         Note that this status is a snapshot in time, and will not update during the conversation."
            .to_string(),
        format!("Current branch: {}", branch),
        format!("Main branch (you will usually use this for PRs): {}", main_branch),
    ];
    if let Some(name) = user_name {
        parts.push(format!("Git user: {}", name));
    }
    parts.push(format!(
        "Status:\n{}",
        if truncated_status.is_empty() {
            "(clean)".to_string()
        } else {
            truncated_status
        }
    ));
    parts.push(format!("Recent commits:\n{}", log));

    Some(parts.join("\n\n"))
}

/// 执行 git 命令并返回 stdout。
async fn exec_git(git_exe: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = tokio::process::Command::new(git_exe)
        .args(args)
        .output()
        .await
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// 获取系统上下文（缓存一次）。
pub async fn get_system_context() -> HashMap<String, String> {
    let mut ctx = HashMap::new();

    // CCR 模式下跳过 git
    let is_remote = std::env::var("MOSSEN_CODE_REMOTE")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    if !is_remote {
        if let Some(git_status) = get_git_status().await {
            ctx.insert("gitStatus".to_string(), git_status);
        }
    }

    // 缓存破坏注入
    if let Some(injection) = get_system_prompt_injection() {
        ctx.insert(
            "cacheBreaker".to_string(),
            format!("[CACHE_BREAKER: {}]", injection),
        );
    }

    ctx
}

/// 获取用户上下文。
pub async fn get_user_context() -> HashMap<String, String> {
    let mut ctx = HashMap::new();

    // 当前日期
    let now = chrono::Local::now();
    ctx.insert(
        "currentDate".to_string(),
        format!("Today's date is {}.", now.format("%Y-%m-%d")),
    );

    // MOSSEN.md 内容
    let disable_mossen_md = std::env::var("MOSSEN_CODE_DISABLE_MOSSEN_MDS")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    if !disable_mossen_md {
        if let Some(content) = load_mossen_md_content().await {
            ctx.insert("mossenMd".to_string(), content);
        }
    }

    ctx
}

/// 加载 MOSSEN.md 文件内容。
async fn load_mossen_md_content() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let candidates = [
        cwd.join("MOSSEN.md"),
        cwd.join(".mossen").join("MOSSEN.md"),
    ];

    for path in &candidates {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            if !content.trim().is_empty() {
                return Some(content);
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// history.ts
// ═══════════════════════════════════════════════════════════════════════════════

const MAX_HISTORY_ITEMS: usize = 100;
const MAX_PASTED_CONTENT_LENGTH: usize = 1024;

/// 历史条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub display: String,
    #[serde(default)]
    pub pasted_contents: HashMap<u32, PastedContent>,
}

/// 粘贴内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedContent {
    pub id: u32,
    #[serde(rename = "type")]
    pub content_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// 存储格式的粘贴内容。
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

/// 日志文件条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    display: String,
    #[serde(default)]
    pasted_contents: HashMap<u32, StoredPastedContent>,
    timestamp: u64,
    project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

/// 获取粘贴文本的引用行数。
pub fn get_pasted_text_ref_num_lines(text: &str) -> usize {
    text.matches('\n').count() + text.matches('\r').count()
        - text.matches("\r\n").count()
}

/// 格式化粘贴文本引用。
pub fn format_pasted_text_ref(id: u32, num_lines: usize) -> String {
    if num_lines == 0 {
        format!("[Pasted text #{}]", id)
    } else {
        format!("[Pasted text #{} +{} lines]", id, num_lines)
    }
}

/// 格式化图片引用。
pub fn format_image_ref(id: u32) -> String {
    format!("[Image #{}]", id)
}

/// 引用匹配结果。
#[derive(Debug, Clone)]
pub struct ReferenceMatch {
    pub id: u32,
    pub matched: String,
    pub index: usize,
}

/// 解析输入中的引用。
pub fn parse_references(input: &str) -> Vec<ReferenceMatch> {
    let re = regex::Regex::new(
        r"\[(Pasted text|Image|\.\.\.Truncated text) #(\d+)(?: \+\d+ lines)?(\.)*\]",
    )
    .unwrap();

    re.find_iter(input)
        .filter_map(|m| {
            let caps = re.captures(m.as_str())?;
            let id: u32 = caps.get(2)?.as_str().parse().ok()?;
            if id == 0 {
                return None;
            }
            Some(ReferenceMatch {
                id,
                matched: m.as_str().to_string(),
                index: m.start(),
            })
        })
        .collect()
}

/// 展开粘贴文本引用。
pub fn expand_pasted_text_refs(
    input: &str,
    pasted_contents: &HashMap<u32, PastedContent>,
) -> String {
    let refs = parse_references(input);
    let mut expanded = input.to_string();

    // 从后往前替换，避免偏移错位
    for r in refs.iter().rev() {
        if let Some(content) = pasted_contents.get(&r.id) {
            if content.content_type == "text" {
                expanded = format!(
                    "{}{}{}",
                    &expanded[..r.index],
                    content.content,
                    &expanded[r.index + r.matched.len()..]
                );
            }
        }
    }
    expanded
}

/// 历史记录管理器。
pub struct HistoryManager {
    pending_entries: Vec<LogEntry>,
    is_writing: bool,
    last_added_entry: Option<LogEntry>,
    skipped_timestamps: HashSet<u64>,
    project_root: String,
    session_id: String,
}

impl HistoryManager {
    /// 创建新的历史管理器。
    pub fn new(project_root: String, session_id: String) -> Self {
        Self {
            pending_entries: Vec::new(),
            is_writing: false,
            last_added_entry: None,
            skipped_timestamps: HashSet::new(),
            project_root,
            session_id,
        }
    }

    /// 添加历史条目。
    pub fn add_to_history(&mut self, command: HistoryEntry) {
        if std::env::var("MOSSEN_CODE_SKIP_PROMPT_HISTORY")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
        {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let stored_pasted = command
            .pasted_contents
            .iter()
            .filter(|(_, c)| c.content_type != "image")
            .map(|(id, c)| {
                let stored = if c.content.len() <= MAX_PASTED_CONTENT_LENGTH {
                    StoredPastedContent {
                        id: c.id,
                        content_type: c.content_type.clone(),
                        content: Some(c.content.clone()),
                        content_hash: None,
                        media_type: c.media_type.clone(),
                        filename: c.filename.clone(),
                    }
                } else {
                    // 大内容使用 hash 引用
                    let hash = hash_pasted_text(&c.content);
                    StoredPastedContent {
                        id: c.id,
                        content_type: c.content_type.clone(),
                        content: None,
                        content_hash: Some(hash),
                        media_type: c.media_type.clone(),
                        filename: c.filename.clone(),
                    }
                };
                (*id, stored)
            })
            .collect();

        let entry = LogEntry {
            display: command.display,
            pasted_contents: stored_pasted,
            timestamp: now,
            project: self.project_root.clone(),
            session_id: Some(self.session_id.clone()),
        };

        self.pending_entries.push(entry.clone());
        self.last_added_entry = Some(entry);
    }

    /// 移除最后一条历史。
    pub fn remove_last_from_history(&mut self) {
        if let Some(entry) = self.last_added_entry.take() {
            if let Some(idx) = self
                .pending_entries
                .iter()
                .rposition(|e| e.timestamp == entry.timestamp)
            {
                self.pending_entries.remove(idx);
            } else {
                self.skipped_timestamps.insert(entry.timestamp);
            }
        }
    }

    /// 清空待写入条目。
    pub fn clear_pending(&mut self) {
        self.pending_entries.clear();
        self.last_added_entry = None;
        self.skipped_timestamps.clear();
    }

    /// 刷新历史到磁盘。
    pub async fn flush(&mut self) -> anyhow::Result<()> {
        if self.pending_entries.is_empty() {
            return Ok(());
        }

        let config_dir = mossen_utils::env::get_mossen_config_home_dir();
        let history_path = config_dir.join("history.jsonl");

        // 确保文件存在
        if !history_path.exists() {
            if let Some(parent) = history_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&history_path, "").await?;
        }

        let json_lines: String = self
            .pending_entries
            .iter()
            .filter_map(|entry| serde_json::to_string(entry).ok())
            .map(|s| s + "\n")
            .collect();

        self.pending_entries.clear();

        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&history_path)
            .await?
            .write_all(json_lines.as_bytes())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to append history: {}", e))?;

        Ok(())
    }

    /// 获取历史（反向读取文件）。
    pub async fn get_history(&self) -> Vec<HistoryEntry> {
        let config_dir = mossen_utils::env::get_mossen_config_home_dir();
        let history_path = config_dir.join("history.jsonl");

        let content = match tokio::fs::read_to_string(&history_path).await {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut current_session_entries = Vec::new();
        let mut other_entries = Vec::new();
        let mut seen = HashSet::new();

        for line in content.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: LogEntry = match serde_json::from_str(line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.project != self.project_root {
                continue;
            }
            if seen.contains(&entry.display) {
                continue;
            }

            // 跳过已删除的
            if entry.session_id.as_deref() == Some(&self.session_id)
                && self.skipped_timestamps.contains(&entry.timestamp)
            {
                continue;
            }

            seen.insert(entry.display.clone());

            let history_entry = HistoryEntry {
                display: entry.display.clone(),
                pasted_contents: resolve_stored_pasted_contents(&entry.pasted_contents),
            };

            if entry.session_id.as_deref() == Some(&self.session_id) {
                current_session_entries.push(history_entry);
            } else {
                other_entries.push(history_entry);
            }

            if current_session_entries.len() + other_entries.len() >= MAX_HISTORY_ITEMS {
                break;
            }
        }

        // 当前会话优先
        current_session_entries.extend(other_entries);
        current_session_entries.truncate(MAX_HISTORY_ITEMS);
        current_session_entries
    }
}

/// 解析存储的粘贴内容。
fn resolve_stored_pasted_contents(
    stored: &HashMap<u32, StoredPastedContent>,
) -> HashMap<u32, PastedContent> {
    stored
        .iter()
        .filter_map(|(id, s)| {
            let content = s.content.as_deref().unwrap_or("").to_string();
            Some((
                *id,
                PastedContent {
                    id: s.id,
                    content_type: s.content_type.clone(),
                    content,
                    media_type: s.media_type.clone(),
                    filename: s.filename.clone(),
                },
            ))
        })
        .collect()
}

/// 计算粘贴文本的 hash。
fn hash_pasted_text(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

use tokio::io::AsyncWriteExt;

// ═══════════════════════════════════════════════════════════════════════════════
// cost-tracker.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 模型使用量。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
    pub context_window: u64,
    pub max_output_tokens: u64,
}

/// 存储的费用状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredCostState {
    pub total_cost_usd: f64,
    pub total_api_duration: f64,
    pub total_api_duration_without_retries: f64,
    pub total_tool_duration: f64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub last_duration: Option<f64>,
    pub model_usage: Option<HashMap<String, ModelUsage>>,
}

/// 费用跟踪器。
pub struct CostTracker {
    total_cost_usd: f64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cache_read_input_tokens: u64,
    total_cache_creation_input_tokens: u64,
    total_web_search_requests: u64,
    total_api_duration_ms: f64,
    total_api_duration_without_retries_ms: f64,
    total_tool_duration_ms: f64,
    total_duration_ms: f64,
    total_lines_added: u64,
    total_lines_removed: u64,
    model_usage: HashMap<String, ModelUsage>,
    has_unknown_model_cost: bool,
}

impl CostTracker {
    /// 创建新的跟踪器。
    pub fn new() -> Self {
        Self {
            total_cost_usd: 0.0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read_input_tokens: 0,
            total_cache_creation_input_tokens: 0,
            total_web_search_requests: 0,
            total_api_duration_ms: 0.0,
            total_api_duration_without_retries_ms: 0.0,
            total_tool_duration_ms: 0.0,
            total_duration_ms: 0.0,
            total_lines_added: 0,
            total_lines_removed: 0,
            model_usage: HashMap::new(),
            has_unknown_model_cost: false,
        }
    }

    /// 重置所有状态。
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// 从存储状态恢复。
    pub fn restore(&mut self, stored: &StoredCostState) {
        self.total_cost_usd = stored.total_cost_usd;
        self.total_api_duration_ms = stored.total_api_duration;
        self.total_api_duration_without_retries_ms = stored.total_api_duration_without_retries;
        self.total_tool_duration_ms = stored.total_tool_duration;
        self.total_lines_added = stored.total_lines_added;
        self.total_lines_removed = stored.total_lines_removed;
        if let Some(ref usage) = stored.model_usage {
            self.model_usage = usage.clone();
            for mu in usage.values() {
                self.total_input_tokens += mu.input_tokens;
                self.total_output_tokens += mu.output_tokens;
                self.total_cache_read_input_tokens += mu.cache_read_input_tokens;
                self.total_cache_creation_input_tokens += mu.cache_creation_input_tokens;
                self.total_web_search_requests += mu.web_search_requests;
            }
        }
    }

    /// 添加 API 调用的费用。
    pub fn add_usage(
        &mut self,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cache_read: u64,
        cache_creation: u64,
        web_search: u64,
        cost_usd: f64,
    ) {
        self.total_cost_usd += cost_usd;
        self.total_input_tokens += input_tokens;
        self.total_output_tokens += output_tokens;
        self.total_cache_read_input_tokens += cache_read;
        self.total_cache_creation_input_tokens += cache_creation;
        self.total_web_search_requests += web_search;

        let entry = self.model_usage.entry(model.to_string()).or_default();
        entry.input_tokens += input_tokens;
        entry.output_tokens += output_tokens;
        entry.cache_read_input_tokens += cache_read;
        entry.cache_creation_input_tokens += cache_creation;
        entry.web_search_requests += web_search;
        entry.cost_usd += cost_usd;
    }

    /// 添加代码行变更。
    pub fn add_lines_changed(&mut self, added: u64, removed: u64) {
        self.total_lines_added += added;
        self.total_lines_removed += removed;
    }

    /// 获取总费用。
    pub fn total_cost(&self) -> f64 {
        self.total_cost_usd
    }

    /// 格式化费用显示字符串。
    pub fn format_cost_display(cost: f64) -> String {
        if cost > 0.5 {
            format!("${:.2}", (cost * 100.0).round() / 100.0)
        } else {
            format!("${:.4}", cost)
        }
    }

    /// 格式化完整费用摘要。
    pub fn format_total_cost(&self) -> String {
        let cost_display = if self.has_unknown_model_cost {
            format!(
                "{} (costs may be inaccurate due to usage of unknown models)",
                Self::format_cost_display(self.total_cost_usd)
            )
        } else {
            Self::format_cost_display(self.total_cost_usd)
        };

        let model_usage_display = self.format_model_usage();

        format!(
            "Total cost:            {}\n\
             Total duration (API):  {}\n\
             Total duration (wall): {}\n\
             Total code changes:    {} {} added, {} {} removed\n\
             {}",
            cost_display,
            format_duration_ms(self.total_api_duration_ms),
            format_duration_ms(self.total_duration_ms),
            self.total_lines_added,
            if self.total_lines_added == 1 { "line" } else { "lines" },
            self.total_lines_removed,
            if self.total_lines_removed == 1 { "line" } else { "lines" },
            model_usage_display,
        )
    }

    /// 格式化按模型的使用量。
    fn format_model_usage(&self) -> String {
        if self.model_usage.is_empty() {
            return "Usage:                 0 input, 0 output, 0 cache read, 0 cache write"
                .to_string();
        }

        let mut result = "Usage by model:".to_string();
        for (model, usage) in &self.model_usage {
            let web_search_part = if usage.web_search_requests > 0 {
                format!(", {} web search", format_number(usage.web_search_requests))
            } else {
                String::new()
            };
            result.push_str(&format!(
                "\n{:>21}  {} input, {} output, {} cache read, {} cache write{} ({})",
                format!("{}:", model),
                format_number(usage.input_tokens),
                format_number(usage.output_tokens),
                format_number(usage.cache_read_input_tokens),
                format_number(usage.cache_creation_input_tokens),
                web_search_part,
                Self::format_cost_display(usage.cost_usd),
            ));
        }
        result
    }

    /// 导出存储状态。
    pub fn to_stored_state(&self, session_id: &str) -> StoredCostState {
        StoredCostState {
            total_cost_usd: self.total_cost_usd,
            total_api_duration: self.total_api_duration_ms,
            total_api_duration_without_retries: self.total_api_duration_without_retries_ms,
            total_tool_duration: self.total_tool_duration_ms,
            total_lines_added: self.total_lines_added,
            total_lines_removed: self.total_lines_removed,
            last_duration: Some(self.total_duration_ms),
            model_usage: Some(self.model_usage.clone()),
        }
    }
}

/// 格式化数字（带千分位）。
pub fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// 格式化毫秒持续时间。
pub fn format_duration_ms(ms: f64) -> String {
    let secs = ms / 1000.0;
    if secs < 60.0 {
        format!("{:.1}s", secs)
    } else if secs < 3600.0 {
        format!("{}m {}s", (secs / 60.0) as u64, (secs % 60.0) as u64)
    } else {
        format!(
            "{}h {}m {}s",
            (secs / 3600.0) as u64,
            ((secs % 3600.0) / 60.0) as u64,
            (secs % 60.0) as u64
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Task.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 任务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    LocalBash,
    LocalAgent,
    RemoteAgent,
    InProcessTeammate,
    LocalWorkflow,
    MonitorMcp,
    Dream,
}

/// 任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

impl TaskStatus {
    /// 检查是否为终态。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Killed)
    }
}

/// 任务状态基础字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStateBase {
    pub id: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    pub start_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_paused_ms: Option<u64>,
    pub output_file: String,
    pub output_offset: u64,
    pub notified: bool,
}

/// 本地 Shell 任务输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellSpawnInput {
    pub command: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// 任务 ID 前缀映射。
fn get_task_id_prefix(task_type: TaskType) -> char {
    match task_type {
        TaskType::LocalBash => 'b',
        TaskType::LocalAgent => 'a',
        TaskType::RemoteAgent => 'r',
        TaskType::InProcessTeammate => 't',
        TaskType::LocalWorkflow => 'w',
        TaskType::MonitorMcp => 'm',
        TaskType::Dream => 'd',
    }
}

/// 安全字母表（数字 + 小写字母），36^8 ≈ 2.8 万亿组合。
const TASK_ID_ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// 生成任务 ID。
pub fn generate_task_id(task_type: TaskType) -> String {
    let prefix = get_task_id_prefix(task_type);
    let mut id = String::with_capacity(9);
    id.push(prefix);

    let bytes: [u8; 8] = rand::random();
    for b in bytes {
        let idx = (b as usize) % TASK_ID_ALPHABET.len();
        id.push(TASK_ID_ALPHABET[idx] as char);
    }
    id
}

/// 获取任务输出文件路径。
pub fn get_task_output_path(task_id: &str) -> PathBuf {
    let tmp = std::env::temp_dir();
    tmp.join("mossen-tasks").join(format!("{}.log", task_id))
}

/// 创建任务基础状态。
pub fn create_task_state_base(
    id: String,
    task_type: TaskType,
    description: String,
    tool_use_id: Option<String>,
) -> TaskStateBase {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let output_file = get_task_output_path(&id).to_string_lossy().to_string();

    TaskStateBase {
        id,
        task_type,
        status: TaskStatus::Pending,
        description,
        tool_use_id,
        start_time: now,
        end_time: None,
        total_paused_ms: None,
        output_file,
        output_offset: 0,
        notified: false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// tasks.ts (任务注册表)
// ═══════════════════════════════════════════════════════════════════════════════

/// 任务定义 trait。
pub trait TaskDefinition: Send + Sync {
    fn name(&self) -> &str;
    fn task_type(&self) -> TaskType;
}

/// 获取所有已注册的任务类型。
pub fn get_all_task_types() -> Vec<TaskType> {
    vec![
        TaskType::LocalBash,
        TaskType::LocalAgent,
        TaskType::Dream,
        TaskType::LocalWorkflow,
        TaskType::MonitorMcp,
    ]
}

/// 根据类型查找任务。
pub fn get_task_type_by_name(name: &str) -> Option<TaskType> {
    match name {
        "local_bash" => Some(TaskType::LocalBash),
        "local_agent" => Some(TaskType::LocalAgent),
        "remote_agent" => Some(TaskType::RemoteAgent),
        "in_process_teammate" => Some(TaskType::InProcessTeammate),
        "local_workflow" => Some(TaskType::LocalWorkflow),
        "monitor_mcp" => Some(TaskType::MonitorMcp),
        "dream" => Some(TaskType::Dream),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// commands.ts (命令注册表)
// ═══════════════════════════════════════════════════════════════════════════════

/// 命令结果显示方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResultDisplay {
    /// 在状态栏简短显示。
    System,
    /// 作为独立消息显示。
    Message,
    /// 不显示。
    Silent,
}

/// 命令定义。
#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub is_enabled: bool,
    pub is_hidden: bool,
}

/// 获取命令名称（含别名匹配）。
pub fn get_command_name<'a>(commands: &'a [CommandDef], input: &str) -> Option<&'a CommandDef> {
    let lower = input.to_lowercase();
    commands.iter().find(|cmd| {
        cmd.name == lower || cmd.aliases.iter().any(|a| a == &lower)
    })
}

/// 检查命令是否启用。
pub fn is_command_enabled(cmd: &CommandDef) -> bool {
    cmd.is_enabled
}

/// 获取所有内置命令列表。
pub fn get_builtin_commands() -> Vec<CommandDef> {
    vec![
        cmd("help", &["h", "?"], "Show available commands"),
        cmd("clear", &[], "Clear the conversation"),
        cmd("compact", &[], "Compact the conversation context"),
        cmd("config", &[], "Open configuration"),
        cmd("cost", &[], "Show session cost"),
        cmd("doctor", &[], "Run diagnostics"),
        cmd("exit", &["quit", "q"], "Exit the application"),
        cmd("init", &[], "Initialize project configuration"),
        cmd("login", &[], "Log in to your account"),
        cmd("logout", &[], "Log out"),
        cmd("resume", &["r"], "Resume a previous conversation"),
        cmd("status", &[], "Show session status"),
        cmd("memory", &[], "Manage memory files"),
        cmd("mcp", &[], "Manage MCP servers"),
        cmd("vim", &[], "Toggle vim mode"),
        cmd("theme", &[], "Change color theme"),
        cmd("lang", &[], "Change UI language"),
        cmd("feedback", &[], "Submit feedback"),
        cmd("review", &[], "Code review"),
        cmd("commit", &[], "Create a git commit"),
        cmd("diff", &[], "Show diff"),
        cmd("share", &[], "Share conversation"),
        cmd("copy", &[], "Copy last response"),
        cmd("rename", &[], "Rename conversation"),
        cmd("tasks", &[], "Manage background tasks"),
        cmd("skills", &[], "Manage skills"),
        cmd("keybindings", &["keys"], "Show keybindings"),
    ]
}

/// 辅助：创建命令定义。
fn cmd(name: &str, aliases: &[&str], description: &str) -> CommandDef {
    CommandDef {
        name: name.to_string(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        description: description.to_string(),
        is_enabled: true,
        is_hidden: false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// moreright/useMoreRight.tsx (stub)
// ═══════════════════════════════════════════════════════════════════════════════

/// MoreRight hook 的结果（外部构建 stub）。
pub struct MoreRightResult {
    /// 查询前回调：返回 true 则继续。
    pub on_before_query_allows: bool,
}

impl Default for MoreRightResult {
    fn default() -> Self {
        Self {
            on_before_query_allows: true,
        }
    }
}

/// MoreRight stub — 外部构建时直接返回允许。
pub fn use_more_right_stub() -> MoreRightResult {
    MoreRightResult::default()
}
