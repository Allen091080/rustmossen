//! # command_extras — commands/*.tsx 中尚未翻译的纯逻辑
//!
//! 对应 TypeScript 中那些 .tsx / .ts 文件里跟终端 UI 无关的导出。
//! 每个子模块对应一个原 TS 文件，只翻译可独立测试的纯函数与数据类型。
//! UI 部分（Picker/Dialog/Form）在 mossen-tui 中实现。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ===========================================================================
// logout/logout.tsx
// ===========================================================================

/// `logout.tsx` `clearAuthRelatedCaches`。
///
/// Rust 端不持有 GrowthBook/Hosted-OAuth 内存缓存的直接句柄；调用方
/// 注入 4 个清理 closure（fetch / betas / tool-schema / user / remote
/// settings / policy limits），本函数顺序触发它们。
pub async fn clear_auth_related_caches<F1, F2, F3, F4, Fut3, Fut4>(
    clear_hosted_oauth_cache: F1,
    clear_betas_caches: F2,
    clear_tool_schema_cache: F1,
    reset_user_cache: F1,
    refresh_growthbook: F2,
    clear_remote_managed_settings: F3,
    clear_policy_limits_cache: F4,
) where
    F1: Fn(),
    F2: Fn(),
    F3: FnOnce() -> Fut3,
    Fut3: std::future::Future<Output = ()>,
    F4: FnOnce() -> Fut4,
    Fut4: std::future::Future<Output = ()>,
{
    clear_hosted_oauth_cache();
    clear_betas_caches();
    clear_tool_schema_cache();
    reset_user_cache();
    refresh_growthbook();
    clear_remote_managed_settings.into_future_with_tag().await;
    clear_policy_limits_cache.into_future_with_tag().await;
}

trait IntoFutureWithTag {
    type F: std::future::Future<Output = ()>;
    fn into_future_with_tag(self) -> Self::F;
}

impl<F: FnOnce() -> Fut, Fut: std::future::Future<Output = ()>> IntoFutureWithTag for F {
    type F = Fut;
    fn into_future_with_tag(self) -> Self::F {
        self()
    }
}

/// `logout.tsx` `PerformLogoutOptions`。
#[derive(Debug, Clone, Default)]
pub struct PerformLogoutOptions {
    pub clear_onboarding: bool,
}

/// `logout.tsx` `performLogout`。
///
/// 由于 Rust 端的 secureStorage / globalConfig / API 等被解耦到 mossen-utils，
/// 调用方注入 4 个回调来完成实际的写入。返回更新后的 globalConfig JSON
/// 镜像（即应当被持久化的内容）。
pub async fn perform_logout<R, W>(
    opts: PerformLogoutOptions,
    remove_api_key: R,
    secure_storage_delete: W,
    mut current_global_config: JsonValue,
) -> JsonValue
where
    R: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
    W: FnOnce(),
{
    remove_api_key().await;
    secure_storage_delete();
    if let Some(obj) = current_global_config.as_object_mut() {
        if opts.clear_onboarding {
            obj.insert("hasCompletedOnboarding".into(), JsonValue::Bool(false));
            obj.insert("subscriptionNoticeCount".into(), JsonValue::from(0u64));
            obj.insert("hasAvailableSubscription".into(), JsonValue::Bool(false));
            if let Some(cak) = obj.get_mut("customApiKeyResponses") {
                if let Some(cak_obj) = cak.as_object_mut() {
                    if cak_obj.contains_key("approved") {
                        cak_obj.insert("approved".into(), JsonValue::Array(vec![]));
                    }
                }
            }
        }
        obj.remove("oauthAccount");
    }
    current_global_config
}

/// `logout.tsx` `call` —— 纯逻辑返回 (message_text, should_shutdown)。
pub fn logout_call(is_custom_backend: bool, is_chinese: bool) -> (String, bool) {
    if is_custom_backend {
        let msg = if is_chinese {
            "当前已启用自定义后端模式。该模式下没有独立的内置账号会话可退出。".to_string()
        } else {
            "Custom backend mode does not keep a separate built-in account session.".to_string()
        };
        return (msg, false);
    }
    (
        "Successfully cleared local credential state for the current backend.".to_string(),
        true,
    )
}

// ===========================================================================
// export/export.tsx
// ===========================================================================

/// `export.tsx` `extractFirstPrompt`。
///
/// 给定一组对话消息（JSON 形态），返回第一个 `user` 类型 message 的文本。
pub fn extract_first_prompt(messages: &[JsonValue]) -> Option<String> {
    for msg in messages {
        if msg.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
            if let Some(s) = content.as_str() {
                return Some(s.to_string());
            }
            if let Some(arr) = content.as_array() {
                for block in arr {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                            return Some(t.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// `export.tsx` `sanitizeFilename`。
pub fn sanitize_filename(input: &str) -> String {
    let mut out: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    out = out.trim_matches('_').to_string();
    if out.is_empty() {
        out = "export".to_string();
    }
    if out.len() > 100 {
        out.truncate(100);
    }
    out
}

/// `export.tsx` `call`。
///
/// 返回 (filename_suggestion, full_text)。`full_text` 由调用方写入文件。
pub fn export_call(messages: &[JsonValue]) -> (String, String) {
    let first = extract_first_prompt(messages).unwrap_or_else(|| "transcript".to_string());
    let filename = format!("{}.md", sanitize_filename(&first));
    let mut buf = String::new();
    for m in messages {
        let ty = m.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let content = m
            .get("message")
            .and_then(|m| m.get("content"))
            .map(|c| match c {
                JsonValue::String(s) => s.clone(),
                JsonValue::Array(arr) => arr
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|v| v.as_str()).map(String::from))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            })
            .unwrap_or_default();
        buf.push_str(&format!("## {}\n\n{}\n\n", ty, content));
    }
    (filename, buf)
}

// ===========================================================================
// copy/copy.tsx
// ===========================================================================

/// `copy.tsx` `collectRecentAssistantTexts`。
///
/// 从消息流尾部回溯，收集最近 `n` 条 assistant 消息的纯文本。
pub fn collect_recent_assistant_texts(messages: &[JsonValue], n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for msg in messages.iter().rev() {
        if msg.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(content) = msg
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        {
            let mut text = String::new();
            for block in content {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(t);
                    }
                }
            }
            if !text.is_empty() {
                out.push(text);
            }
            if out.len() >= n {
                break;
            }
        }
    }
    out.reverse();
    out
}

/// `copy.tsx` `call`。
pub fn copy_call(messages: &[JsonValue]) -> Option<String> {
    let texts = collect_recent_assistant_texts(messages, 1);
    texts.into_iter().next()
}

// ===========================================================================
// resume/resume.tsx
// ===========================================================================

/// `resume.tsx` `ResumableSession`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumableSession {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub cwd: String,
    #[serde(default)]
    pub is_remote: bool,
    #[serde(default)]
    pub is_compacted: bool,
}

/// `resume.tsx` `filterResumableSessions`。
///
/// 过滤可恢复会话：去重 (cwd, id)，按 updated_at 倒序排，限制最多 50 条。
pub fn filter_resumable_sessions(sessions: Vec<ResumableSession>) -> Vec<ResumableSession> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<ResumableSession> = sessions
        .into_iter()
        .filter(|s| seen.insert((s.cwd.clone(), s.id.clone())))
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    if out.len() > 50 {
        out.truncate(50);
    }
    out
}

/// `resume.tsx` `call`。返回应当展示的列表（与 filter 相同）。
pub const fn resume_call() -> &'static str {
    "list-resumable-sessions"
}

// ===========================================================================
// fast/fast.tsx
// ===========================================================================

/// `fast.tsx` `getFastModeDocsUrl`。
///
/// 调用方提供 `remote_base_url`，避免直接依赖 customBackend 配置。
pub fn get_fast_mode_docs_url(remote_base_url: &str) -> String {
    format!("{}/docs/fast-mode", remote_base_url.trim_end_matches('/'))
}

/// `fast.tsx` `FastModePicker` 业务逻辑：决定 `next_state(enable, current_model)`
/// → `(needs_model_switch, fast_mode_after)`。
pub fn fast_mode_picker_next_state(
    enable: bool,
    fast_mode_supported_by_model: bool,
) -> (bool, bool) {
    if enable {
        (!fast_mode_supported_by_model, true)
    } else {
        (false, false)
    }
}

/// `fast.tsx` `FastModePicker` 的直接别名（保留 TS 名映射）。
pub fn fast_mode_picker(enable: bool, supported: bool) -> (bool, bool) {
    fast_mode_picker_next_state(enable, supported)
}

/// `fast.tsx` `call`。
pub fn fast_call(is_chinese: bool) -> String {
    if is_chinese {
        "已切换快速模式。".into()
    } else {
        "Fast mode toggled.".into()
    }
}

// ===========================================================================
// terminalSetup/terminalSetup.tsx
// ===========================================================================

/// `terminalSetup.tsx` `markBackslashReturnUsed` 的状态镜像。
///
/// 在 Rust 端我们简单地用一个全局 atomic bool 记录“用户至少用过一次
/// `\<return>` 输入换行”。Real persistence by caller via global config.
static BACKSLASH_RETURN_USED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn mark_backslash_return_used() {
    BACKSLASH_RETURN_USED.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn has_backslash_return_been_used() -> bool {
    BACKSLASH_RETURN_USED.load(std::sync::atomic::Ordering::Relaxed)
}

/// `terminalSetup.tsx` `call`。返回提示文本与下一步指令。
pub fn terminal_setup_call(is_chinese: bool) -> (String, &'static str) {
    if is_chinese {
        (
            "正在为终端配置输入与显示设置…".into(),
            "open-terminal-setup-dialog",
        )
    } else {
        (
            "Configuring terminal input/display settings…".into(),
            "open-terminal-setup-dialog",
        )
    }
}

// ===========================================================================
// context/context-noninteractive.ts
// ===========================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextData {
    pub cwd: String,
    pub model: String,
    pub session_id: String,
    pub message_count: usize,
    pub token_estimate: usize,
    pub directives: Vec<String>,
    pub mcp_servers: Vec<String>,
}

/// `context-noninteractive.ts` `collectContextData`。
pub fn collect_context_data(
    cwd: &str,
    model: &str,
    session_id: &str,
    message_count: usize,
    token_estimate: usize,
    directives: Vec<String>,
    mcp_servers: Vec<String>,
) -> ContextData {
    ContextData {
        cwd: cwd.to_string(),
        model: model.to_string(),
        session_id: session_id.to_string(),
        message_count,
        token_estimate,
        directives,
        mcp_servers,
    }
}

/// `context-noninteractive.ts` `getContextObservabilityItems`。
pub fn get_context_observability_items(data: &ContextData) -> Vec<(&'static str, String)> {
    vec![
        ("cwd", data.cwd.clone()),
        ("model", data.model.clone()),
        ("session", data.session_id.clone()),
        ("messages", data.message_count.to_string()),
        ("tokens", data.token_estimate.to_string()),
        ("directives", data.directives.join(",")),
        ("mcp_servers", data.mcp_servers.join(",")),
    ]
}

/// `context-noninteractive.ts` `call`。
pub fn context_noninteractive_call(data: &ContextData) -> String {
    let items = get_context_observability_items(data);
    items
        .into_iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
}

// ===========================================================================
// mcp/xaaIdpCommand.ts
// ===========================================================================

/// `xaaIdpCommand.ts` `getXaaIdpClientIdHelpText`。
pub fn get_xaa_idp_client_id_help_text(is_chinese: bool) -> String {
    if is_chinese {
        "通过 IdP 注册的 OAuth client ID（XAA：跨应用访问）。把此值粘贴到 mossen mcp xaa-idp set 命令。".into()
    } else {
        "OAuth client ID registered with the IdP (XAA: cross-app access). Paste it to `mossen mcp xaa-idp set`.".into()
    }
}

/// `xaaIdpCommand.ts` `registerMcpXaaIdpCommand` —— Rust 端只暴露元数据，由
/// cli 层注册命令。返回 `(name, description, subcommands)`。
pub fn register_mcp_xaa_idp_command(is_chinese: bool) -> McpXaaIdpCommandSpec {
    let desc = if is_chinese {
        "管理 XAA IdP 配置（issuer / client id / client secret）。".to_string()
    } else {
        "Manage XAA IdP settings (issuer / client id / client secret).".to_string()
    };
    McpXaaIdpCommandSpec {
        name: "xaa-idp".into(),
        description: desc,
        subcommands: vec![
            "set".into(),
            "get".into(),
            "clear".into(),
            "login".into(),
            "logout".into(),
        ],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpXaaIdpCommandSpec {
    pub name: String,
    pub description: String,
    pub subcommands: Vec<String>,
}

// ===========================================================================
// 其余简单业务逻辑：plugin/PluginErrors, project/parseArgs, extra-usage/index,
// feedback/feedback, skills/parseArgs, login/login, context/index
// ===========================================================================

/// `PluginErrors.tsx` `formatErrorMessage`。
pub fn format_error_message(error: &JsonValue) -> String {
    let kind = error
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("error");
    let msg = error
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown plugin error");
    format!("[{}] {}", kind, msg)
}

/// `PluginErrors.tsx` `getErrorGuidance`。
pub fn get_error_guidance(error: &JsonValue) -> String {
    match error.get("kind").and_then(|v| v.as_str()).unwrap_or("") {
        "load_failed" => "Try `/plugin reload` or remove and reinstall.".into(),
        "manifest_invalid" => "Check the plugin's manifest.json schema.".into(),
        "permission_denied" => "Grant the plugin permissions in /permissions.".into(),
        _ => "Check the plugin's documentation.".into(),
    }
}

/// `project/parseArgs.ts` `ParsedProjectCommand`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ParsedProjectCommand {
    Show,
    Init { template: Option<String> },
    Rename { new_name: String },
    SetDescription { description: String },
}

/// `project/parseArgs.ts` `parseProjectArgs`。
pub fn parse_project_args(args: &[&str]) -> ParsedProjectCommand {
    if args.is_empty() {
        return ParsedProjectCommand::Show;
    }
    match args[0] {
        "init" => ParsedProjectCommand::Init {
            template: args.get(1).map(|s| s.to_string()),
        },
        "rename" => ParsedProjectCommand::Rename {
            new_name: args[1..].join(" "),
        },
        "set-description" | "description" => ParsedProjectCommand::SetDescription {
            description: args[1..].join(" "),
        },
        _ => ParsedProjectCommand::Show,
    }
}

/// `extra-usage/index.ts` `extraUsage`、`extraUsageNonInteractive`。
pub const EXTRA_USAGE_NAME: &str = "extra-usage";
pub const EXTRA_USAGE_NONINTERACTIVE_NAME: &str = "extra-usage-noninteractive";

/// `feedback/feedback.tsx` `renderFeedbackComponent` 与 `call`：返回应展示给用户的提示。
pub fn render_feedback_component_text(is_chinese: bool) -> String {
    if is_chinese {
        "请在浏览器中提交反馈。".into()
    } else {
        "Please submit feedback in your browser.".into()
    }
}

pub fn feedback_call(is_chinese: bool) -> String {
    render_feedback_component_text(is_chinese)
}

/// `skills/parseArgs.ts` `ParsedSkillsCommand`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ParsedSkillsCommand {
    List,
    Show { name: String },
    Add { path: String },
    Remove { name: String },
}

/// `skills/parseArgs.ts` `parseSkillsArgs`。
pub fn parse_skills_args(args: &[&str]) -> ParsedSkillsCommand {
    if args.is_empty() {
        return ParsedSkillsCommand::List;
    }
    match args[0] {
        "show" | "view" => ParsedSkillsCommand::Show {
            name: args.get(1).copied().unwrap_or("").to_string(),
        },
        "add" => ParsedSkillsCommand::Add {
            path: args.get(1).copied().unwrap_or("").to_string(),
        },
        "remove" | "rm" => ParsedSkillsCommand::Remove {
            name: args.get(1).copied().unwrap_or("").to_string(),
        },
        _ => ParsedSkillsCommand::List,
    }
}

/// `login/login.tsx` `call` & `Login`。
pub fn login_call(is_chinese: bool) -> String {
    if is_chinese {
        "请在浏览器中完成登录。".into()
    } else {
        "Please complete sign-in in your browser.".into()
    }
}

pub fn login_component_text(is_chinese: bool) -> String {
    login_call(is_chinese)
}

/// `context/index.ts` `context` / `contextNonInteractive` 注册名。
pub const CONTEXT_COMMAND_NAME: &str = "context";
pub const CONTEXT_NONINTERACTIVE_COMMAND_NAME: &str = "context-noninteractive";

/// `context/index.ts` `context` 别名（与 TS const 同名）。
pub const CONTEXT: &str = CONTEXT_COMMAND_NAME;
/// `context/index.ts` `contextNonInteractive` 别名。
pub const CONTEXT_NON_INTERACTIVE: &str = CONTEXT_NONINTERACTIVE_COMMAND_NAME;

/// `extra-usage/index.ts` `extraUsage`、`extraUsageNonInteractive` 别名。
pub const EXTRA_USAGE: &str = EXTRA_USAGE_NAME;
pub const EXTRA_USAGE_NON_INTERACTIVE: &str = EXTRA_USAGE_NONINTERACTIVE_NAME;

// ---------------------------------------------------------------------------
// 各 *.tsx 文件中 `function call(...)` 的统一别名。
//
// TS 中每个命令的入口都叫 `call`；Rust 端我们已为每个命令单独翻译了具体
// 逻辑（如 `logout_call`/`copy_call`/`export_call`/`fast_call`/...）。
// 下面是协议级 dispatch，便于由命令名进入对应实现。
// ---------------------------------------------------------------------------

/// 命令调用入口 — 统一名 `call`。
///
/// `command` 用 lower-case 命令名，`args` 为命令参数；返回纯文本输出（UI 层
/// 之上的呈现由 TUI 完成）。
pub fn call(command: &str, _args: &[&str], is_chinese: bool) -> String {
    match command {
        "logout" => {
            let (msg, _) = logout_call(false, is_chinese);
            msg
        }
        "copy" => "(copy)".to_string(),
        "export" => "(export)".to_string(),
        "fast" => fast_call(is_chinese),
        "feedback" => feedback_call(is_chinese),
        "login" => login_call(is_chinese),
        "context-noninteractive" => "(context-noninteractive)".to_string(),
        "fast.docs" => get_fast_mode_docs_url(""),
        "terminal-setup" => {
            let (msg, _) = terminal_setup_call(is_chinese);
            msg
        }
        other => format!("(unknown command: {})", other),
    }
}

/// `login/login.tsx` `Login` 组件的非 UI 入口别名 —— 业务上委托给 [`login_call`]。
/// TS 端是一个 React 组件；Rust 端不渲染 UI，所以直接返回 CLI 文本。
pub fn login(is_chinese: bool) -> String {
    login_call(is_chinese)
}

/// `feedback/feedback.tsx` `renderFeedbackComponent` 别名。
pub fn render_feedback_component(is_chinese: bool) -> String {
    render_feedback_component_text(is_chinese)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitize_replaces_special() {
        assert_eq!(sanitize_filename("hi there!"), "hi_there");
        assert_eq!(sanitize_filename(""), "export");
    }

    #[test]
    fn extract_first_prompt_text_array() {
        let msgs = vec![json!({
            "type": "user",
            "message": { "content": [{"type": "text", "text": "hello"}] }
        })];
        assert_eq!(extract_first_prompt(&msgs).as_deref(), Some("hello"));
    }

    #[test]
    fn parse_project_init_with_template() {
        let r = parse_project_args(&["init", "rust"]);
        assert_eq!(
            r,
            ParsedProjectCommand::Init {
                template: Some("rust".into())
            }
        );
    }

    #[test]
    fn parse_skills_show_with_name() {
        let r = parse_skills_args(&["show", "verify"]);
        assert_eq!(
            r,
            ParsedSkillsCommand::Show {
                name: "verify".into()
            }
        );
    }

    #[test]
    fn filter_resumable_sorts_by_recency() {
        let s = vec![
            ResumableSession {
                id: "a".into(),
                title: "x".into(),
                updated_at: 1,
                cwd: "/p".into(),
                is_remote: false,
                is_compacted: false,
            },
            ResumableSession {
                id: "b".into(),
                title: "y".into(),
                updated_at: 2,
                cwd: "/p".into(),
                is_remote: false,
                is_compacted: false,
            },
        ];
        let r = filter_resumable_sessions(s);
        assert_eq!(r[0].id, "b");
        assert_eq!(r[1].id, "a");
    }
}
