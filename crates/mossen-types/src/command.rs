//! # command — 命令类型
//!
//! 对应 TypeScript `types/command.ts`。
//! 定义 `Command`、`PromptCommand`、`CommandBase` 等命令系统类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 本地命令结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalCommandResult {
    /// 文本结果。
    Text { value: String },
    /// 压缩结果。
    Compact {
        compaction_result: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_text: Option<String>,
    },
    /// 跳过。
    Skip,
}

/// 恢复入口点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeEntrypoint {
    /// CLI 标志。
    CliFlag,
    /// 斜杠命令选择器。
    SlashCommandPicker,
    /// 斜杠命令指定会话 ID。
    SlashCommandSessionId,
    /// 斜杠命令指定标题。
    SlashCommandTitle,
    /// Fork。
    Fork,
}

/// 命令结果显示方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandResultDisplay {
    /// 跳过显示。
    Skip,
    /// 系统消息。
    System,
    /// 用户消息。
    User,
}

/// 命令可用性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAvailability {
    /// 托管适配器订阅用户。
    Hosted,
    /// 直接使用提供商 API 密钥的用户。
    Console,
}

/// 设置来源。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingSource {
    /// 用户设置。
    UserSettings,
    /// 项目设置。
    ProjectSettings,
    /// 本地设置。
    LocalSettings,
    /// 策略设置。
    PolicySettings,
    /// 功能标志设置。
    FlagSettings,
}

/// Prompt 命令来源。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptCommandSource {
    /// 用户设置。
    UserSettings,
    /// 项目设置。
    ProjectSettings,
    /// 本地设置。
    LocalSettings,
    /// 策略设置。
    PolicySettings,
    /// 功能标志设置。
    FlagSettings,
    /// 内置。
    Builtin,
    /// MCP。
    Mcp,
    /// 插件。
    Plugin,
    /// 捆绑。
    Bundled,
}

/// 执行上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContext {
    /// 内联执行。
    Inline,
    /// 分叉子代理执行。
    Fork,
}

/// Effort 值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortValue {
    /// 低。
    Low,
    /// 中。
    Medium,
    /// 高。
    High,
    /// 最高。
    Max,
}

/// 插件信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// 插件清单。
    pub plugin_manifest: serde_json::Value,
    /// 仓库。
    pub repository: String,
}

/// Prompt 命令（数据部分）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCommandData {
    /// 进度消息。
    pub progress_message: String,
    /// 命令内容长度（字符数）。
    pub content_length: usize,
    /// 参数名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_names: Option<Vec<String>>,
    /// 允许的工具。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// 模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 来源。
    pub source: PromptCommandSource,
    /// 插件信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_info: Option<PluginInfo>,
    /// 是否禁用非交互模式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_non_interactive: Option<bool>,
    /// Hooks 设置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
    /// 技能根目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_root: Option<String>,
    /// 执行上下文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ExecutionContext>,
    /// Agent 类型（仅 fork 模式）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Effort 值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortValue>,
    /// Glob 路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
}

/// 命令加载来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandLoadedFrom {
    /// 已废弃的命令格式。
    #[serde(rename = "commands_DEPRECATED")]
    CommandsDeprecated,
    /// 技能。
    Skills,
    /// 插件。
    Plugin,
    /// 托管。
    Managed,
    /// 捆绑。
    Bundled,
    /// MCP。
    Mcp,
}

/// 命令种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    /// 工作流。
    Workflow,
}

/// 命令基础属性。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandBase {
    /// 可用性。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<Vec<CommandAvailability>>,
    /// 描述。
    pub description: String,
    /// 是否有用户指定的描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_user_specified_description: Option<bool>,
    /// 是否隐藏。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_hidden: Option<bool>,
    /// 命令名称。
    pub name: String,
    /// 别名列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// 是否为 MCP 命令。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mcp: Option<bool>,
    /// 参数提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    /// 使用场景。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    /// 版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 是否禁用模型调用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_model_invocation: Option<bool>,
    /// 用户是否可调用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_invocable: Option<bool>,
    /// 加载来源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded_from: Option<CommandLoadedFrom>,
    /// 命令种类。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<CommandKind>,
    /// 是否立即执行。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub immediate: Option<bool>,
    /// 是否敏感（参数会被脱敏）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sensitive: Option<bool>,
    /// 额外属性。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 命令完成选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDoneOptions {
    /// 显示方式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<CommandResultDisplay>,
    /// 是否发送给模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_query: Option<bool>,
    /// 附加元消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_messages: Option<Vec<String>>,
    /// 下一个输入。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_input: Option<String>,
    /// 是否提交下一个输入。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submit_next_input: Option<bool>,
}

/// 命令类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    /// Prompt 命令。
    Prompt,
    /// 本地命令。
    Local,
    /// 本地 JSX 命令。
    #[serde(rename = "local-jsx")]
    LocalJsx,
}

/// 获取命令名称。
/// 对应 TS `getCommandName()`: 返回 `userFacingName` 或回退到 `name`。
pub fn get_command_name(cmd: &CommandBase) -> &str {
    &cmd.name
}

/// 检查命令是否启用（默认为 true）。
/// 对应 TS `isCommandEnabled()`。
pub fn is_command_enabled(_cmd: &CommandBase) -> bool {
    // In TS this calls cmd.isEnabled?.() ?? true
    // isEnabled is a runtime function; in Rust data-only context, default to true.
    // When isEnabled logic is ported, callers can override.
    true
}
