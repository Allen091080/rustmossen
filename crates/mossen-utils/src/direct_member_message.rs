//! 直接成员消息解析与发送工具。
//!
//! 翻译自 `utils/directMemberMessage.ts`。

use std::collections::HashMap;
use std::future::Future;

use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

/// 解析 `@agent-name message` 语法的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDirectMessage {
    pub recipient_name: String,
    pub message: String,
}

/// 用于解析直接消息的正则表达式。
static DIRECT_MSG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^@([\w-]+)\s+(.+)$").unwrap());

/// 解析 `@agent-name message` 语法的直接团队成员消息。
pub fn parse_direct_member_message(input: &str) -> Option<ParsedDirectMessage> {
    let caps = DIRECT_MSG_RE.captures(input)?;

    let recipient_name = caps.get(1)?.as_str();
    let message = caps.get(2)?.as_str();

    if recipient_name.is_empty() || message.is_empty() {
        return None;
    }

    let trimmed_message = message.trim();
    if trimmed_message.is_empty() {
        return None;
    }

    Some(ParsedDirectMessage {
        recipient_name: recipient_name.to_string(),
        message: trimmed_message.to_string(),
    })
}

/// 直接消息发送结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectMessageResult {
    /// 发送成功。
    Success { recipient_name: String },
    /// 没有团队上下文。
    NoTeamContext,
    /// 未知收件人。
    UnknownRecipient { recipient_name: String },
}

/// 邮箱消息体。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MailboxMessage {
    pub from: String,
    pub text: String,
    pub timestamp: String,
}

/// 团队成员信息。
#[derive(Debug, Clone)]
pub struct Teammate {
    pub name: String,
}

/// 团队上下文。
#[derive(Debug, Clone)]
pub struct TeamContext {
    pub team_name: String,
    pub teammates: HashMap<String, Teammate>,
}

/// 发送直接消息给团队成员，绕过模型。
pub async fn send_direct_member_message<F, Fut>(
    recipient_name: &str,
    message: &str,
    team_context: Option<&TeamContext>,
    write_to_mailbox: Option<F>,
) -> DirectMessageResult
where
    F: FnOnce(&str, MailboxMessage, &str) -> Fut,
    Fut: Future<Output = ()>,
{
    let (ctx, write_fn) = match (team_context, write_to_mailbox) {
        (Some(ctx), Some(f)) => (ctx, f),
        _ => return DirectMessageResult::NoTeamContext,
    };

    // 按名称查找团队成员
    let member = ctx.teammates.values().find(|t| t.name == recipient_name);

    if member.is_none() {
        return DirectMessageResult::UnknownRecipient {
            recipient_name: recipient_name.to_string(),
        };
    }

    let msg = MailboxMessage {
        from: "user".to_string(),
        text: message.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    write_fn(recipient_name, msg, &ctx.team_name).await;

    DirectMessageResult::Success {
        recipient_name: recipient_name.to_string(),
    }
}
