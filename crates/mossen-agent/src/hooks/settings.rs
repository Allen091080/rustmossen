//! # settings — Hook 来源与配置
//!
//! 对应 TS `utils/hooks/hooksSettings.ts`。
//! 定义 `HookSource`、`IndividualHookConfig` 等类型。

use mossen_types::hooks::HookEvent;
use serde::{Deserialize, Serialize};

/// Hook 来源 — 标识 Hook 配置的出处。
///
/// 对应 TS `HookSource`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookSource {
    /// 用户设置 (~/.mossen/settings.json)。
    UserSettings,
    /// 项目设置 (.mossen/settings.json)。
    ProjectSettings,
    /// 本地设置 (.mossen/settings.local.json)。
    LocalSettings,
    /// 策略设置（管理员）。
    PolicySettings,
    /// 插件 Hook。
    PluginHook,
    /// 会话 Hook（临时、内存中）。
    SessionHook,
    /// 内建 Hook（Mossen 内部注册）。
    BuiltinHook,
}

impl HookSource {
    /// 来源描述字符串（用于 UI 显示）。
    ///
    /// 对应 TS `hookSourceDescriptionDisplayString()`。
    pub fn description(&self) -> &'static str {
        match self {
            Self::UserSettings => "User settings (~/.mossen/settings.json)",
            Self::ProjectSettings => "Project settings (.mossen/settings.json)",
            Self::LocalSettings => "Local settings (.mossen/settings.local.json)",
            Self::PolicySettings => "Policy settings (managed)",
            Self::PluginHook => "Plugin hooks (~/.mossen/plugins/*/hooks/hooks.json)",
            Self::SessionHook => "Session hooks (in-memory, temporary)",
            Self::BuiltinHook => "Built-in hooks (registered internally by Mossen)",
        }
    }

    /// 来源简短标题（用于 UI 标头）。
    ///
    /// 对应 TS `hookSourceHeaderDisplayString()`。
    pub fn header(&self) -> &'static str {
        match self {
            Self::UserSettings => "User Settings",
            Self::ProjectSettings => "Project Settings",
            Self::LocalSettings => "Local Settings",
            Self::PolicySettings => "Policy Settings",
            Self::PluginHook => "Plugin Hooks",
            Self::SessionHook => "Session Hooks",
            Self::BuiltinHook => "Built-in Hooks",
        }
    }

    /// 来源内联显示名。
    ///
    /// 对应 TS `hookSourceInlineDisplayString()`。
    pub fn inline_name(&self) -> &'static str {
        match self {
            Self::UserSettings => "User",
            Self::ProjectSettings => "Project",
            Self::LocalSettings => "Local",
            Self::PolicySettings => "Policy",
            Self::PluginHook => "Plugin",
            Self::SessionHook => "Session",
            Self::BuiltinHook => "Built-in",
        }
    }
}

/// Hook 命令类型 — 对应 TS `HookCommand` 的类型标签。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookCommand {
    /// Shell 命令 Hook。
    Command {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shell: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
    /// HTTP 请求 Hook。
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<std::collections::HashMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_env_vars: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
    /// LLM Prompt Hook。
    Prompt {
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
    /// Agent Hook（多轮 LLM 查询）。
    Agent {
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<f64>,
        #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
        condition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
}

impl HookCommand {
    /// 获取 Hook 类型名称。
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Command { .. } => "command",
            Self::Http { .. } => "http",
            Self::Prompt { .. } => "prompt",
            Self::Agent { .. } => "agent",
        }
    }

    /// 获取 Hook 的显示文本。
    ///
    /// 对应 TS `getHookDisplayText()`。
    pub fn display_text(&self) -> &str {
        match self {
            Self::Command { command, .. } => command.as_str(),
            Self::Http { url, .. } => url.as_str(),
            Self::Prompt { prompt, .. } => prompt.as_str(),
            Self::Agent { prompt, .. } => prompt.as_str(),
        }
    }

    /// 获取超时时间（秒）。
    pub fn timeout_secs(&self) -> Option<f64> {
        match self {
            Self::Command { timeout, .. }
            | Self::Http { timeout, .. }
            | Self::Prompt { timeout, .. }
            | Self::Agent { timeout, .. } => *timeout,
        }
    }

    /// 获取条件表达式。
    pub fn condition(&self) -> Option<&str> {
        match self {
            Self::Command { condition, .. }
            | Self::Http { condition, .. }
            | Self::Prompt { condition, .. }
            | Self::Agent { condition, .. } => condition.as_deref(),
        }
    }

    /// 获取 shell 名称（仅 Command 类型）。
    pub fn shell(&self) -> Option<&str> {
        match self {
            Self::Command { shell, .. } => shell.as_deref(),
            _ => None,
        }
    }
}

/// 判断两个 Hook 命令是否相等（不比较 timeout）。
///
/// 对应 TS `isHookEqual()`。
pub fn is_hook_equal(a: &HookCommand, b: &HookCommand) -> bool {
    let same_if = |a_cond: &Option<String>, b_cond: &Option<String>| -> bool {
        a_cond.as_deref().unwrap_or("") == b_cond.as_deref().unwrap_or("")
    };

    match (a, b) {
        (
            HookCommand::Command {
                command: a_cmd,
                shell: a_shell,
                condition: a_cond,
                ..
            },
            HookCommand::Command {
                command: b_cmd,
                shell: b_shell,
                condition: b_cond,
                ..
            },
        ) => {
            let default_shell = "bash";
            a_cmd == b_cmd
                && a_shell.as_deref().unwrap_or(default_shell)
                    == b_shell.as_deref().unwrap_or(default_shell)
                && same_if(a_cond, b_cond)
        }
        (
            HookCommand::Prompt {
                prompt: a_p,
                condition: a_cond,
                ..
            },
            HookCommand::Prompt {
                prompt: b_p,
                condition: b_cond,
                ..
            },
        ) => a_p == b_p && same_if(a_cond, b_cond),
        (
            HookCommand::Agent {
                prompt: a_p,
                condition: a_cond,
                ..
            },
            HookCommand::Agent {
                prompt: b_p,
                condition: b_cond,
                ..
            },
        ) => a_p == b_p && same_if(a_cond, b_cond),
        (
            HookCommand::Http {
                url: a_url,
                condition: a_cond,
                ..
            },
            HookCommand::Http {
                url: b_url,
                condition: b_cond,
                ..
            },
        ) => a_url == b_url && same_if(a_cond, b_cond),
        _ => false,
    }
}

/// Hook 匹配器 — 事件的匹配规则和关联的 Hook 命令列表。
///
/// 对应 TS `HookMatcher`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    /// 匹配器表达式（如工具名、通知类型等）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    /// 关联的 Hook 命令列表。
    pub hooks: Vec<HookCommand>,
}

/// 单个 Hook 配置 — 包含事件、命令配置、匹配器和来源。
///
/// 对应 TS `IndividualHookConfig`。
#[derive(Debug, Clone)]
pub struct IndividualHookConfig {
    /// Hook 事件。
    pub event: HookEvent,
    /// Hook 命令配置。
    pub config: HookCommand,
    /// 匹配器表达式（可选）。
    pub matcher: Option<String>,
    /// Hook 来源。
    pub source: HookSource,
    /// 插件名称（仅 pluginHook 来源）。
    pub plugin_name: Option<String>,
}

/// Hooks 设置 — 事件到匹配器列表的映射。
///
/// 对应 TS `HooksSettings`。
pub type HooksSettings = std::collections::HashMap<HookEvent, Vec<HookMatcher>>;
