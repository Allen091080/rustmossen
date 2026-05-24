//! # collapse_teammate_shutdowns — 队友关闭消息折叠工具
//!
//! 对应 TypeScript `utils/collapseTeammateShutdowns.ts`。
//! 将连续的进程内队友关闭 task_status 附件折叠为带计数的单个附件。

use serde::{Deserialize, Serialize};

/// 队友关闭批量附件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateShutdownBatch {
    pub count: usize,
}

/// 任务状态附件（简化）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusAttachment {
    pub task_type: String,
    pub status: String,
}

/// 可渲染消息类型。
#[derive(Debug, Clone)]
pub enum CollapseRenderableMessage {
    TeammateShutdown {
        uuid: String,
        timestamp: String,
        task_type: String,
        status: String,
    },
    TeammateShutdownBatch {
        uuid: String,
        timestamp: String,
        count: usize,
    },
    Other(serde_json::Value),
}

/// 判断消息是否为队友关闭附件。
fn is_teammate_shutdown(msg: &CollapseRenderableMessage) -> bool {
    matches!(
        msg,
        CollapseRenderableMessage::TeammateShutdown {
            task_type,
            status,
            ..
        } if task_type == "in_process_teammate" && status == "completed"
    )
}

/// 折叠连续的进程内队友关闭 task_status 附件为带计数的单个
/// `teammate_shutdown_batch` 附件。
pub fn collapse_teammate_shutdowns(
    messages: Vec<CollapseRenderableMessage>,
) -> Vec<CollapseRenderableMessage> {
    let mut result: Vec<CollapseRenderableMessage> = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        if is_teammate_shutdown(&messages[i]) {
            let first_uuid;
            let first_timestamp;
            match &messages[i] {
                CollapseRenderableMessage::TeammateShutdown {
                    uuid, timestamp, ..
                } => {
                    first_uuid = uuid.clone();
                    first_timestamp = timestamp.clone();
                }
                _ => {
                    i += 1;
                    continue;
                }
            }

            let mut count = 0;
            while i < messages.len() && is_teammate_shutdown(&messages[i]) {
                count += 1;
                i += 1;
            }

            if count == 1 {
                result.push(CollapseRenderableMessage::TeammateShutdown {
                    uuid: first_uuid,
                    timestamp: first_timestamp,
                    task_type: "in_process_teammate".to_string(),
                    status: "completed".to_string(),
                });
            } else {
                result.push(CollapseRenderableMessage::TeammateShutdownBatch {
                    uuid: first_uuid,
                    timestamp: first_timestamp,
                    count,
                });
            }
        } else {
            result.push(messages[i].clone());
            i += 1;
        }
    }

    result
}
