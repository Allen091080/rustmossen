//! # commands — 顶层命令注册与过滤
//!
//! 对应 TypeScript `commands.ts` (顶层入口；不要与 `commands/` 目录混淆)。
//! 负责把内置命令、技能、插件、工作流合并为统一列表，并按当前用户/会话
//! 状态过滤可用性。
//!
//! 这里只保留 TS 中“纯数据”的部分；TS 里的终端渲染相关分支由
//! mossen-tui/mossen-cli 各自承担。

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use mossen_types::command::{CommandAvailability, CommandBase, CommandLoadedFrom, CommandType};

/// 一个统一的命令记录 — 对应 TS `Command` 的子集。
#[derive(Debug, Clone)]
pub struct Command {
    pub base: CommandBase,
    pub command_type: CommandType,
    pub source: CommandSource,
}

/// 命令来源标记，用于过滤“仅内置 / 仅 MCP / 仅技能”等场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandSource {
    Builtin,
    Skill,
    Plugin,
    Mcp,
    Workflow,
    Bundled,
    External,
}

// ---------------------------------------------------------------------------
// 静态命令清单（TS 中由各 commands/*.tsx 模块在导入时注册）
// ---------------------------------------------------------------------------

/// `commands.ts` `INTERNAL_ONLY_COMMANDS` — 仅供内部使用、不暴露给用户。
pub const INTERNAL_ONLY_COMMANDS: &[&str] = &["internal-debug", "internal-eval", "internal-trace"];

/// `commands.ts` `builtInCommandNames` — 内置命令名集合。线程安全、惰性初始化。
pub fn built_in_command_names() -> Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            vec![
                "help",
                "init",
                "review",
                "security-review",
                "model",
                "config",
                "compact",
                "clear",
                "export",
                "exit",
                "logout",
                "login",
                "auth",
                "doctor",
                "feedback",
                "release-notes",
                "plugin",
                "mcp",
                "ide",
                "permissions",
                "memory",
                "agents",
                "vim",
                "approve",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        })
        .clone()
}

// ---------------------------------------------------------------------------
// 可用性
// ---------------------------------------------------------------------------

/// 当前用户状态契约 — 由调用方传入。Rust 端不假设全局 auth 状态。
#[derive(Debug, Clone, Copy, Default)]
pub struct AvailabilityContext {
    /// 是否为 hosted（Provider 托管账号）订阅者。
    pub is_hosted_subscriber: bool,
    /// 是否使用 Bedrock/Vertex/Foundry 等第三方推理服务。
    pub is_using_3p_services: bool,
    /// 是否连接到 1P provider base URL。
    pub is_first_party_base_url: bool,
}

/// `commands.ts` `meetsAvailabilityRequirement`。
pub fn meets_availability_requirement(cmd: &Command, ctx: &AvailabilityContext) -> bool {
    let Some(avail) = &cmd.base.availability else {
        return true;
    };
    for a in avail {
        match a {
            CommandAvailability::Hosted => {
                if ctx.is_hosted_subscriber {
                    return true;
                }
            }
            CommandAvailability::Console => {
                if !ctx.is_hosted_subscriber
                    && !ctx.is_using_3p_services
                    && ctx.is_first_party_base_url
                {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// 加载 & 过滤
// ---------------------------------------------------------------------------

fn memo_cache() -> &'static Mutex<HashMap<String, Vec<Command>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<Command>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `commands.ts` `getCommands`。
///
/// 调用方负责通过 [`register_command_sources`] 把各来源（内置、技能、插件、
/// 工作流）注册进来；本函数把它们按 TS 顺序拼接并过滤。
pub fn get_commands(cwd: &str, ctx: &AvailabilityContext) -> Vec<Command> {
    if let Some(cached) = memo_cache().lock().unwrap().get(cwd) {
        return cached
            .iter()
            .filter(|c| meets_availability_requirement(c, ctx) && is_command_enabled(c))
            .cloned()
            .collect();
    }
    let all = collect_registered_sources();
    memo_cache()
        .lock()
        .unwrap()
        .insert(cwd.to_string(), all.clone());
    all.into_iter()
        .filter(|c| meets_availability_requirement(c, ctx) && is_command_enabled(c))
        .collect()
}

/// `commands.ts` `clearCommandMemoizationCaches`。
pub fn clear_command_memoization_caches() {
    memo_cache().lock().unwrap().clear();
    skill_tool_cache().lock().unwrap().clear();
    slash_skill_cache().lock().unwrap().clear();
}

/// `commands.ts` `clearCommandsCache`。
pub fn clear_commands_cache() {
    clear_command_memoization_caches();
    ensure_sources().lock().unwrap().clear();
}

fn is_command_enabled(cmd: &Command) -> bool {
    !cmd.base.is_hidden.unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 来源注册（替代 TS 中的动态 import 副作用）
// ---------------------------------------------------------------------------

fn sources_cell() -> &'static Mutex<Vec<Command>> {
    static CELL: OnceLock<Mutex<Vec<Command>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

static SOURCES: OnceLock<Mutex<Vec<Command>>> = OnceLock::new();

fn ensure_sources() -> &'static Mutex<Vec<Command>> {
    SOURCES.get_or_init(|| Mutex::new(Vec::new()))
}

/// 注册一批命令来源（例如某个技能子系统）。
pub fn register_command_sources(mut cmds: Vec<Command>) {
    ensure_sources().lock().unwrap().append(&mut cmds);
    let _ = sources_cell();
}

fn collect_registered_sources() -> Vec<Command> {
    ensure_sources().lock().unwrap().clone()
}

// ---------------------------------------------------------------------------
// 技能子集
// ---------------------------------------------------------------------------

static SKILL_TOOL_CACHE: OnceLock<Mutex<HashMap<String, Vec<Command>>>> = OnceLock::new();
fn skill_tool_cache() -> &'static Mutex<HashMap<String, Vec<Command>>> {
    SKILL_TOOL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

static SLASH_SKILL_CACHE: OnceLock<Mutex<HashMap<String, Vec<Command>>>> = OnceLock::new();
fn slash_skill_cache() -> &'static Mutex<HashMap<String, Vec<Command>>> {
    SLASH_SKILL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `commands.ts` `getMcpSkillCommands`。
pub fn get_mcp_skill_commands(
    mcp_commands: &[Command],
    mcp_skills_feature_on: bool,
) -> Vec<Command> {
    if !mcp_skills_feature_on {
        return Vec::new();
    }
    mcp_commands
        .iter()
        .filter(|cmd| {
            matches!(cmd.command_type, CommandType::Prompt)
                && cmd.base.loaded_from == Some(CommandLoadedFrom::Mcp)
                && !cmd.base.disable_model_invocation.unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// `commands.ts` `getSkillToolCommands`。
pub fn get_skill_tool_commands(cwd: &str, ctx: &AvailabilityContext) -> Vec<Command> {
    if let Some(v) = skill_tool_cache().lock().unwrap().get(cwd) {
        return v.clone();
    }
    let cmds = get_commands(cwd, ctx)
        .into_iter()
        .filter(|cmd| {
            matches!(cmd.command_type, CommandType::Prompt)
                && !cmd.base.disable_model_invocation.unwrap_or(false)
                && cmd.source != CommandSource::Builtin
                && (matches!(
                    cmd.base.loaded_from,
                    Some(CommandLoadedFrom::Bundled) | Some(CommandLoadedFrom::Skills)
                ) || cmd.base.has_user_specified_description.unwrap_or(false)
                    || cmd.base.when_to_use.is_some())
        })
        .collect::<Vec<_>>();
    skill_tool_cache()
        .lock()
        .unwrap()
        .insert(cwd.to_string(), cmds.clone());
    cmds
}

/// `commands.ts` `getSlashCommandToolSkills`。
pub fn get_slash_command_tool_skills(cwd: &str, ctx: &AvailabilityContext) -> Vec<Command> {
    if let Some(v) = slash_skill_cache().lock().unwrap().get(cwd) {
        return v.clone();
    }
    let cmds = get_commands(cwd, ctx)
        .into_iter()
        .filter(|cmd| {
            matches!(cmd.command_type, CommandType::Prompt)
                && cmd.source != CommandSource::Builtin
                && (cmd.base.has_user_specified_description.unwrap_or(false)
                    || cmd.base.when_to_use.is_some())
                && (matches!(
                    cmd.base.loaded_from,
                    Some(CommandLoadedFrom::Skills)
                        | Some(CommandLoadedFrom::Plugin)
                        | Some(CommandLoadedFrom::Bundled)
                ) || cmd.base.disable_model_invocation.unwrap_or(false))
        })
        .collect::<Vec<_>>();
    slash_skill_cache()
        .lock()
        .unwrap()
        .insert(cwd.to_string(), cmds.clone());
    cmds
}

// ---------------------------------------------------------------------------
// Remote / bridge 过滤集合
// ---------------------------------------------------------------------------

/// `commands.ts` `REMOTE_SAFE_COMMANDS` — 在远程会话中仍可用的命令名集合。
pub fn remote_safe_commands() -> &'static HashSet<&'static str> {
    static CACHE: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CACHE.get_or_init(|| {
        [
            "help",
            "clear",
            "exit",
            "logout",
            "model",
            "feedback",
            "memory",
            "release-notes",
            "review",
            "security-review",
            "doctor",
        ]
        .into_iter()
        .collect()
    })
}

/// `commands.ts` `BRIDGE_SAFE_COMMANDS` — 桥接（mossen bridge）下可执行的命令。
pub fn bridge_safe_commands() -> &'static HashSet<&'static str> {
    static CACHE: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut s = remote_safe_commands().clone();
        s.extend(["init", "config", "permissions", "agents"].into_iter());
        s
    })
}

/// `commands.ts` `isBridgeSafeCommand`。
pub fn is_bridge_safe_command(cmd: &Command) -> bool {
    bridge_safe_commands().contains(cmd.base.name.as_str())
}

/// `commands.ts` `filterCommandsForRemoteMode`。
pub fn filter_commands_for_remote_mode(commands: &[Command]) -> Vec<Command> {
    let allow = remote_safe_commands();
    commands
        .iter()
        .filter(|c| allow.contains(c.base.name.as_str()))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// 查找辅助
// ---------------------------------------------------------------------------

/// `commands.ts` `findCommand`。
pub fn find_command<'a>(name: &str, commands: &'a [Command]) -> Option<&'a Command> {
    commands.iter().find(|c| {
        c.base.name == name
            || c.base
                .aliases
                .as_ref()
                .map(|a| a.iter().any(|s| s == name))
                .unwrap_or(false)
    })
}

/// `commands.ts` `hasCommand`。
pub fn has_command(name: &str, commands: &[Command]) -> bool {
    find_command(name, commands).is_some()
}

/// `commands.ts` `getCommand`。Panics if missing — mirrors TS throw.
pub fn get_command<'a>(name: &str, commands: &'a [Command]) -> &'a Command {
    find_command(name, commands).expect("command not found")
}

/// `commands.ts` `formatDescriptionWithSource`。
pub fn format_description_with_source(cmd: &Command) -> String {
    let src = match cmd.base.loaded_from {
        Some(CommandLoadedFrom::Bundled) => " (bundled)",
        Some(CommandLoadedFrom::Skills) => " (skill)",
        Some(CommandLoadedFrom::Plugin) => " (plugin)",
        Some(CommandLoadedFrom::Mcp) => " (mcp)",
        _ => "",
    };
    format!("{}{}", cmd.base.description, src)
}
