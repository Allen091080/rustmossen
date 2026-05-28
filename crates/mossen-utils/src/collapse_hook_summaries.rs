//! # collapse_hook_summaries — Hook 摘要折叠工具
//!
//! 对应 TypeScript `utils/collapseHookSummaries.ts`。
//! 将连续相同 hookLabel 的 hook 摘要消息折叠为单个摘要。

use serde::{Deserialize, Serialize};

/// Hook 信息条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInfo {
    pub name: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Hook 错误条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookError {
    pub name: String,
    pub message: String,
}

/// 系统 stop hook 摘要消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStopHookSummaryMessage {
    pub hook_label: Option<String>,
    pub hook_count: usize,
    pub hook_infos: Vec<HookInfo>,
    pub hook_errors: Vec<HookError>,
    pub prevented_continuation: bool,
    pub has_output: bool,
    pub total_duration_ms: Option<u64>,
    pub uuid: String,
    pub timestamp: String,
}

/// 可渲染消息（简化版——实际使用 enum 变体）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RenderableMessage {
    #[serde(rename = "system_stop_hook_summary")]
    StopHookSummary(SystemStopHookSummaryMessage),
    #[serde(other)]
    Other,
}

/// 判断消息是否为带标签的 hook 摘要。
fn is_labeled_hook_summary(msg: &RenderableMessage) -> Option<&SystemStopHookSummaryMessage> {
    match msg {
        RenderableMessage::StopHookSummary(m) if m.hook_label.is_some() => Some(m),
        _ => None,
    }
}

/// 折叠连续具有相同 hookLabel 的 hook 摘要消息（如 PostToolUse）为单个摘要。
/// 当并行工具调用各自发出 hook 摘要时会发生这种情况。
pub fn collapse_hook_summaries(messages: Vec<RenderableMessage>) -> Vec<RenderableMessage> {
    let mut result: Vec<RenderableMessage> = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        if let Some(summary) = is_labeled_hook_summary(&messages[i]) {
            let label = summary.hook_label.as_ref().unwrap().clone();
            let mut group: Vec<&SystemStopHookSummaryMessage> = Vec::new();

            while i < messages.len() {
                if let Some(next_summary) = is_labeled_hook_summary(&messages[i]) {
                    if next_summary.hook_label.as_deref() == Some(&label) {
                        group.push(next_summary);
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            if group.len() == 1 {
                result.push(RenderableMessage::StopHookSummary(group[0].clone()));
            } else {
                let merged = SystemStopHookSummaryMessage {
                    hook_label: Some(label),
                    hook_count: group.iter().map(|m| m.hook_count).sum(),
                    hook_infos: group.iter().flat_map(|m| m.hook_infos.clone()).collect(),
                    hook_errors: group.iter().flat_map(|m| m.hook_errors.clone()).collect(),
                    prevented_continuation: group.iter().any(|m| m.prevented_continuation),
                    has_output: group.iter().any(|m| m.has_output),
                    total_duration_ms: Some(
                        group
                            .iter()
                            .map(|m| m.total_duration_ms.unwrap_or(0))
                            .max()
                            .unwrap_or(0),
                    ),
                    uuid: group[0].uuid.clone(),
                    timestamp: group[0].timestamp.clone(),
                };
                result.push(RenderableMessage::StopHookSummary(merged));
            }
        } else {
            result.push(messages[i].clone());
            i += 1;
        }
    }

    result
}
