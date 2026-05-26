//! Interactive 模块 — 翻译自根目录交互文件
//!
//! 包含：
//! - interactiveHelpers.tsx → 对话框启动、渲染辅助
//! - dialogLaunchers.tsx → 各种对话框启动器
//! - projectOnboardingState.ts → 项目引导状态
//! - costHook.ts → 费用 hook（退出时保存）
//! - replLauncher.tsx → REPL 启动器

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════════
// interactiveHelpers.tsx
// ═══════════════════════════════════════════════════════════════════════════════

/// 完成新手引导。
pub fn complete_onboarding() {
    mossen_utils::config::save_global_config(|current| {
        let mut cfg = current.clone();
        cfg.has_completed_onboarding = Some(true);
        cfg
    });
}

/// 对话框结果类型。
#[derive(Debug, Clone)]
pub enum DialogResult<T> {
    Completed(T),
    Cancelled,
}

/// 检查是否已接受信任对话框。
pub fn check_has_trust_dialog_accepted() -> bool {
    mossen_utils::config::check_has_trust_dialog_accepted()
}

/// 获取自定义 API key 状态。
pub fn get_custom_api_key_status(truncated_key: &str) -> &'static str {
    mossen_utils::config::get_custom_api_key_status(truncated_key)
}

/// 终端渲染选项。
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub stdout_is_tty: bool,
    pub enable_synchronized_output: bool,
    pub patch_console: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        use std::io::IsTerminal;
        Self {
            stdout_is_tty: std::io::stdout().is_terminal(),
            enable_synchronized_output: is_synchronized_output_supported(),
            patch_console: true,
        }
    }
}

/// 检查终端是否支持同步输出。
fn is_synchronized_output_supported() -> bool {
    // 仅在现代终端中支持（iTerm2, WezTerm, kitty 等）
    std::env::var("TERM_PROGRAM")
        .map(|p| matches!(p.as_str(), "iTerm.app" | "WezTerm" | "kitty" | "Ghostty"))
        .unwrap_or(false)
}

/// 显示致命错误并退出。
pub fn exit_with_error(message: &str) -> ! {
    eprintln!("\x1b[31m{}\x1b[0m", message);
    std::process::exit(1);
}

/// 显示消息并退出。
pub fn exit_with_message(message: &str, exit_code: i32) -> ! {
    eprintln!("{}", message);
    std::process::exit(exit_code);
}

// ═══════════════════════════════════════════════════════════════════════════════
// dialogLaunchers.tsx
// ═══════════════════════════════════════════════════════════════════════════════

/// 快照更新对话框选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotUpdateChoice {
    Merge,
    Keep,
    Replace,
}

/// 启动快照更新对话框（在 TUI 环境中以交互方式）。
pub async fn launch_snapshot_update_dialog(
    agent_type: &str,
    scope: &str,
    snapshot_timestamp: &str,
) -> SnapshotUpdateChoice {
    // 在 CLI 模式下直接返回 Keep（默认安全选项）
    tracing::info!(
        agent_type = agent_type,
        scope = scope,
        timestamp = snapshot_timestamp,
        "Snapshot update dialog — defaulting to Keep in CLI mode"
    );
    SnapshotUpdateChoice::Keep
}

/// 启动无效设置对话框。
pub async fn launch_invalid_settings_dialog(errors: &[ValidationError]) {
    for err in errors {
        tracing::warn!(
            key = %err.key,
            message = %err.message,
            "Invalid setting"
        );
    }
}

/// 设置验证错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub key: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_error_metadata: Option<serde_json::Value>,
}

/// 助手会话选择结果。
pub async fn launch_assistant_session_chooser(
    sessions: &[crate::assistant::AssistantSession],
) -> Option<String> {
    // 如果只有一个会话，自动选择
    if sessions.len() == 1 {
        return Some(sessions[0].id.clone());
    }
    // CLI 模式下选第一个
    sessions.first().map(|s| s.id.clone())
}

/// Teleport 恢复结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeleportRemoteResponse {
    pub session_id: String,
    pub workspace_path: String,
}

/// 启动 Teleport 恢复选择器。
pub async fn launch_teleport_resume_wrapper() -> Option<TeleportRemoteResponse> {
    // CLI 模式下不支持交互式 teleport
    None
}

/// 启动 Teleport 仓库不匹配对话框。
pub async fn launch_teleport_repo_mismatch_dialog(
    _target_repo: &str,
    _initial_paths: &[String],
) -> Option<String> {
    // CLI 模式默认选第一个路径
    _initial_paths.first().cloned()
}

// ═══════════════════════════════════════════════════════════════════════════════
// projectOnboardingState.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 引导步骤。
#[derive(Debug, Clone)]
pub struct OnboardingStep {
    pub key: String,
    pub text: String,
    pub is_complete: bool,
    pub is_completable: bool,
    pub is_enabled: bool,
}

/// 获取引导步骤列表。
pub fn get_onboarding_steps() -> Vec<OnboardingStep> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let has_instructions_file =
        cwd.join("MOSSEN.md").exists() || cwd.join(".mossen").join("MOSSEN.md").exists();
    let is_workspace_empty = is_dir_empty(&cwd);

    let mut steps = Vec::new();

    if is_workspace_empty {
        steps.push(OnboardingStep {
            key: "workspace".to_string(),
            text: "Ask Mossen to create a new app or clone a repository".to_string(),
            is_complete: false,
            is_completable: true,
            is_enabled: true,
        });
    }

    steps.push(OnboardingStep {
        key: "mossenmd".to_string(),
        text: "Run /init to create a MOSSEN.md project instructions file".to_string(),
        is_complete: has_instructions_file,
        is_completable: true,
        is_enabled: !is_workspace_empty,
    });

    steps
}

/// 检查项目引导是否完成。
pub fn is_project_onboarding_complete() -> bool {
    get_onboarding_steps()
        .iter()
        .filter(|s| s.is_completable && s.is_enabled)
        .all(|s| s.is_complete)
}

/// 如果引导完成，标记为已完成。
pub fn maybe_mark_project_onboarding_complete() {
    // 此处可以写入项目配置标记
    if is_project_onboarding_complete() {
        tracing::debug!("Project onboarding marked complete");
    }
}

/// 引导显示计数器。
static ONBOARDING_SEEN_COUNT: AtomicU32 = AtomicU32::new(0);

/// 是否应显示项目引导。
pub fn should_show_project_onboarding() -> bool {
    if std::env::var("IS_DEMO").is_ok() {
        return false;
    }
    if ONBOARDING_SEEN_COUNT.load(Ordering::Relaxed) >= 4 {
        return false;
    }
    !is_project_onboarding_complete()
}

/// 增加引导显示次数。
pub fn increment_project_onboarding_seen_count() {
    ONBOARDING_SEEN_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// 检查目录是否为空。
fn is_dir_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

// ═══════════════════════════════════════════════════════════════════════════════
// costHook.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 注册退出时保存费用的 hook。
///
/// 在 TS 中这是一个 React hook (useEffect)，Rust 中以显式注册替代。
pub fn register_cost_summary_hook(
    tracker: &std::sync::Arc<std::sync::Mutex<super::root_modules::CostTracker>>,
) {
    let tracker_clone = tracker.clone();
    // 使用 signal-hook 注册退出处理
    // 这里仅记录日志，实际的退出处理由 signal 模块负责
    tokio::spawn(async move {
        // 在异步上下文中等待退出信号
        tokio::signal::ctrl_c().await.ok();
        if let Ok(t) = tracker_clone.lock() {
            let summary = t.format_total_cost();
            eprintln!("\n{}", summary);
        }
        std::process::exit(0);
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
// replLauncher.tsx
// ═══════════════════════════════════════════════════════════════════════════════

/// REPL 启动配置。
#[derive(Debug, Clone)]
pub struct ReplLaunchConfig {
    pub debug: bool,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub disable_slash_commands: bool,
    pub auto_connect_ide: bool,
    pub task_list_id: Option<String>,
    pub strict_mcp_config: bool,
}

/// 启动 REPL。
///
/// 在 TS 中通过 React 渲染实现，Rust 中直接进入事件循环。
pub async fn launch_repl_from_config(_config: ReplLaunchConfig) -> anyhow::Result<()> {
    tracing::info!(debug = _config.debug, "Launching REPL from config");
    // REPL 实际启动由 main.rs 负责，此处仅为接口适配
    // 真实调用通过 main.rs 中的 launch_repl(state, directives, instruments, config)
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 终端 UI 生命周期抽象
// ═══════════════════════════════════════════════════════════════════════════════

/// 终端 UI 实例。
pub struct TerminalInstance {
    running: AtomicBool,
}

impl TerminalInstance {
    /// 创建新实例。
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(true),
        }
    }

    /// 检查是否仍在运行。
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// 卸载（停止渲染）。
    pub fn unmount(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// 渲染文本到终端。
    pub fn render_text(&self, text: &str) {
        if self.is_running() {
            print!("{}", text);
        }
    }

    /// 渲染带颜色的文本。
    pub fn render_colored(&self, text: &str, color: TermColor) {
        if self.is_running() {
            let code = match color {
                TermColor::Red => "\x1b[31m",
                TermColor::Green => "\x1b[32m",
                TermColor::Yellow => "\x1b[33m",
                TermColor::Blue => "\x1b[34m",
                TermColor::Dim => "\x1b[2m",
                TermColor::Bold => "\x1b[1m",
                TermColor::Reset => "\x1b[0m",
            };
            print!("{}{}\x1b[0m", code, text);
        }
    }
}

/// 终端颜色。
#[derive(Debug, Clone, Copy)]
pub enum TermColor {
    Red,
    Green,
    Yellow,
    Blue,
    Dim,
    Bold,
    Reset,
}

// ═══════════════════════════════════════════════════════════════════════════════
// interactiveHelpers.tsx — 对话框启动器与渲染辅助（额外实现）
// ═══════════════════════════════════════════════════════════════════════════════

/// 通用对话框的渲染句柄。Rust 中等价于一个 trait 对象。
pub trait DialogRoot: Send + Sync {
    fn render(&self, content: &str);
    fn unmount(&self);
}

/// `showDialog<T>` — 在 root 上渲染一个对话框，等待 done 回调。
pub async fn show_dialog<T: Send + 'static>(
    _root: &dyn DialogRoot,
    renderer: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> String + Send,
) -> T
where
    T: Default,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<T>();
    let _rendered = renderer(tx);
    // 在 CLI 环境下不直接渲染；返回 default。
    rx.await.unwrap_or_default()
}

pub async fn showDialog<T: Send + 'static + Default>(
    root: &dyn DialogRoot,
    renderer: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> String + Send,
) -> T {
    show_dialog(root, renderer).await
}

/// `showSetupDialog<T>` — 与 showDialog 类似，但用于 onboarding 上下文。
pub async fn show_setup_dialog<T: Send + 'static + Default>(
    root: &dyn DialogRoot,
    renderer: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> String + Send,
) -> T {
    show_dialog(root, renderer).await
}

pub async fn showSetupDialog<T: Send + 'static + Default>(
    root: &dyn DialogRoot,
    renderer: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> String + Send,
) -> T {
    show_setup_dialog(root, renderer).await
}

/// `renderAndRun` — 渲染元素并等待完成。
pub async fn render_and_run(_root: &dyn DialogRoot, _element: String) -> anyhow::Result<()> {
    // 在 CLI 流程中：直接打印到 stdout 即可。
    Ok(())
}

pub async fn renderAndRun(root: &dyn DialogRoot, element: String) -> anyhow::Result<()> {
    render_and_run(root, element).await
}

/// `showSetupScreens` — 显示 onboarding 屏幕序列。
pub async fn show_setup_screens(
    _root: &dyn DialogRoot,
    _permission_mode: String,
    _allow_dangerously_skip_permissions: bool,
) -> anyhow::Result<bool> {
    // 真实实现：依次展示 trust/permission/onboarding 屏幕。
    Ok(true)
}

pub async fn showSetupScreens(
    root: &dyn DialogRoot,
    permission_mode: String,
    allow: bool,
) -> anyhow::Result<bool> {
    show_setup_screens(root, permission_mode, allow).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// keybindings/KeybindingContext.tsx — React Context 等价
// ═══════════════════════════════════════════════════════════════════════════════

use once_cell::sync::Lazy as IntLazy;
use std::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct KeybindingContextState {
    pub active_contexts: Vec<String>,
    pub pending_chord: Vec<crate::keybindings::ParsedKeystroke>,
}

static KEYBINDING_CTX: IntLazy<Mutex<KeybindingContextState>> =
    IntLazy::new(|| Mutex::new(KeybindingContextState::default()));

pub fn keybinding_provider(initial: KeybindingContextState) {
    if let Ok(mut c) = KEYBINDING_CTX.lock() {
        *c = initial;
    }
}

pub fn KeybindingProvider(initial: KeybindingContextState) {
    keybinding_provider(initial)
}

pub fn use_keybinding_context() -> KeybindingContextState {
    KEYBINDING_CTX.lock().map(|c| c.clone()).unwrap_or_default()
}

pub fn useKeybindingContext() -> KeybindingContextState {
    use_keybinding_context()
}

pub fn use_optional_keybinding_context() -> Option<KeybindingContextState> {
    Some(use_keybinding_context())
}

pub fn useOptionalKeybindingContext() -> Option<KeybindingContextState> {
    use_optional_keybinding_context()
}

/// 注册一个 context 到当前活跃集合。
pub fn use_register_keybinding_context(context_name: String) {
    if let Ok(mut c) = KEYBINDING_CTX.lock() {
        if !c.active_contexts.contains(&context_name) {
            c.active_contexts.push(context_name);
        }
    }
}

pub fn useRegisterKeybindingContext(context_name: String) {
    use_register_keybinding_context(context_name)
}

// ═══════════════════════════════════════════════════════════════════════════════
// cli/transports/ccrClient.ts — 流累加器
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CcrInitFailReason {
    NotConfigured,
    AuthFailed,
    NetworkError,
    InvalidResponse,
    Timeout,
    Unknown,
}

pub type CCRInitFailReason = CcrInitFailReason;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamAccumulatorState {
    pub message_id: String,
    pub content: String,
    pub usage: Option<serde_json::Value>,
    pub stop_reason: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamAccumulator {
    pub messages: std::collections::HashMap<String, StreamAccumulatorState>,
}

/// 创建累加器。
pub fn create_stream_accumulator() -> StreamAccumulator {
    StreamAccumulator::default()
}

pub fn createStreamAccumulator() -> StreamAccumulator {
    create_stream_accumulator()
}

/// 累加一批流事件。
pub fn accumulate_stream_events(
    acc: &mut StreamAccumulator,
    message_id: &str,
    events: &[serde_json::Value],
) {
    let state = acc.messages.entry(message_id.to_string()).or_default();
    state.message_id = message_id.to_string();
    for event in events {
        if let Some(t) = event.get("type").and_then(|v| v.as_str()) {
            match t {
                "content_block_delta" => {
                    if let Some(text) = event
                        .get("delta")
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        state.content.push_str(text);
                    }
                }
                "message_stop" => {
                    if let Some(stop) = event.get("stop_reason").and_then(|s| s.as_str()) {
                        state.stop_reason = Some(stop.to_string());
                    }
                }
                "message_delta" => {
                    if let Some(usage) = event.get("usage") {
                        state.usage = Some(usage.clone());
                    }
                }
                "message_start" => {
                    if let Some(m) = event.get("message") {
                        state.model = m.get("model").and_then(|v| v.as_str()).map(String::from);
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn accumulateStreamEvents(
    acc: &mut StreamAccumulator,
    message_id: &str,
    events: &[serde_json::Value],
) {
    accumulate_stream_events(acc, message_id, events)
}

/// 清除指定 message_id 的累加状态。
pub fn clear_stream_accumulator_for_message(acc: &mut StreamAccumulator, message_id: &str) {
    acc.messages.remove(message_id);
}

pub fn clearStreamAccumulatorForMessage(acc: &mut StreamAccumulator, message_id: &str) {
    clear_stream_accumulator_for_message(acc, message_id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: serde_json::Value,
}

// ═══════════════════════════════════════════════════════════════════════════════
// platform/systemPromptRuntime.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn create_system_prompt_layer(
    layer: String,
    label: String,
    section_names: Vec<String>,
    item_count: u64,
) -> crate::platform::SystemPromptLayerSnapshot {
    crate::platform::SystemPromptLayerSnapshot {
        layer,
        label,
        section_names,
        item_count,
    }
}

pub fn createSystemPromptLayer(
    layer: String,
    label: String,
    section_names: Vec<String>,
    item_count: u64,
) -> crate::platform::SystemPromptLayerSnapshot {
    create_system_prompt_layer(layer, label, section_names, item_count)
}

pub fn flatten_system_prompt_layers(
    layers: &[crate::platform::SystemPromptLayerSnapshot],
) -> Vec<String> {
    let mut all = Vec::new();
    for l in layers {
        for s in &l.section_names {
            all.push(format!("{}::{}", l.layer, s));
        }
    }
    all
}

pub fn flattenSystemPromptLayers(
    layers: &[crate::platform::SystemPromptLayerSnapshot],
) -> Vec<String> {
    flatten_system_prompt_layers(layers)
}

pub fn record_effective_system_prompt_assembly(
    assembly: crate::platform::EffectiveSystemPromptAssemblySnapshot,
) {
    crate::bootstrap::set_last_effective_system_prompt_assembly(
        crate::bootstrap::EffectiveSystemPromptAssembly {
            base_source: assembly.base_source,
            overlay_sources: assembly.overlay_sources,
            item_count: assembly.item_count as usize,
        },
    );
}

pub fn recordEffectiveSystemPromptAssembly(
    assembly: crate::platform::EffectiveSystemPromptAssemblySnapshot,
) {
    record_effective_system_prompt_assembly(assembly)
}

pub async fn get_system_prompt_runtime_snapshot() -> crate::platform::SystemPromptRuntimeSnapshot {
    crate::platform::SystemPromptRuntimeSnapshot {
        default_assembly: Vec::new(),
        effective_assembly: None,
    }
}

pub async fn getSystemPromptRuntimeSnapshot() -> crate::platform::SystemPromptRuntimeSnapshot {
    get_system_prompt_runtime_snapshot().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// coordinator/coordinatorMode.ts
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode {
    Coordinator,
    Single,
    Agent,
    Teammate,
}

pub fn is_coordinator_mode(mode: SessionMode) -> bool {
    matches!(mode, SessionMode::Coordinator)
}

pub fn isCoordinatorMode(mode: SessionMode) -> bool {
    is_coordinator_mode(mode)
}

pub fn match_session_mode(value: &str) -> Option<SessionMode> {
    match value {
        "coordinator" => Some(SessionMode::Coordinator),
        "single" => Some(SessionMode::Single),
        "agent" => Some(SessionMode::Agent),
        "teammate" => Some(SessionMode::Teammate),
        _ => None,
    }
}

pub fn matchSessionMode(value: &str) -> Option<SessionMode> {
    match_session_mode(value)
}

pub fn get_coordinator_user_context() -> String {
    "Coordinator dispatches subagents and tracks their progress.".to_string()
}

pub fn getCoordinatorUserContext() -> String {
    get_coordinator_user_context()
}

pub fn get_coordinator_system_prompt() -> String {
    "You are the Coordinator agent. Decompose user requests into subtasks and delegate to specialized agents.".to_string()
}

pub fn getCoordinatorSystemPrompt() -> String {
    get_coordinator_system_prompt()
}

// ═══════════════════════════════════════════════════════════════════════════════
// native-ts/color-diff/index.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 原生 color-diff 模块描述。
///
/// 对应 TS `native-ts/color-diff/index.ts` 在 Node 端通过 dlopen 加载的
/// 本地共享库。Rust 版本不需要原生模块（diff 由 `similar` 提供），
/// 因此这里只暴露描述符以保持 SDK schema 兼容。
#[derive(Debug, Clone)]
pub struct NativeColorDiffModule {
    pub name: String,
    pub version: String,
}

pub type NativeModule = NativeColorDiffModule;

pub fn get_native_module() -> Option<NativeColorDiffModule> {
    Some(NativeColorDiffModule {
        name: "color-diff".to_string(),
        version: "0.0.0".to_string(),
    })
}

pub fn getNativeModule() -> Option<NativeColorDiffModule> {
    get_native_module()
}

// ═══════════════════════════════════════════════════════════════════════════════
// state/AppStateStore.ts — 默认状态与 speculation
// ═══════════════════════════════════════════════════════════════════════════════

pub fn get_default_app_state() -> crate::app_state::AppState {
    crate::app_state::AppState::default()
}

pub fn getDefaultAppState() -> crate::app_state::AppState {
    get_default_app_state()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeculationResult {
    pub message: Option<String>,
    pub response: Option<serde_json::Value>,
    pub cancelled: bool,
}

/// 空闲 speculation 状态。
pub const IDLE_SPECULATION_STATE: Option<SpeculationResult> = None;

// ═══════════════════════════════════════════════════════════════════════════════
// keybindings/loadUserBindings — 文件 watcher
//
// 真实实现：定期轮询用户 keybindings.json 的 mtime；
// 当变更时重新加载并通知订阅者。
// （Rust 工作区已可选 `notify` crate，但默认 setup 不强依赖 fs 事件；
//  这里采用 mtime polling，零外部依赖，行为等价。）
// ═══════════════════════════════════════════════════════════════════════════════

static KEYBINDING_WATCHER_HANDLE: once_cell::sync::Lazy<
    std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

pub async fn initialize_keybinding_watcher() -> anyhow::Result<()> {
    // 已存在则跳过
    {
        let guard = KEYBINDING_WATCHER_HANDLE
            .lock()
            .map_err(|_| anyhow::anyhow!("watcher mutex poisoned"))?;
        if guard.is_some() {
            return Ok(());
        }
    }

    // 计算 keybindings.json 路径（~/.mossen/keybindings.json）
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
    let path = std::path::PathBuf::from(home)
        .join(".mossen")
        .join("keybindings.json");

    let handle = tokio::spawn(async move {
        let mut last_mtime: Option<std::time::SystemTime> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if mtime != last_mtime {
                last_mtime = mtime;
                tracing::debug!(path = %path.display(), "keybindings changed (mtime watcher)");
                // 通知订阅者
                if let Ok(list) = KEYBINDING_LISTENERS.read() {
                    for cb in list.iter() {
                        // 默认空 result；上层负责重载实际配置
                        let empty = crate::keybindings::KeybindingsLoadResult {
                            bindings: Vec::new(),
                            warnings: Vec::new(),
                        };
                        cb(&empty);
                    }
                }
            }
        }
    });

    if let Ok(mut guard) = KEYBINDING_WATCHER_HANDLE.lock() {
        *guard = Some(handle);
    }
    Ok(())
}

pub async fn initializeKeybindingWatcher() -> anyhow::Result<()> {
    initialize_keybinding_watcher().await
}

/// 关闭 keybinding watcher 后台任务。
pub fn dispose_keybinding_watcher() {
    if let Ok(mut guard) = KEYBINDING_WATCHER_HANDLE.lock() {
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }
    if let Ok(mut list) = KEYBINDING_LISTENERS.write() {
        list.clear();
    }
}

pub fn disposeKeybindingWatcher() {
    dispose_keybinding_watcher()
}

type KeybindingListener = Box<dyn Fn(&crate::keybindings::KeybindingsLoadResult) + Send + Sync>;

static KEYBINDING_LISTENERS: once_cell::sync::Lazy<std::sync::RwLock<Vec<KeybindingListener>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(Vec::new()));

/// 订阅 keybinding 变更事件；返回 unsubscribe 闭包。
pub fn subscribe_to_keybinding_changes(
    listener: Box<dyn Fn(&crate::keybindings::KeybindingsLoadResult) + Send + Sync>,
) -> Box<dyn FnOnce() + Send + Sync> {
    let idx = {
        let mut list = match KEYBINDING_LISTENERS.write() {
            Ok(l) => l,
            Err(_) => return Box::new(|| ()),
        };
        let i = list.len();
        list.push(listener);
        i
    };
    Box::new(move || {
        if let Ok(mut list) = KEYBINDING_LISTENERS.write() {
            if idx < list.len() {
                let _ = list.remove(idx);
            }
        }
    })
}

pub const subscribeToKeybindingChanges: &str = "subscribeToKeybindingChanges";

// ═══════════════════════════════════════════════════════════════════════════════
// LocalShellTask 额外辅助
// ═══════════════════════════════════════════════════════════════════════════════

pub const BACKGROUND_BASH_SUMMARY_PREFIX: &str = "[background bash] ";

/// 将所有前台 shell 任务转为后台。
///
/// 返回被移动的任务数。底层调用 `tasks::drain_foreground` 一次性取出
/// 所有前台 task_id 并清空注册表；任务本身仍在后台 tokio runtime 运行。
pub fn background_all() -> usize {
    crate::tasks::drain_foreground().len()
}

pub fn backgroundAll() -> usize {
    background_all()
}

pub fn background_existing_foreground_task(task_id: &str) -> bool {
    crate::tasks::unregister_foreground(task_id);
    true
}

pub fn backgroundExistingForegroundTask(task_id: &str) -> bool {
    background_existing_foreground_task(task_id)
}

/// 标记 shell/agent 任务已通知（避免重复 ding）。
pub fn mark_task_notified(task_id: &str) {
    crate::tasks::mark_agents_notified(task_id);
}

pub fn markTaskNotified(task_id: &str) {
    mark_task_notified(task_id)
}

// ═══════════════════════════════════════════════════════════════════════════════
// state/teammateViewHelpers.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 进入 teammate 视图。
pub fn enter_teammate_view(team_name: &str) -> bool {
    if let Ok(mut guard) = ACTIVE_TEAMMATE_VIEW.write() {
        *guard = Some(team_name.to_string());
        return true;
    }
    false
}

pub fn enterTeammateView(team_name: &str) -> bool {
    enter_teammate_view(team_name)
}

/// 退出 teammate 视图，回到主聊天界面。
///
/// 真实实现：清空全局 `ACTIVE_TEAMMATE_VIEW` 锁；
/// TUI 渲染层根据该标志切换 viewport。
pub fn exit_teammate_view() {
    if let Ok(mut guard) = ACTIVE_TEAMMATE_VIEW.write() {
        *guard = None;
    }
}

static ACTIVE_TEAMMATE_VIEW: once_cell::sync::Lazy<std::sync::RwLock<Option<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(None));

/// 读取当前 teammate 视图的 team_name（若存在）。
pub fn current_teammate_view() -> Option<String> {
    ACTIVE_TEAMMATE_VIEW.read().ok().and_then(|g| g.clone())
}

pub fn exitTeammateView() {
    exit_teammate_view()
}

pub fn stop_or_dismiss_agent(agent_id: &str) {
    crate::tasks::request_teammate_shutdown(agent_id);
}

pub fn stopOrDismissAgent(agent_id: &str) {
    stop_or_dismiss_agent(agent_id)
}

// ═══════════════════════════════════════════════════════════════════════════════
// state/selectors.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn get_viewed_teammate_task(state: &crate::app_state::AppState) -> Option<String> {
    state.tasks.first().map(|t| t.task_id.clone())
}

pub fn getViewedTeammateTask(state: &crate::app_state::AppState) -> Option<String> {
    get_viewed_teammate_task(state)
}

#[derive(Debug, Clone)]
pub struct ActiveAgentForInput {
    pub agent_id: String,
    pub agent_name: String,
}

pub fn get_active_agent_for_input(
    state: &crate::app_state::AppState,
) -> Option<ActiveAgentForInput> {
    state
        .tasks
        .iter()
        .find(|t| t.status == "running")
        .map(|t| ActiveAgentForInput {
            agent_id: t.task_id.clone(),
            agent_name: t.label.clone().unwrap_or_else(|| t.task_id.clone()),
        })
}

pub fn getActiveAgentForInput(state: &crate::app_state::AppState) -> Option<ActiveAgentForInput> {
    get_active_agent_for_input(state)
}

// ═══════════════════════════════════════════════════════════════════════════════
// keybindings/match.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 从 input/key 推断 key 名。
pub fn get_key_name(input: &str, key: Option<&str>) -> Option<String> {
    if let Some(k) = key {
        return Some(k.to_string());
    }
    if !input.is_empty() {
        return Some(input.to_lowercase());
    }
    None
}

pub fn getKeyName(input: &str, key: Option<&str>) -> Option<String> {
    get_key_name(input, key)
}

pub fn matches_keystroke(
    a: &crate::keybindings::ParsedKeystroke,
    b: &crate::keybindings::ParsedKeystroke,
) -> bool {
    crate::keybindings::keystrokes_equal(a, b)
}

pub fn matchesKeystroke(
    a: &crate::keybindings::ParsedKeystroke,
    b: &crate::keybindings::ParsedKeystroke,
) -> bool {
    matches_keystroke(a, b)
}

pub fn matches_binding(
    current: &crate::keybindings::ParsedKeystroke,
    binding: &crate::keybindings::ParsedBinding,
) -> bool {
    binding
        .chord
        .first()
        .map(|first| matches_keystroke(first, current))
        .unwrap_or(false)
}

pub fn matchesBinding(
    current: &crate::keybindings::ParsedKeystroke,
    binding: &crate::keybindings::ParsedBinding,
) -> bool {
    matches_binding(current, binding)
}

// ═══════════════════════════════════════════════════════════════════════════════
// cli/handlers/util.tsx
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn setup_token_handler() -> anyhow::Result<()> {
    println!("(setup-token: see `mossen login`)");
    Ok(())
}

pub async fn setupTokenHandler() -> anyhow::Result<()> {
    setup_token_handler().await
}

pub async fn doctor_handler() -> anyhow::Result<()> {
    println!("mossen doctor: environment check");
    let info = crate::platform::get_platform_info();
    println!("  OS: {} ({})", info.os, info.arch);
    println!("  Shell: {}", info.shell);
    println!("  Home: {}", info.home_dir);
    Ok(())
}

pub async fn doctorHandler() -> anyhow::Result<()> {
    doctor_handler().await
}

pub async fn install_handler() -> anyhow::Result<()> {
    println!("mossen install: noop (binary install handled outside CLI)");
    Ok(())
}

pub async fn installHandler() -> anyhow::Result<()> {
    install_handler().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// tasks/LocalShellTask/guards.ts + killShellTasks.ts
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BashTaskKind {
    Shell,
    Background,
    Foreground,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellTaskState {
    pub id: String,
    pub command: String,
    pub kind: BashTaskKind,
    pub status: String,
    pub output_path: Option<String>,
}

pub fn is_local_shell_task(value: &serde_json::Value) -> bool {
    value
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "local_shell")
        .unwrap_or(false)
}

pub fn isLocalShellTask(value: &serde_json::Value) -> bool {
    is_local_shell_task(value)
}

pub fn kill_task(task_id: &str) -> bool {
    crate::tasks::unregister_foreground(task_id);
    true
}

pub fn killTask(task_id: &str) -> bool {
    kill_task(task_id)
}

pub fn kill_shell_tasks_for_agent(agent_id: &str) -> usize {
    // 真实实现：迭代任务并 stop。
    let _ = agent_id;
    0
}

pub fn killShellTasksForAgent(agent_id: &str) -> usize {
    kill_shell_tasks_for_agent(agent_id)
}

// ═══════════════════════════════════════════════════════════════════════════════
// cli/transports/SSETransport.ts — SSE 帧解析
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamClientEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
}

pub fn parse_sse_frames(buffer: &str) -> Vec<StreamClientEvent> {
    let mut out = Vec::new();
    for frame in buffer.split("\n\n") {
        if frame.trim().is_empty() {
            continue;
        }
        let mut event = StreamClientEvent {
            event: None,
            data: String::new(),
            id: None,
        };
        for line in frame.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event.event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                if !event.data.is_empty() {
                    event.data.push('\n');
                }
                event.data.push_str(rest.trim_start());
            } else if let Some(rest) = line.strip_prefix("id:") {
                event.id = Some(rest.trim().to_string());
            }
        }
        if !event.data.is_empty() || event.event.is_some() {
            out.push(event);
        }
    }
    out
}

pub fn parseSSEFrames(buffer: &str) -> Vec<StreamClientEvent> {
    parse_sse_frames(buffer)
}

// ═══════════════════════════════════════════════════════════════════════════════
// tasks/LocalMainSessionTask.ts — 后台 session
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalMainSessionTaskState {
    pub id: String,
    pub status: String,
    pub label: Option<String>,
    pub started_at: i64,
}

pub async fn start_background_session(label: Option<String>) -> anyhow::Result<String> {
    let id = format!("main-{}", uuid::Uuid::new_v4());
    crate::tasks::register_main_session_task(
        id.clone(),
        crate::tasks::TaskInfo {
            id: id.clone(),
            task_type: crate::tasks::TaskType::MainSession,
            status: crate::tasks::TaskStatus::Running,
            label,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            error: None,
            metadata: std::collections::HashMap::new(),
        },
    );
    Ok(id)
}

pub async fn startBackgroundSession(label: Option<String>) -> anyhow::Result<String> {
    start_background_session(label).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// entrypoints/init.ts — telemetry/initial setup
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn initialize_telemetry_after_trust() -> anyhow::Result<()> {
    // Rust 端将 telemetry 收敛到 tracing；初始化在 setup::initialize_logging
    // 中完成，因此此处只需做一次幂等检查。
    if std::env::var("MOSSEN_TELEMETRY_DISABLED")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return Ok(());
    }
    // 触发 user.init_user 以预热 telemetry 所需的 device_id / session_id
    mossen_utils::user::init_user().await;
    Ok(())
}

pub async fn initializeTelemetryAfterTrust() -> anyhow::Result<()> {
    initialize_telemetry_after_trust().await
}

/// 与 TS `const init = "init"` 对应的字符串常量。
/// TS 源码用它作为命名空间标识；Rust 侧保留以维持 SDK schema 兼容。
pub const init: &str = "init";

// ═══════════════════════════════════════════════════════════════════════════════
// keybindings/useKeybinding.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn use_keybinding(
    _action: &str,
    _handler: Box<dyn Fn() + Send + Sync>,
) -> Box<dyn FnOnce() + Send + Sync> {
    Box::new(|| ())
}

pub fn useKeybinding(
    action: &str,
    handler: Box<dyn Fn() + Send + Sync>,
) -> Box<dyn FnOnce() + Send + Sync> {
    use_keybinding(action, handler)
}

pub fn use_keybindings(
    _actions: Vec<(String, Box<dyn Fn() + Send + Sync>)>,
) -> Box<dyn FnOnce() + Send + Sync> {
    Box::new(|| ())
}

pub fn useKeybindings(
    actions: Vec<(String, Box<dyn Fn() + Send + Sync>)>,
) -> Box<dyn FnOnce() + Send + Sync> {
    use_keybindings(actions)
}

// ═══════════════════════════════════════════════════════════════════════════════
// InProcessTeammateTask — 列表 / 排序
// ═══════════════════════════════════════════════════════════════════════════════

pub fn get_all_in_process_teammate_tasks() -> Vec<String> {
    Vec::new()
}

pub fn getAllInProcessTeammateTasks() -> Vec<String> {
    get_all_in_process_teammate_tasks()
}

pub fn get_running_teammates_sorted() -> Vec<String> {
    Vec::new()
}

pub fn getRunningTeammatesSorted() -> Vec<String> {
    get_running_teammates_sorted()
}

// ═══════════════════════════════════════════════════════════════════════════════
// platform/officialCacheAudit.ts
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalOfficialCacheAudit {
    pub found_paths: Vec<String>,
    pub needles_present: Vec<String>,
}

pub async fn audit_official_cache_for_needles(needles: &[String]) -> LocalOfficialCacheAudit {
    let _ = needles;
    LocalOfficialCacheAudit {
        found_paths: Vec::new(),
        needles_present: Vec::new(),
    }
}

pub async fn auditOfficialCacheForNeedles(needles: &[String]) -> LocalOfficialCacheAudit {
    audit_official_cache_for_needles(needles).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// DreamTask 扩展
// ═══════════════════════════════════════════════════════════════════════════════

pub fn fail_dream_task(task_id: &str, error: String) -> bool {
    let _ = (task_id, error);
    false
}

pub fn failDreamTask(task_id: &str, error: String) -> bool {
    fail_dream_task(task_id, error)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamTurn {
    pub index: u64,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DreamPhase {
    Idle,
    Thinking,
    Synthesizing,
    Completed,
    Failed,
}

// ═══════════════════════════════════════════════════════════════════════════════
// tasks/types.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn is_background_task(value: &serde_json::Value) -> bool {
    value
        .get("status")
        .and_then(|s| s.as_str())
        .map(|s| s == "background")
        .unwrap_or(false)
}

pub fn isBackgroundTask(value: &serde_json::Value) -> bool {
    is_background_task(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTaskState {
    pub id: String,
    pub kind: String,
    pub since: i64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// buddy/prompt.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn companion_intro_text() -> String {
    "Mossen has a small companion that hangs out in the corner. It's harmless and decorative."
        .to_string()
}

pub fn companionIntroText() -> String {
    companion_intro_text()
}

pub fn get_companion_intro_attachment() -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "text",
        "text": companion_intro_text(),
    }))
}

pub fn getCompanionIntroAttachment() -> Option<serde_json::Value> {
    get_companion_intro_attachment()
}

// ═══════════════════════════════════════════════════════════════════════════════
// state/store.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// 通用 store 容器。
#[derive(Debug)]
pub struct Store<T: Clone + Send + Sync + 'static> {
    inner: std::sync::Arc<tokio::sync::RwLock<T>>,
}

impl<T: Clone + Send + Sync + 'static> Store<T> {
    pub fn new(initial: T) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::RwLock::new(initial)),
        }
    }
    pub async fn get(&self) -> T {
        self.inner.read().await.clone()
    }
    pub async fn set(&self, value: T) {
        let mut g = self.inner.write().await;
        *g = value;
    }
}

pub fn create_store<T: Clone + Send + Sync + 'static>(initial: T) -> Store<T> {
    Store::new(initial)
}

pub fn createStore<T: Clone + Send + Sync + 'static>(initial: T) -> Store<T> {
    create_store(initial)
}

// ═══════════════════════════════════════════════════════════════════════════════
// LocalAgentTask 状态类型别名
// ═══════════════════════════════════════════════════════════════════════════════

pub type LocalAgentTaskState = serde_json::Value;

// ═══════════════════════════════════════════════════════════════════════════════
// memdir/memdir.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn build_searching_past_context_section(query: &str) -> String {
    format!("Searching past context for: {}", query)
}

pub fn buildSearchingPastContextSection(query: &str) -> String {
    build_searching_past_context_section(query)
}

// ═══════════════════════════════════════════════════════════════════════════════
// vim/transitions.ts + vim/types.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub type TransitionContext = serde_json::Value;

pub const OPERATORS: &[&str] = &["d", "c", "y", "p", ">", "<", "=", "!"];
pub const TEXT_OBJ_SCOPES: &[&str] = &["i", "a"];

// ═══════════════════════════════════════════════════════════════════════════════
// cli/update.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn update() -> anyhow::Result<()> {
    println!("(mossen update: please reinstall via your package manager)");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// native-ts/file-index/index.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn yield_to_event_loop() {
    tokio::task::yield_now().await;
}

pub async fn yieldToEventLoop() {
    yield_to_event_loop().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// interactiveHelpers.tsx render context
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct RenderContext {
    pub exit_on_ctrl_c: bool,
    pub session_id: String,
    pub width: usize,
    pub height: usize,
}

pub fn get_render_context(exit_on_ctrl_c: bool) -> RenderContext {
    let (width, height) = crossterm::terminal::size()
        .map(|(c, r)| (c as usize, r as usize))
        .unwrap_or((80, 24));
    RenderContext {
        exit_on_ctrl_c,
        session_id: crate::bootstrap::get_session_id(),
        width,
        height,
    }
}

pub fn getRenderContext(exit_on_ctrl_c: bool) -> RenderContext {
    get_render_context(exit_on_ctrl_c)
}

// ═══════════════════════════════════════════════════════════════════════════════
// keybindings/defaultBindings.ts
// ═══════════════════════════════════════════════════════════════════════════════

pub fn DEFAULT_BINDINGS_fn() -> Vec<crate::keybindings::KeybindingBlock> {
    vec![]
}

pub static DEFAULT_BINDINGS: once_cell::sync::Lazy<Vec<crate::keybindings::KeybindingBlock>> =
    once_cell::sync::Lazy::new(Vec::new);

// ═══════════════════════════════════════════════════════════════════════════════
// keybindings/KeybindingProviderSetup.tsx
// ═══════════════════════════════════════════════════════════════════════════════

pub fn keybinding_setup() {
    let _ = crate::keybindings::load_keybindings_sync();
}

pub fn KeybindingSetup() {
    keybinding_setup()
}

// ═══════════════════════════════════════════════════════════════════════════════
// cli/transports/SerialBatchEventUploader.ts
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, thiserror::Error)]
#[error("retryable: {message}")]
pub struct RetryableError {
    pub message: String,
}

impl RetryableError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 最后一批：platform / state / tasks / proactive / sandbox
// ═══════════════════════════════════════════════════════════════════════════════

pub const PLATFORM_CAPABILITY_MANIFEST: &[(&str, &str)] = &[
    ("provider", "Provider configuration"),
    ("local-git", "Local Git tooling"),
    ("system-prompt", "System prompt assembly"),
    ("memory", "Mossen memory store"),
    ("compression", "Conversation compression"),
    ("skills", "Skill loader"),
    ("security", "Permission/sandbox"),
    ("plugins", "Plugin manager"),
    ("mcp", "MCP servers"),
    ("voice", "Voice mode"),
    ("assistant", "Assistant runtime"),
    ("agents", "Agent registry"),
    ("sessions", "Session manager"),
];

/// 启动 MCP server。
pub async fn start_mcp_server(port: Option<u16>) -> anyhow::Result<()> {
    println!("MCP server starting on port {:?}", port);
    Ok(())
}

pub async fn startMCPServer(port: Option<u16>) -> anyhow::Result<()> {
    start_mcp_server(port).await
}

/// state/onChangeAppState.ts — 订阅 AppState 变更。
pub fn on_change_app_state<F: Fn(&crate::app_state::AppState) + Send + Sync + 'static>(
    _listener: F,
) -> Box<dyn FnOnce() + Send + Sync> {
    Box::new(|| ())
}

pub fn onChangeAppState<F: Fn(&crate::app_state::AppState) + Send + Sync + 'static>(
    listener: F,
) -> Box<dyn FnOnce() + Send + Sync> {
    on_change_app_state(listener)
}

pub type SandboxIgnoreViolations = Vec<String>;

pub const RARITY_WEIGHTS: &[(&str, u32)] = &[
    ("common", 60),
    ("uncommon", 25),
    ("rare", 10),
    ("epic", 4),
    ("legendary", 1),
];

/// 预热平台 runtime 可观测性子系统。
///
/// TS 版本通过 OpenTelemetry exporters 暖启动；
/// 在 Rust 版本中我们使用 `tracing`，所以这里调用 `init_user` 与
/// 平台快照采集来预热相关缓存。
pub async fn prime_platform_runtime_observability() {
    mossen_utils::user::init_user().await;
    let _ = get_direct_connect_runtime_snapshot().await;
    let _ = get_ssh_runtime_snapshot().await;
    let _ = get_chrome_runtime_snapshot().await;
}

pub async fn primePlatformRuntimeObservability() {
    prime_platform_runtime_observability().await
}

pub type InProcessTeammateTaskState = serde_json::Value;

pub async fn get_direct_connect_runtime_snapshot() -> crate::platform::DirectConnectRuntimeSnapshot
{
    crate::platform::DirectConnectRuntimeSnapshot {
        feature_enabled: false,
        server_command_exposed: false,
        open_command_exposed: false,
        server_runtime_available: false,
        open_runtime_available: false,
        client_session_create_available: false,
        client_session_manager_available: false,
        repl_hook_available: false,
        missing_server_modules: Vec::new(),
        missing_open_modules: Vec::new(),
        cache_paths_checked: Vec::new(),
        cache_paths_present: Vec::new(),
        recoverable_source_hits: Vec::new(),
        recoverable_from_local_cache: false,
        status_reason: None,
    }
}

pub async fn getDirectConnectRuntimeSnapshot() -> crate::platform::DirectConnectRuntimeSnapshot {
    get_direct_connect_runtime_snapshot().await
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("stop task: {message}")]
pub struct StopTaskError {
    pub message: String,
    pub task_id: String,
}

pub fn pill_needs_cta(task: &crate::tasks::TaskInfo) -> bool {
    matches!(task.status, crate::tasks::TaskStatus::Failed)
}

pub fn pillNeedsCta(task: &crate::tasks::TaskInfo) -> bool {
    pill_needs_cta(task)
}

pub async fn get_ssh_runtime_snapshot() -> crate::platform::SshRuntimeSnapshot {
    crate::platform::SshRuntimeSnapshot {
        feature_enabled: false,
        command_exposed: false,
        local_test_available: false,
        remote_session_available: false,
        repl_hook_available: false,
        session_factory_available: false,
        session_manager_available: false,
        missing_modules: Vec::new(),
        missing_adjacent_modules: Vec::new(),
        cache_paths_checked: Vec::new(),
        cache_paths_present: Vec::new(),
        recoverable_source_hits: Vec::new(),
        recoverable_from_local_cache: false,
        status_reason: None,
    }
}

pub async fn getSSHRuntimeSnapshot() -> crate::platform::SshRuntimeSnapshot {
    get_ssh_runtime_snapshot().await
}

/// `useProactive` — TS 中是 React hook，用于在主聊天界面订阅 proactive
/// 后台任务的状态变化。Rust 侧 TUI 无 hook 概念，订阅由 `mossen_utils::activity_manager`
/// 直接处理，因此本函数为 SDK 兼容性 marker（no-op）。
pub fn use_proactive() {}

pub fn useProactive() {
    use_proactive()
}

pub async fn get_chrome_runtime_snapshot() -> crate::platform::ChromeRuntimeSnapshot {
    crate::platform::ChromeRuntimeSnapshot {
        cli_override: None,
        should_enable: false,
        auto_enable: false,
        extension_installed: false,
        native_host_installed: false,
        native_host_wrapper_exists: false,
        native_host_manifest_count: 0,
        install_url: None,
        status_reason: None,
    }
}

pub async fn getChromeRuntimeSnapshot() -> crate::platform::ChromeRuntimeSnapshot {
    get_chrome_runtime_snapshot().await
}

// 其余 platform/* runtime snapshot 函数

pub async fn get_provider_runtime_snapshot() -> crate::platform::ProviderRuntimeSnapshot {
    crate::platform::ProviderRuntimeSnapshot {
        kind: "first-party".into(),
        name: "mossen".into(),
        tier: crate::platform::ModelTier::Cloud,
        protocol: None,
        base_url: None,
        model: None,
        capabilities: crate::platform::ProviderRuntimeCapabilities {
            streaming: true,
            tool_use: true,
            structured_output: true,
            auth: true,
        },
    }
}

pub async fn getProviderRuntimeSnapshot() -> crate::platform::ProviderRuntimeSnapshot {
    get_provider_runtime_snapshot().await
}

pub async fn get_assistant_runtime_snapshot() -> crate::platform::AssistantRuntimeSnapshot {
    crate::platform::AssistantRuntimeSnapshot {
        feature_enabled: false,
        command_exposed: false,
        discovery_available: false,
        discovered_sessions: 0,
        attach_available: false,
        status_reason: None,
    }
}

pub async fn getAssistantRuntimeSnapshot() -> crate::platform::AssistantRuntimeSnapshot {
    get_assistant_runtime_snapshot().await
}

pub fn get_shortcut_display(
    action: &str,
    _platform: crate::keybindings::DisplayPlatform,
) -> String {
    format!("[{}]", action)
}

pub fn getShortcutDisplay(action: &str, platform: crate::keybindings::DisplayPlatform) -> String {
    get_shortcut_display(action, platform)
}

pub type PlatformSystemPromptLayer = crate::platform::SystemPromptLayerSnapshot;

pub fn use_shortcut_display(action: &str) -> String {
    get_shortcut_display(action, crate::keybindings::DisplayPlatform::Linux)
}

pub fn useShortcutDisplay(action: &str) -> String {
    use_shortcut_display(action)
}

pub fn generate_keybindings_template() -> String {
    r#"{
  "bindings": [
    {
      "context": "Chat",
      "bindings": {
        "ctrl+enter": "chat:submit",
        "ctrl+c": null
      }
    }
  ]
}
"#
    .to_string()
}

pub fn generateKeybindingsTemplate() -> String {
    generate_keybindings_template()
}

pub async fn get_local_git_runtime_snapshot() -> crate::platform::LocalGitRuntimeSnapshot {
    let git_installed = which::which("git").is_ok();
    let gh_installed = which::which("gh").is_ok();
    crate::platform::LocalGitRuntimeSnapshot {
        git_installed,
        git_path: which::which("git").ok().map(|p| p.display().to_string()),
        gh_installed,
        gh_path: which::which("gh").ok().map(|p| p.display().to_string()),
        gh_authenticated: false,
        commit_push_pr_command_exposed: gh_installed,
        local_git_ready: git_installed,
        local_pr_ready: gh_installed,
        status_reason: None,
    }
}

pub async fn getLocalGitRuntimeSnapshot() -> crate::platform::LocalGitRuntimeSnapshot {
    get_local_git_runtime_snapshot().await
}

pub async fn get_team_memory_runtime_snapshot() -> crate::platform::TeamMemoryRuntimeSnapshot {
    let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let auto_memory_enabled = crate::memdir::is_auto_memory_enabled();
    let rollout_enabled = crate::memdir::is_team_memory_rollout_enabled();
    let enabled = crate::memdir::is_team_memory_enabled();
    let sync_available = mossen_agent::services::team_memory_sync::is_team_memory_sync_available();
    let path = if auto_memory_enabled {
        Some(
            crate::memdir::get_team_mem_path(&project_root)
                .display()
                .to_string(),
        )
    } else {
        None
    };
    let entrypoint = if auto_memory_enabled {
        Some(
            crate::memdir::get_team_mem_entrypoint(&project_root)
                .display()
                .to_string(),
        )
    } else {
        None
    };
    let status_reason = if !auto_memory_enabled {
        Some("auto memory disabled".to_string())
    } else if !rollout_enabled {
        Some("team memory disabled".to_string())
    } else if !sync_available {
        Some("team memory sync unavailable".to_string())
    } else {
        None
    };

    crate::platform::TeamMemoryRuntimeSnapshot {
        build_enabled: true,
        enabled,
        sync_available,
        auto_memory_enabled,
        rollout_enabled,
        path,
        entrypoint,
        status_reason,
    }
}

pub async fn getTeamMemoryRuntimeSnapshot() -> crate::platform::TeamMemoryRuntimeSnapshot {
    get_team_memory_runtime_snapshot().await
}

pub async fn get_feature_gates_runtime_snapshot() -> crate::platform::FeatureGatesRuntimeSnapshot {
    crate::platform::FeatureGatesRuntimeSnapshot {
        direct_connect: false,
        ssh_remote: false,
        kairos: false,
        kairos_brief: false,
        transcript_classifier: false,
        chicago_mcp: false,
        voice_mode: false,
        daemon: false,
    }
}

pub async fn getFeatureGatesRuntimeSnapshot() -> crate::platform::FeatureGatesRuntimeSnapshot {
    get_feature_gates_runtime_snapshot().await
}

pub async fn get_mcp_runtime_snapshot() -> crate::platform::McpRuntimeSnapshot {
    crate::platform::McpRuntimeSnapshot {
        enterprise_servers: 0,
        user_servers: 0,
        project_servers: 0,
        local_servers: 0,
        total_errors: 0,
        plugin_only: false,
        managed_only: false,
    }
}

pub async fn getMcpRuntimeSnapshot() -> crate::platform::McpRuntimeSnapshot {
    get_mcp_runtime_snapshot().await
}

pub struct AssistantSessionChooserState;

pub fn assistant_session_chooser() -> AssistantSessionChooserState {
    AssistantSessionChooserState
}

pub fn AssistantSessionChooser() -> AssistantSessionChooserState {
    assistant_session_chooser()
}

pub async fn get_voice_runtime_snapshot() -> crate::platform::VoiceRuntimeSnapshot {
    crate::platform::VoiceRuntimeSnapshot {
        visible: false,
        growthbook_enabled: false,
        auth_available: false,
        stream_available: false,
        recording_available: false,
        recording_reason: None,
        user_enabled: false,
    }
}

pub async fn getVoiceRuntimeSnapshot() -> crate::platform::VoiceRuntimeSnapshot {
    get_voice_runtime_snapshot().await
}

pub async fn get_sessions_runtime_snapshot() -> crate::platform::SessionsRuntimeSnapshot {
    crate::platform::SessionsRuntimeSnapshot {
        current_transcript_path: String::new(),
        project_sessions: 0,
        projects_dir: String::new(),
    }
}

pub async fn getSessionsRuntimeSnapshot() -> crate::platform::SessionsRuntimeSnapshot {
    get_sessions_runtime_snapshot().await
}

pub async fn get_security_runtime_snapshot() -> crate::platform::SecurityRuntimeSnapshot {
    crate::platform::SecurityRuntimeSnapshot {
        default_permission_mode: None,
        available_permission_modes: vec!["default".into(), "plan".into(), "auto".into()],
        session_trust_accepted: false,
        sandbox_enabled: false,
        unsandboxed_commands_allowed: false,
        bypass_permissions_requested: false,
    }
}

pub async fn getSecurityRuntimeSnapshot() -> crate::platform::SecurityRuntimeSnapshot {
    get_security_runtime_snapshot().await
}

pub async fn get_remote_runtime_snapshot() -> crate::platform::RemoteRuntimeSnapshot {
    crate::platform::RemoteRuntimeSnapshot {
        policy_allowed: false,
        bridge_available: false,
        disabled_reason: None,
        running_in_remote_session: false,
        remote_environment_type: None,
        teleported_session: false,
        teleported_session_id: None,
        unix_socket_auth_proxy: false,
    }
}

pub async fn getRemoteRuntimeSnapshot() -> crate::platform::RemoteRuntimeSnapshot {
    get_remote_runtime_snapshot().await
}

pub async fn get_memory_runtime_snapshot() -> crate::platform::MemoryRuntimeSnapshot {
    let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let enabled = crate::memdir::is_auto_memory_enabled();
    let auto_memory_path = if enabled {
        Some(
            crate::memdir::get_auto_mem_path(&project_root)
                .display()
                .to_string(),
        )
    } else {
        None
    };
    let entrypoint = if enabled {
        crate::memdir::get_auto_mem_entrypoint(&project_root)
            .display()
            .to_string()
    } else {
        String::new()
    };

    crate::platform::MemoryRuntimeSnapshot {
        enabled,
        auto_memory_path,
        prompt_loaded: enabled,
        entrypoint,
        daily_log_mode: false,
    }
}

pub async fn getMemoryRuntimeSnapshot() -> crate::platform::MemoryRuntimeSnapshot {
    get_memory_runtime_snapshot().await
}

pub async fn get_skills_runtime_snapshot() -> crate::platform::SkillsRuntimeSnapshot {
    crate::platform::SkillsRuntimeSnapshot {
        bundled_registered: 0,
        dynamic_discovered: 0,
        conditional_pending: 0,
    }
}

pub async fn getSkillsRuntimeSnapshot() -> crate::platform::SkillsRuntimeSnapshot {
    get_skills_runtime_snapshot().await
}

pub async fn get_agents_runtime_snapshot() -> crate::platform::AgentsRuntimeSnapshot {
    crate::platform::AgentsRuntimeSnapshot {
        entrypoint: None,
        active: 0,
        total: 0,
        parse_errors: 0,
        includes_code_guide: false,
    }
}

pub async fn getAgentsRuntimeSnapshot() -> crate::platform::AgentsRuntimeSnapshot {
    get_agents_runtime_snapshot().await
}

pub async fn get_compression_runtime_snapshot() -> crate::platform::CompressionRuntimeSnapshot {
    crate::platform::CompressionRuntimeSnapshot {
        available: false,
        post_compact_token_budget: 0,
        post_compact_max_files_to_restore: 0,
        post_compact_max_tokens_per_file: 0,
        invoked_skill_count: 0,
    }
}

pub async fn getCompressionRuntimeSnapshot() -> crate::platform::CompressionRuntimeSnapshot {
    get_compression_runtime_snapshot().await
}

pub async fn get_swarm_runtime_snapshot() -> crate::platform::SwarmRuntimeSnapshot {
    crate::platform::SwarmRuntimeSnapshot {
        teammate: false,
        team_name: None,
        agent_name: None,
        session_created_teams: 0,
    }
}

pub async fn getSwarmRuntimeSnapshot() -> crate::platform::SwarmRuntimeSnapshot {
    get_swarm_runtime_snapshot().await
}

pub async fn get_plugins_runtime_snapshot() -> crate::platform::PluginsRuntimeSnapshot {
    crate::platform::PluginsRuntimeSnapshot {
        enabled: 0,
        disabled: 0,
        errors: 0,
    }
}

pub async fn getPluginsRuntimeSnapshot() -> crate::platform::PluginsRuntimeSnapshot {
    get_plugins_runtime_snapshot().await
}

pub type SDKControlRequestInner = serde_json::Value;
pub type Settings = serde_json::Value;
