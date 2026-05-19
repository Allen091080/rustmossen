//! # collapse_background_bash_notifications — 折叠后台 Bash 通知
//!
//! 对应 TypeScript `utils/collapseBackgroundBashNotifications.ts`。

/// XML 标签常量
const STATUS_TAG: &str = "status";
const SUMMARY_TAG: &str = "summary";
const TASK_NOTIFICATION_TAG: &str = "task-notification";
const BACKGROUND_BASH_SUMMARY_PREFIX: &str = "Background command";

/// 可渲染消息类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    User,
    Assistant,
    System,
}

/// 消息内容块
#[derive(Debug, Clone)]
pub struct TextContent {
    pub text: String,
}

/// 消息内容
#[derive(Debug, Clone)]
pub struct MessageContent {
    pub content: Vec<TextContent>,
}

/// 可渲染消息
#[derive(Debug, Clone)]
pub struct RenderableMessage {
    pub msg_type: MessageType,
    pub message: MessageContent,
}

/// 从文本中提取 XML 标签内容
fn extract_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = text.find(&open_tag)?;
    let content_start = start + open_tag.len();
    let end = text[content_start..].find(&close_tag)?;
    Some(&text[content_start..content_start + end])
}

/// 判断消息是否为已完成的后台 bash 通知
fn is_completed_background_bash(msg: &RenderableMessage) -> bool {
    if msg.msg_type != MessageType::User {
        return false;
    }
    let content = match msg.message.content.first() {
        Some(c) => c,
        None => return false,
    };

    let open_tag = format!("<{}", TASK_NOTIFICATION_TAG);
    if !content.text.contains(&open_tag) {
        return false;
    }

    // 仅折叠成功完成 — 失败/终止的保持单独可见
    if extract_tag(&content.text, STATUS_TAG) != Some("completed") {
        return false;
    }

    // 前缀常量区分 bash 类型的 LocalShellTask 完成与代理/工作流/监控通知
    match extract_tag(&content.text, SUMMARY_TAG) {
        Some(summary) => summary.starts_with(BACKGROUND_BASH_SUMMARY_PREFIX),
        None => false,
    }
}

/// 检查全屏环境是否启用 —— 转发到 [`crate::fullscreen::is_fullscreen_env_enabled`]，
/// 与 TS `isFullscreenEnvEnabled` 一致：读取 `MOSSEN_CODE_NO_FLICKER`、`USER_TYPE`
/// 并自动在 tmux -CC 下关闭。
fn is_fullscreen_env_enabled() -> bool {
    let user_type = std::env::var("USER_TYPE").ok();
    crate::fullscreen::is_fullscreen_env_enabled(user_type.as_deref())
}

/// 折叠连续的已完成后台 bash 任务通知为单个合成的
/// "N background commands completed" 通知。失败/终止的任务和
/// 代理/工作流通知保持不变。
///
/// 在详细模式下透传，以便 ctrl+O 显示每个完成通知。
pub fn collapse_background_bash_notifications(
    messages: &[RenderableMessage],
    verbose: bool,
) -> Vec<RenderableMessage> {
    if !is_fullscreen_env_enabled() {
        return messages.to_vec();
    }
    if verbose {
        return messages.to_vec();
    }

    let mut result: Vec<RenderableMessage> = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        let msg = &messages[i];
        if is_completed_background_bash(msg) {
            let mut count = 0;
            let first_msg = msg.clone();
            while i < messages.len() && is_completed_background_bash(&messages[i]) {
                count += 1;
                i += 1;
            }
            if count == 1 {
                result.push(first_msg);
            } else {
                // 合成一个 UserAgentNotificationMessage 已知如何渲染的任务通知
                result.push(RenderableMessage {
                    msg_type: MessageType::User,
                    message: MessageContent {
                        content: vec![TextContent {
                            text: format!(
                                "<{tag}><{status}>completed</{status}><{summary}>{count} background commands completed</{summary}></{tag}>",
                                tag = TASK_NOTIFICATION_TAG,
                                status = STATUS_TAG,
                                summary = SUMMARY_TAG,
                                count = count,
                            ),
                        }],
                    },
                });
            }
        } else {
            result.push(msg.clone());
            i += 1;
        }
    }

    result
}
