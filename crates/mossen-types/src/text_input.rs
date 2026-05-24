//! # text_input — 文本输入 UI 类型
//!
//! 对应 TypeScript `types/textInputTypes.ts`。
//! 定义文本输入组件的状态和配置类型。
//! 注意：React/JSX 相关类型不翻译，仅翻译纯数据类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::PastedContent;
use crate::ids::AgentId;
use crate::message::{AssistantMessage, ContentBlock, MessageOrigin};

/// 内联幽灵文本（命令自动补全提示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineGhostText {
    /// 幽灵文本。
    pub text: String,
    /// 完整命令名称。
    pub full_command: String,
    /// 插入位置。
    pub insert_position: usize,
}

/// Vim 编辑模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VimMode {
    INSERT,
    NORMAL,
}

/// 提示输入模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptInputMode {
    /// Bash 模式。
    Bash,
    /// 提示模式。
    Prompt,
    /// 孤立权限模式。
    OrphanedPermission,
    /// 任务通知模式。
    TaskNotification,
}

/// 可编辑的提示输入模式（排除通知模式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditablePromptInputMode {
    Bash,
    Prompt,
    OrphanedPermission,
}

/// 队列优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuePriority {
    /// 立即中断并发送。
    Now,
    /// 当前工具调用完成后发送。
    Next,
    /// 当前回合完成后发送。
    Later,
}

/// 排队的命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedCommand {
    /// 值（字符串或内容块列表）。
    pub value: QueuedCommandValue,
    /// 模式。
    pub mode: PromptInputMode,
    /// 优先级。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<QueuePriority>,
    /// UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// 孤立权限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orphaned_permission: Option<OrphanedPermission>,
    /// 粘贴内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pasted_contents: Option<HashMap<String, serde_json::Value>>,
    /// 展开前的值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_expansion_value: Option<String>,
    /// 跳过斜杠命令。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_slash_commands: Option<bool>,
    /// 桥接来源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_origin: Option<bool>,
    /// 是否为元消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_meta: Option<bool>,
    /// 消息来源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<MessageOrigin>,
    /// 工作负载标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workload: Option<String>,
    /// 目标 Agent ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

/// 排队命令的值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueuedCommandValue {
    /// 字符串。
    Text(String),
    /// 内容块列表。
    Blocks(Vec<ContentBlock>),
}

/// 孤立权限。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanedPermission {
    /// 权限结果。
    pub permission_result: crate::permissions::PermissionResult,
    /// 助手消息。
    pub assistant_message: AssistantMessage,
}

/// 检查粘贴内容是否为有效的图片（非空内容）。
/// 对应 TS `isValidImagePaste()`。
pub fn is_valid_image_paste(c: &PastedContent) -> bool {
    match c {
        PastedContent::Image { content, .. } => !content.is_empty(),
        _ => false,
    }
}

/// 从 QueuedCommand 的 pastedContents 中提取有效图片粘贴 ID。
/// 对应 TS `getImagePasteIds()`。
pub fn get_image_paste_ids(
    pasted_contents: Option<&HashMap<String, PastedContent>>,
) -> Option<Vec<usize>> {
    let map = pasted_contents?;
    let ids: Vec<usize> = map
        .values()
        .filter(|c| is_valid_image_paste(c))
        .filter_map(|c| match c {
            PastedContent::Image { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// 粘贴累积状态。
/// 对应 TS `BaseInputState.pasteState`。
/// `timeoutId` 在 Rust 端不持久化（属于运行时 handle），不参与 serde。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteState {
    /// 粘贴分片累积缓冲。
    pub chunks: Vec<String>,
}

/// 基础输入状态。
/// 对应 TS `BaseInputState`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseInputState {
    /// 渲染值。
    pub rendered_value: String,
    /// 光标偏移。
    pub offset: usize,
    /// 光标行（0-indexed）。
    pub cursor_line: usize,
    /// 光标列（显示宽度）。
    pub cursor_column: usize,
    /// 视口开始字符偏移。
    pub viewport_char_offset: usize,
    /// 视口结束字符偏移。
    pub viewport_char_end: usize,
    /// 是否正在粘贴。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_pasting: Option<bool>,
    /// 粘贴累积状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paste_state: Option<PasteState>,
}

/// 文本输入状态（等同于 BaseInputState）。
pub type TextInputState = BaseInputState;

/// Vim 输入状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VimInputState {
    /// 基础状态。
    #[serde(flatten)]
    pub base: BaseInputState,
    /// Vim 模式。
    pub mode: VimMode,
}
