//! Screens 模块 — 翻译自 screens/ 目录
//!
//! TS 原版中这些是 React 组件（REPL.tsx, Doctor.tsx, ResumeConversation.tsx）。
//! Rust 版提取其核心域逻辑：
//! - REPL 主循环的状态机和事件处理
//! - Doctor 诊断信息收集与格式化
//! - ResumeConversation 会话恢复流程

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ─── ResumeConversation (screens/ResumeConversation.tsx) ─────────────────────

/// PR 标识符解析：从数字或 GitHub URL 提取 PR 号。
pub fn parse_pr_identifier(value: &str) -> Option<u64> {
    // 直接数字
    if let Ok(n) = value.parse::<u64>() {
        if n > 0 {
            return Some(n);
        }
    }
    // GitHub URL: github.com/owner/repo/pull/123
    // 手动解析避免依赖 regex
    if let Some(pull_idx) = value.find("/pull/") {
        let after = &value[pull_idx + 6..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = num_str.parse::<u64>() {
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

/// 会话日志选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogOption {
    pub session_id: Option<String>,
    pub title: String,
    pub full_path: Option<String>,
    pub is_sidechain: bool,
    pub pr_number: Option<u64>,
    pub value: usize,
}

/// 跨项目恢复检查结果。
#[derive(Debug, Clone)]
pub struct CrossProjectCheck {
    pub is_cross_project: bool,
    pub is_same_repo_worktree: bool,
    pub command: String,
}

/// 检查是否为跨项目恢复。
pub fn check_cross_project_resume(
    log: &LogOption,
    show_all_projects: bool,
    worktree_paths: &[String],
) -> CrossProjectCheck {
    if !show_all_projects {
        return CrossProjectCheck {
            is_cross_project: false,
            is_same_repo_worktree: false,
            command: String::new(),
        };
    }

    let log_dir = log
        .full_path
        .as_deref()
        .and_then(|p| Path::new(p).parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let is_same_repo = worktree_paths.iter().any(|wp| log_dir.starts_with(wp));

    if is_same_repo {
        CrossProjectCheck {
            is_cross_project: true,
            is_same_repo_worktree: true,
            command: String::new(),
        }
    } else {
        let cmd = format!("mossen --resume --session-id {}", log.session_id.as_deref().unwrap_or(""));
        CrossProjectCheck {
            is_cross_project: true,
            is_same_repo_worktree: false,
            command: cmd,
        }
    }
}

/// 恢复会话的数据载荷。
#[derive(Debug, Clone)]
pub struct ResumeData {
    pub messages: Vec<serde_json::Value>,
    pub session_id: Option<String>,
    pub agent_name: Option<String>,
    pub agent_color: Option<String>,
    pub agent_setting: Option<String>,
    pub file_history_snapshots: Vec<serde_json::Value>,
    pub content_replacements: Vec<serde_json::Value>,
    pub mode: Option<String>,
    pub worktree_session: Option<serde_json::Value>,
    pub context_collapse_commits: Vec<serde_json::Value>,
    pub context_collapse_snapshot: Option<serde_json::Value>,
}

/// 加载会话用于恢复。
pub async fn load_conversation_for_resume(
    log: &LogOption,
) -> anyhow::Result<Option<ResumeData>> {
    let full_path = match &log.full_path {
        Some(p) => p.clone(),
        None => return Ok(None),
    };

    let path = PathBuf::from(&full_path);
    if !path.exists() {
        return Ok(None);
    }

    let content = tokio::fs::read_to_string(&path).await?;
    let lines: Vec<&str> = content.lines().collect();

    let mut messages = Vec::new();
    let mut session_id = log.session_id.clone();
    let mut agent_name = None;
    let mut agent_color = None;
    let mut agent_setting = None;
    let mut mode = None;

    for line in &lines {
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(msg_type) = val.get("type").and_then(|t| t.as_str()) {
                match msg_type {
                    "message" => messages.push(val),
                    "metadata" => {
                        if let Some(sid) = val.get("sessionId").and_then(|s| s.as_str()) {
                            session_id = Some(sid.to_string());
                        }
                        if let Some(name) = val.get("agentName").and_then(|s| s.as_str()) {
                            agent_name = Some(name.to_string());
                        }
                        if let Some(color) = val.get("agentColor").and_then(|s| s.as_str()) {
                            agent_color = Some(color.to_string());
                        }
                        if let Some(setting) = val.get("agentSetting").and_then(|s| s.as_str()) {
                            agent_setting = Some(setting.to_string());
                        }
                        if let Some(m) = val.get("mode").and_then(|s| s.as_str()) {
                            mode = Some(m.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(Some(ResumeData {
        messages,
        session_id,
        agent_name,
        agent_color,
        agent_setting,
        file_history_snapshots: Vec::new(),
        content_replacements: Vec::new(),
        mode,
        worktree_session: None,
        context_collapse_commits: Vec::new(),
        context_collapse_snapshot: None,
    }))
}

/// 按 PR 过滤日志。
pub fn filter_logs_by_pr(logs: &[LogOption], filter: &PrFilter) -> Vec<LogOption> {
    let base: Vec<LogOption> = logs.iter().filter(|l| !l.is_sidechain).cloned().collect();
    match filter {
        PrFilter::None => base,
        PrFilter::Any => base.into_iter().filter(|l| l.pr_number.is_some()).collect(),
        PrFilter::Number(n) => base.into_iter().filter(|l| l.pr_number == Some(*n)).collect(),
        PrFilter::String(s) => {
            if let Some(n) = parse_pr_identifier(s) {
                base.into_iter().filter(|l| l.pr_number == Some(n)).collect()
            } else {
                base
            }
        }
    }
}

/// PR 过滤模式。
#[derive(Debug, Clone)]
pub enum PrFilter {
    None,
    Any,
    Number(u64),
    String(String),
}

// ─── Doctor (screens/Doctor.tsx) ─────────────────────────────────────────────

/// Doctor 诊断信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub version: String,
    pub installation_type: String,
    pub installation_path: String,
    pub invoked_binary: String,
    pub config_install_method: String,
    pub package_manager: Option<String>,
    pub auto_updates: String,
    pub has_update_permissions: Option<bool>,
    pub recommendation: Option<String>,
    pub multiple_installations: Vec<InstallationEntry>,
    pub warnings: Vec<DoctorWarning>,
    pub ripgrep_status: RipgrepStatus,
    pub platform_runtime: serde_json::Value,
}

/// 安装条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationEntry {
    #[serde(rename = "type")]
    pub install_type: String,
    pub path: String,
}

/// Doctor 警告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorWarning {
    pub issue: String,
    pub fix: String,
}

/// Ripgrep 状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepStatus {
    pub working: bool,
    pub mode: String,
    pub system_path: Option<String>,
}

/// MCP 客户端状态统计。
#[derive(Debug, Clone, Default)]
pub struct McpByState {
    pub connected: usize,
    pub pending: usize,
    pub needs_auth: usize,
    pub failed: usize,
    pub disabled: usize,
}

/// 收集 doctor 诊断信息。
pub async fn get_doctor_diagnostic() -> DiagnosticInfo {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let invoked_binary = std::env::args()
        .next()
        .unwrap_or_else(|| "mossen".to_string());

    // 检查 ripgrep
    let rg_status = check_ripgrep_status().await;

    // 检查多安装
    let multiple_installations = detect_multiple_installations().await;

    // 检查更新权限
    let has_update_permissions = check_update_permissions(&exe_path).await;

    // 安装类型检测
    let installation_type = detect_installation_type(&exe_path);
    let config_install_method = detect_config_install_method();

    // 包管理器检测
    let package_manager = detect_package_manager(&exe_path);

    // 自动更新状态
    let auto_updates = if package_manager.is_some() {
        "Managed by package manager".to_string()
    } else {
        "enabled".to_string()
    };

    // 建议
    let recommendation = generate_recommendation(
        &installation_type,
        &multiple_installations,
        has_update_permissions,
    );

    DiagnosticInfo {
        version,
        installation_type,
        installation_path: exe_path,
        invoked_binary,
        config_install_method,
        package_manager,
        auto_updates,
        has_update_permissions,
        recommendation,
        multiple_installations,
        warnings: Vec::new(),
        ripgrep_status: rg_status,
        platform_runtime: serde_json::json!({}),
    }
}

/// 检查 ripgrep 可用性。
async fn check_ripgrep_status() -> RipgrepStatus {
    let rg_result = tokio::process::Command::new("rg")
        .arg("--version")
        .output()
        .await;

    match rg_result {
        Ok(output) if output.status.success() => {
            let path = which::which("rg")
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            RipgrepStatus {
                working: true,
                mode: if path.is_some() { "system".to_string() } else { "embedded".to_string() },
                system_path: path,
            }
        }
        _ => RipgrepStatus {
            working: false,
            mode: "missing".to_string(),
            system_path: None,
        },
    }
}

/// 检测多个安装。
async fn detect_multiple_installations() -> Vec<InstallationEntry> {
    let mut installations = Vec::new();

    // 搜索 PATH 中的 mossen 可执行文件
    let paths = std::env::var("PATH").unwrap_or_default();
    let mut seen = std::collections::HashSet::new();

    for dir in paths.split(':') {
        let candidate = PathBuf::from(dir).join("mossen");
        if candidate.exists() {
            let canonical = candidate
                .canonicalize()
                .unwrap_or_else(|_| candidate.clone());
            let key = canonical.to_string_lossy().to_string();
            if seen.insert(key.clone()) {
                installations.push(InstallationEntry {
                    install_type: "PATH".to_string(),
                    path: key,
                });
            }
        }
    }

    installations
}

/// 检查更新权限。
async fn check_update_permissions(exe_path: &str) -> Option<bool> {
    let path = Path::new(exe_path);
    if !path.exists() {
        return None;
    }
    // 尝试测试写权限
    let metadata = tokio::fs::metadata(path).await.ok()?;
    // Unix: 检查是否可写
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // nix::unistd::getuid() is safe and doesn't need unsafe
        let uid = nix::unistd::getuid();
        let file_uid = metadata.uid();
        Some(uid.as_raw() == 0 || uid.as_raw() == file_uid)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Some(true)
    }
}

/// 检测安装类型。
fn detect_installation_type(exe_path: &str) -> String {
    if exe_path.contains("homebrew") || exe_path.contains("Homebrew") {
        "homebrew".to_string()
    } else if exe_path.contains(".npm") || exe_path.contains("node_modules") {
        "npm".to_string()
    } else if exe_path.contains(".cargo") {
        "cargo".to_string()
    } else {
        "native".to_string()
    }
}

/// 检测配置安装方式。
fn detect_config_install_method() -> String {
    let config_dir = mossen_utils::env::get_mossen_config_home_dir();
    if config_dir.join("install-method").exists() {
        std::fs::read_to_string(config_dir.join("install-method"))
            .unwrap_or_else(|_| "unknown".to_string())
            .trim()
            .to_string()
    } else {
        "auto".to_string()
    }
}

/// 检测包管理器。
fn detect_package_manager(exe_path: &str) -> Option<String> {
    if exe_path.contains("homebrew") || exe_path.contains("Homebrew") {
        Some("homebrew".to_string())
    } else if exe_path.contains(".npm") {
        Some("npm".to_string())
    } else {
        None
    }
}

/// 生成建议。
fn generate_recommendation(
    installation_type: &str,
    multiple_installations: &[InstallationEntry],
    has_update_permissions: Option<bool>,
) -> Option<String> {
    if multiple_installations.len() > 1 {
        return Some(
            "Multiple installations detected. Consider removing duplicates.\n\
             Run `which -a mossen` to see all locations."
                .to_string(),
        );
    }
    if has_update_permissions == Some(false) {
        return Some(
            "Update permissions denied. You may need sudo for updates.\n\
             Consider reinstalling with proper permissions."
                .to_string(),
        );
    }
    if installation_type == "npm" {
        return Some(
            "npm installation detected. Consider using the native installer for better performance.\n\
             Visit https://mossen.ai/install for instructions."
                .to_string(),
        );
    }
    None
}

/// 格式化 doctor 输出。
pub fn format_doctor_output(diag: &DiagnosticInfo) -> String {
    let mut output = String::new();

    output.push_str(&format!("=== Diagnostics ===\n"));
    output.push_str(&format!(
        "Currently running: {} ({})\n",
        diag.installation_type, diag.version
    ));
    if let Some(ref pm) = diag.package_manager {
        output.push_str(&format!("Package manager: {}\n", pm));
    }
    output.push_str(&format!("Path: {}\n", diag.installation_path));
    output.push_str(&format!("Invoked: {}\n", diag.invoked_binary));
    output.push_str(&format!("Config install method: {}\n", diag.config_install_method));

    let rg_status = if diag.ripgrep_status.working { "OK" } else { "Not working" };
    let rg_mode = &diag.ripgrep_status.mode;
    output.push_str(&format!("Search: {} ({})\n", rg_status, rg_mode));

    output.push_str(&format!("\n=== Updates ===\n"));
    output.push_str(&format!("Auto-updates: {}\n", diag.auto_updates));
    if let Some(perms) = diag.has_update_permissions {
        output.push_str(&format!(
            "Update permissions: {}\n",
            if perms { "Yes" } else { "No (requires sudo)" }
        ));
    }

    if let Some(ref rec) = diag.recommendation {
        output.push_str(&format!("\n=== Recommendation ===\n{}\n", rec));
    }

    if diag.multiple_installations.len() > 1 {
        output.push_str("\n=== Warning: Multiple installations ===\n");
        for inst in &diag.multiple_installations {
            output.push_str(&format!("  {} at {}\n", inst.install_type, inst.path));
        }
    }

    for warning in &diag.warnings {
        output.push_str(&format!("\nWarning: {}\nFix: {}\n", warning.issue, warning.fix));
    }

    output
}

// ─── REPL 主循环 (screens/REPL.tsx 核心逻辑) ────────────────────────────────

/// REPL 状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplState {
    /// 等待用户输入。
    WaitingForInput,
    /// 正在处理查询。
    Processing,
    /// 显示权限请求。
    PermissionRequest,
    /// 成本阈值对话。
    CostThresholdDialog,
    /// 空闲返回对话。
    IdleReturnDialog,
    /// 选择消息（编辑模式）。
    MessageSelection,
    /// 完成。
    Done,
}

/// Spinner 显示模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpinnerMode {
    /// 正在思考…
    Thinking,
    /// 工具执行中 (附带名称)。
    Tool(String),
    /// 初始化中。
    Initializing,
}

/// REPL 配置。
#[derive(Debug, Clone)]
pub struct ReplScreenConfig {
    pub debug: bool,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub disable_slash_commands: bool,
    pub task_list_id: Option<String>,
    pub auto_connect_ide: bool,
    pub strict_mcp_config: bool,
}

/// 用户输入处理结果。
#[derive(Debug, Clone)]
pub enum InputResult {
    /// 提交查询给模型。
    Query(String),
    /// 执行斜杠命令。
    SlashCommand { name: String, args: String },
    /// 退出 REPL。
    Exit,
    /// 忽略（空输入等）。
    Ignore,
}

/// 解析用户输入。
pub fn parse_user_input(input: &str, disable_slash_commands: bool) -> InputResult {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return InputResult::Ignore;
    }

    // 退出命令
    if trimmed == "/exit" || trimmed == "/quit" || trimmed == "/q" {
        return InputResult::Exit;
    }

    // 斜杠命令
    if !disable_slash_commands && trimmed.starts_with('/') {
        let (name, args) = match trimmed.find(' ') {
            Some(pos) => (&trimmed[1..pos], trimmed[pos + 1..].trim()),
            None => (&trimmed[1..], ""),
        };
        return InputResult::SlashCommand {
            name: name.to_string(),
            args: args.to_string(),
        };
    }

    InputResult::Query(trimmed.to_string())
}

/// 可选择消息过滤器：仅保留用户消息。
pub fn selectable_user_messages_filter(msg: &serde_json::Value) -> bool {
    msg.get("role")
        .and_then(|r| r.as_str())
        .map(|r| r == "user")
        .unwrap_or(false)
}

/// 检查消息后续是否只有合成消息。
pub fn messages_after_are_only_synthetic(
    messages: &[serde_json::Value],
    index: usize,
) -> bool {
    messages[index + 1..].iter().all(|m| {
        m.get("synthetic")
            .and_then(|s| s.as_bool())
            .unwrap_or(false)
    })
}

/// Tab 状态类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabStatusKind {
    Idle,
    Working,
    Waiting,
    Done,
    Error,
}

/// 根据 REPL 状态确定 Tab 状态。
pub fn get_tab_status(state: &ReplState) -> TabStatusKind {
    match state {
        ReplState::WaitingForInput => TabStatusKind::Idle,
        ReplState::Processing => TabStatusKind::Working,
        ReplState::PermissionRequest | ReplState::CostThresholdDialog | ReplState::IdleReturnDialog => {
            TabStatusKind::Waiting
        }
        ReplState::MessageSelection => TabStatusKind::Idle,
        ReplState::Done => TabStatusKind::Done,
    }
}

/// 费用摘要。
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    pub total_cost_usd: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub total_tool_duration_ms: u64,
}

/// 格式化费用显示。
pub fn format_cost(usd: f64) -> String {
    if usd < 0.01 {
        format!("${:.4}", usd)
    } else if usd < 1.0 {
        format!("${:.2}", usd)
    } else {
        format!("${:.2}", usd)
    }
}

/// 格式化 token 数。
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// 截断字符串到指定宽度。
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        "...".to_string()
    } else {
        format!("{}...", &s[..max_width - 3])
    }
}
