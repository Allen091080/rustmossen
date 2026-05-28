//! PushNotificationTool prompt.
pub const PUSH_NOTIFICATION_TOOL_NAME: &str = "PushNotification";
pub const DESCRIPTION: &str = "Send a local push notification to the user";
pub const PUSH_NOTIFICATION_TOOL_PROMPT: &str = r#"Send a concise local push notification.

Use this only when the user is likely away and needs to be alerted about something important, such as task completion, required input, or a blocker.

Keep the title short and the body concise."#;
