//! # executor — 技能执行引擎
//!
//! 对应 TypeScript `skills/loadSkillsDir.ts` 中的 `getPromptForCommand` 逻辑
//! 以及 `skills/mcpSkills.ts` 中的 MCP 技能获取。
//! 负责技能调用时的参数替换、shell 命令执行、内容组装。

use std::collections::HashMap;

use tracing::{debug, warn};

use crate::skill::{ContentBlock, CraftCommand};

// ---------------------------------------------------------------------------
// 技能执行
// ---------------------------------------------------------------------------

/// 技能执行上下文。
pub struct CraftExecutionContext {
    /// 当前会话 ID。
    pub session_id: String,
    /// 当前工作目录。
    pub cwd: String,
    /// 平台（darwin / win32 / linux）。
    pub platform: String,
}

/// 执行技能命令，生成 prompt 内容块。
///
/// 对应 TS `getPromptForCommand(args, toolUseContext)` 的逻辑。
pub async fn execute_craft(
    craft: &CraftCommand,
    args: &str,
    context: &CraftExecutionContext,
) -> Vec<ContentBlock> {
    let markdown = craft.markdown_content.as_deref().unwrap_or("");
    let base_dir = craft.skill_root.as_deref();

    // 1. 可选前缀 base directory
    let mut content = match base_dir {
        Some(dir) => format!("Base directory for this skill: {}\n\n{}", dir, markdown),
        None => markdown.to_string(),
    };

    // 2. 参数替换
    if let Some(arg_names) = &craft.prompt_data.arg_names {
        content = substitute_arguments(&content, args, arg_names);
    } else if !args.trim().is_empty() {
        // 如果没有命名参数，将 args 作为 $ARGUMENTS 替换
        content = content.replace("$ARGUMENTS", args.trim());
    }

    // 3. 替换 ${MOSSEN_SKILL_DIR}
    if let Some(dir) = base_dir {
        let skill_dir = if context.platform == "win32" {
            dir.replace('\\', "/")
        } else {
            dir.to_string()
        };
        content = content.replace("${MOSSEN_SKILL_DIR}", &skill_dir);
    }

    // 4. 替换 ${MOSSEN_SESSION_ID}
    content = content.replace("${MOSSEN_SESSION_ID}", &context.session_id);

    vec![ContentBlock::Text { text: content }]
}

/// Wrap a rendered skill prompt with the command tags consumed by the model
/// prompt and transcript recovery paths.
pub fn format_invoked_skill_prompt(skill_name: &str, args: &str, rendered_prompt: &str) -> String {
    let tags = mossen_utils::messages::format_command_input_tags(skill_name, args);
    let body = rendered_prompt.trim();
    if body.is_empty() {
        tags
    } else {
        format!("{tags}\n\n{body}")
    }
}

/// 参数替换。
///
/// 对应 TS `substituteArguments(content, args, true, argumentNames)`。
fn substitute_arguments(content: &str, args: &str, arg_names: &[String]) -> String {
    let mut result = content.to_string();
    let parts: Vec<&str> = args.splitn(arg_names.len().max(1), ' ').collect();

    for (i, name) in arg_names.iter().enumerate() {
        let value = parts.get(i).unwrap_or(&"");
        let placeholder = format!("${{{}}}", name);
        result = result.replace(&placeholder, value);

        // 也支持 $name 格式（无大括号）
        let alt_placeholder = format!("${}", name);
        result = result.replace(&alt_placeholder, value);
    }

    // 替换剩余的 $ARGUMENTS
    result = result.replace("$ARGUMENTS", args.trim());

    result
}

// ---------------------------------------------------------------------------
// 技能查找
// ---------------------------------------------------------------------------

/// 按名称查找技能。
pub fn find_craft_by_name<'a>(crafts: &'a [CraftCommand], name: &str) -> Option<&'a CraftCommand> {
    crafts.iter().find(|c| {
        c.name() == name
            || c.base
                .aliases
                .as_ref()
                .map_or(false, |aliases| aliases.iter().any(|a| a == name))
    })
}

/// 按名称查找技能（在多个来源中）。
pub fn find_craft_in_sources<'a>(
    name: &str,
    sources: &[&'a [CraftCommand]],
) -> Option<&'a CraftCommand> {
    for source in sources {
        if let Some(craft) = find_craft_by_name(source, name) {
            return Some(craft);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 技能命令参数解析
// ---------------------------------------------------------------------------

/// 解析 /skills 命令参数。
///
/// 对应 TS `parseSkillsArgs()`。
#[derive(Debug, Clone)]
pub enum ParsedSkillsCommand {
    /// 显示菜单。
    Menu,
    /// 显示帮助。
    Help,
    /// 安装技能。
    Install {
        target: Option<String>,
        confirm_token: Option<String>,
    },
}

/// 解析技能子命令参数。
pub fn parse_skills_args(args: Option<&str>) -> ParsedSkillsCommand {
    let trimmed = match args {
        Some(s) => s.trim(),
        None => return ParsedSkillsCommand::Menu,
    };

    if trimmed.is_empty() {
        return ParsedSkillsCommand::Menu;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let command = parts.first().map(|s| s.to_lowercase());

    match command.as_deref() {
        Some("help" | "--help" | "-h") => ParsedSkillsCommand::Help,
        Some("install" | "i") => {
            let confirm_idx = parts.iter().position(|p| *p == "--confirm");
            let confirm_token = confirm_idx.and_then(|i| parts.get(i + 1).map(|s| s.to_string()));
            let target_parts: Vec<&&str> = parts[1..]
                .iter()
                .enumerate()
                .filter(|(idx, _)| {
                    let abs_idx = idx + 1;
                    Some(abs_idx) != confirm_idx
                        && confirm_idx.map(|ci| abs_idx != ci + 1).unwrap_or(true)
                })
                .map(|(_, p)| p)
                .collect();
            let target = {
                let joined: String = target_parts
                    .iter()
                    .map(|p| **p)
                    .collect::<Vec<_>>()
                    .join(" ");
                let trimmed = joined.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            };
            ParsedSkillsCommand::Install {
                target,
                confirm_token,
            }
        }
        _ => ParsedSkillsCommand::Menu,
    }
}

// ---------------------------------------------------------------------------
// 插件命令参数解析
// ---------------------------------------------------------------------------

/// 解析 /plugin 命令参数。
///
/// 对应 TS `parsePluginArgs()`。
#[derive(Debug, Clone)]
pub enum ParsedPluginCommand {
    /// 显示菜单。
    Menu,
    /// 显示帮助。
    Help,
    /// 安装。
    Install {
        marketplace: Option<String>,
        plugin: Option<String>,
    },
    /// 安装计划（dry-run / confirm）。
    InstallPlan {
        plugin: Option<String>,
        scope: Option<String>,
        confirm_token: Option<String>,
    },
    /// 管理。
    Manage,
    /// 卸载。
    Uninstall { plugin: Option<String> },
    /// 启用。
    Enable { plugin: Option<String> },
    /// 禁用。
    Disable { plugin: Option<String> },
    /// 验证。
    Validate { path: Option<String> },
    /// 市场操作。
    Marketplace {
        action: Option<String>,
        target: Option<String>,
    },
    /// 市场添加计划。
    MarketplaceAddPlan {
        target: Option<String>,
        confirm_token: Option<String>,
    },
    /// 清理。
    Prune { confirm_token: Option<String> },
    /// 状态。
    Status,
    /// 来源。
    Sources,
    /// 路径。
    Paths,
}

/// 解析插件子命令参数。
pub fn parse_plugin_args(args: Option<&str>) -> ParsedPluginCommand {
    let trimmed = match args {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => return ParsedPluginCommand::Menu,
    };

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let command = parts.first().map(|s| s.to_lowercase());

    match command.as_deref() {
        Some("help" | "--help" | "-h") => ParsedPluginCommand::Help,
        Some("install" | "i") => {
            if parts.get(1) == Some(&"--dry-run") {
                let scope_idx = parts.iter().position(|p| *p == "--scope");
                let scope = scope_idx.and_then(|i| parts.get(i + 1).map(|s| s.to_string()));
                let plugin_str: String = parts[2..]
                    .iter()
                    .filter(|p| {
                        **p != "--scope"
                            && scope_idx.map_or(true, |si| {
                                parts.iter().position(|x| x == *p) != Some(si + 1)
                            })
                    })
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
                let plugin = if plugin_str.trim().is_empty() {
                    None
                } else {
                    Some(plugin_str.trim().to_string())
                };
                return ParsedPluginCommand::InstallPlan {
                    plugin,
                    scope,
                    confirm_token: None,
                };
            }
            if parts.get(1) == Some(&"--confirm") {
                return ParsedPluginCommand::InstallPlan {
                    plugin: None,
                    scope: None,
                    confirm_token: parts.get(2).map(|s| s.to_string()),
                };
            }
            let target = parts.get(1);
            match target {
                None => ParsedPluginCommand::Install {
                    marketplace: None,
                    plugin: None,
                },
                Some(t) => {
                    if t.contains('@') {
                        let mut split = t.splitn(2, '@');
                        let plugin = split.next().map(|s| s.to_string());
                        let marketplace = split.next().map(|s| s.to_string());
                        ParsedPluginCommand::Install {
                            marketplace,
                            plugin,
                        }
                    } else {
                        ParsedPluginCommand::Install {
                            marketplace: None,
                            plugin: Some(t.to_string()),
                        }
                    }
                }
            }
        }
        Some("manage") => ParsedPluginCommand::Manage,
        Some("uninstall") => ParsedPluginCommand::Uninstall {
            plugin: parts.get(1).map(|s| s.to_string()),
        },
        Some("enable") => ParsedPluginCommand::Enable {
            plugin: parts.get(1).map(|s| s.to_string()),
        },
        Some("disable") => ParsedPluginCommand::Disable {
            plugin: parts.get(1).map(|s| s.to_string()),
        },
        Some("validate") => {
            let path = parts[1..].join(" ");
            ParsedPluginCommand::Validate {
                path: if path.trim().is_empty() {
                    None
                } else {
                    Some(path.trim().to_string())
                },
            }
        }
        Some("status" | "stat") => ParsedPluginCommand::Status,
        Some("sources" | "source") => ParsedPluginCommand::Sources,
        Some("paths" | "path") => ParsedPluginCommand::Paths,
        Some("prune") => {
            let flag_idx = parts.iter().position(|p| *p == "--confirm");
            let confirm_token = flag_idx.and_then(|i| parts.get(i + 1).map(|s| s.to_string()));
            ParsedPluginCommand::Prune { confirm_token }
        }
        Some("marketplace" | "market") => {
            let action = parts.get(1).map(|s| s.to_lowercase());
            let rest = &parts[2.min(parts.len())..];
            match action.as_deref() {
                Some("add") => {
                    if rest.first() == Some(&"--dry-run") {
                        ParsedPluginCommand::MarketplaceAddPlan {
                            target: Some(rest[1..].join(" ")),
                            confirm_token: None,
                        }
                    } else if rest.first() == Some(&"--confirm") {
                        ParsedPluginCommand::MarketplaceAddPlan {
                            target: None,
                            confirm_token: rest.get(1).map(|s| s.to_string()),
                        }
                    } else {
                        ParsedPluginCommand::Marketplace {
                            action: Some("add".to_string()),
                            target: Some(rest.join(" ")),
                        }
                    }
                }
                Some("remove" | "rm") => ParsedPluginCommand::Marketplace {
                    action: Some("remove".to_string()),
                    target: Some(rest.join(" ")),
                },
                Some("update") => ParsedPluginCommand::Marketplace {
                    action: Some("update".to_string()),
                    target: Some(rest.join(" ")),
                },
                Some("list") => ParsedPluginCommand::Marketplace {
                    action: Some("list".to_string()),
                    target: None,
                },
                _ => ParsedPluginCommand::Marketplace {
                    action: None,
                    target: None,
                },
            }
        }
        _ => ParsedPluginCommand::Menu,
    }
}
