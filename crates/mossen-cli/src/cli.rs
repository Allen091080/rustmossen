//! CLI 参数定义 — 使用 clap derive 宏构建类型安全的命令行接口。
//!
//! 对应 TS 的 Commander.js 解析逻辑（main.tsx 第 100-400 行）。
//! 命名遵循深度命名转换词典（文档 12）。

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Mossen — AI 编程助手 (Rust Edition)
#[derive(Parser, Debug)]
#[command(
    name = "mossen",
    version,
    about = "Mossen — AI 编程助手",
    long_about = "Mossen 是一个基于终端的 AI 编程助手，支持交互式对话、\
                  代码编辑、文件操作、Shell 命令执行等功能。"
)]
pub struct Cli {
    /// 单次执行模式：提交提示后直接输出结果并退出（非交互式）。
    /// 对应 TS 的 --print / -p。
    #[arg(short = '1', long = "oneshot", value_name = "PROMPT")]
    pub oneshot: Option<String>,

    /// 输出格式（仅在 --oneshot 模式下有效）。
    /// 对应 TS 的 --output-format。
    #[arg(long = "emit", value_enum, default_value_t = EmitFormat::Text)]
    pub emit: EmitFormat,

    /// 指定使用的模型。
    #[arg(short, long, value_name = "MODEL")]
    pub model: Option<String>,

    /// 访问策略（权限模式）。
    /// 对应 TS 的 --permission-mode。
    #[arg(long = "access-policy", value_enum)]
    pub access_policy: Option<AccessPolicyArg>,

    /// 最大轮次限制。
    /// 对应 TS 的 --max-turns。
    #[arg(long = "turn-limit", value_name = "N")]
    pub turn_limit: Option<u32>,

    /// 详细输出模式。
    #[arg(short, long)]
    pub verbose: bool,

    /// 恢复上次会话。
    /// 对应 TS 的 --resume。
    #[arg(long = "restore")]
    pub restore: bool,

    /// 恢复指定会话 ID。
    #[arg(long = "restore-id", value_name = "SESSION_ID")]
    pub restore_id: Option<String>,

    /// 包含额外目录（用于 MOSSEN.md 加载）。
    /// 对应 TS 的 --add-dir。
    #[arg(long = "include-dir", value_name = "DIR")]
    pub include_dir: Vec<PathBuf>,

    /// 指定可用工具集（逗号分隔）。
    /// 对应 TS 的 --tools。
    #[arg(long = "instruments", value_delimiter = ',')]
    pub instruments: Vec<String>,

    /// 禁用指定工具（逗号分隔）。
    #[arg(long = "disable-instruments", value_delimiter = ',')]
    pub disable_instruments: Vec<String>,

    /// 系统提示覆盖。
    #[arg(long = "system-prompt", value_name = "PROMPT")]
    pub system_prompt: Option<String>,

    /// 追加额外系统提示。
    /// 对应 TS 的 --append-system-prompt。
    #[arg(long = "extra-prompt", value_name = "PROMPT")]
    pub extra_prompt: Option<String>,

    /// 预算上限（美元）。
    /// 对应 TS 的 --max-budget-usd。
    #[arg(long = "budget", value_name = "USD")]
    pub budget: Option<f64>,

    /// JSON Schema（用于结构化输出）。
    #[arg(long = "schema", value_name = "SCHEMA")]
    pub schema: Option<String>,

    /// 工作目录。
    #[arg(short = 'C', long = "cwd", value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// 启用 worktree 模式。
    #[arg(long)]
    pub worktree: bool,

    /// 指定 worktree 名称。
    #[arg(long = "worktree-name", value_name = "NAME")]
    pub worktree_name: Option<String>,

    /// 启用 tmux 集成。
    #[arg(long)]
    pub tmux: bool,

    /// 跳过权限检查（危险！仅限沙箱环境）。
    #[arg(long = "dangerously-skip-permissions", hide = true)]
    pub dangerously_skip_permissions: bool,

    /// 初始化 MCP 服务器配置（JSON 格式）。
    #[arg(long = "mcp-config", value_name = "JSON")]
    pub mcp_config: Option<String>,

    /// 指定 Agent 类型。
    #[arg(long = "agent", value_name = "AGENT")]
    pub agent: Option<String>,

    /// 精简模式（跳过非必要功能）。
    #[arg(long = "bare")]
    pub bare: bool,

    /// 涡轮模式（快速响应）。
    /// 对应 TS 的 --fast。
    #[arg(long = "turbo")]
    pub turbo: bool,

    /// 思考深度控制。
    #[arg(long = "effort", value_enum)]
    pub effort: Option<EffortArg>,

    /// 输入文件（从文件读取提示）。
    #[arg(long = "input-file", value_name = "FILE")]
    pub input_file: Option<PathBuf>,

    /// 从 stdin 读取提示。
    #[arg(long = "stdin")]
    pub stdin: bool,

    /// 远程模式。
    #[arg(long = "remote", hide = true)]
    pub remote: bool,

    /// 继续上次对话并追加消息。
    #[arg(long = "continue", value_name = "PROMPT")]
    pub continue_prompt: Option<String>,

    /// 调试模式。
    #[arg(long)]
    pub debug: bool,

    /// 子命令。
    #[command(subcommand)]
    pub command: Option<SubCmd>,
}

/// 输出格式枚举。
#[derive(Debug, Clone, ValueEnum)]
pub enum EmitFormat {
    /// 纯文本输出。
    Text,
    /// JSON 格式输出。
    Json,
    /// 流式 JSON 输出（NDJSON）。
    StreamJson,
    /// 本进程直接渲染终端 UI（实验性 oneshot 前端）。
    Terminal,
}

/// 访问策略参数枚举。
/// 对应 TS 的 PermissionMode。
#[derive(Debug, Clone, ValueEnum)]
pub enum AccessPolicyArg {
    /// 受监督模式（默认，需要人工确认）。
    Supervised,
    /// 只读模式（计划模式，不执行写操作）。
    ReadOnly,
    /// 信任编辑模式。
    TrustEdits,
    /// 不受限模式（跳过所有权限检查）。
    Unrestricted,
    /// 自动拒绝模式。
    AutoDeny,
    /// Swift 模式（智能自动判断）。
    Swift,
}

/// 思考深度参数。
#[derive(Debug, Clone, ValueEnum)]
pub enum EffortArg {
    /// 低思考深度。
    Low,
    /// 中等思考深度。
    Medium,
    /// 高思考深度。
    High,
}

/// 子命令定义。
#[derive(Subcommand, Debug)]
pub enum SubCmd {
    /// 版本升级 (对应 /upgrade)。
    Evolve,

    /// 显示个人版后端凭据配置状态 (对应 /login)。
    Auth,

    /// 报告本地凭据登出状态 (对应 /logout)。
    Deauth,

    /// 诊断工具 (对应 /doctor)。
    Diagnose,

    /// 配置管理 (对应 /config)。
    Config {
        /// 配置操作：get, set, list, reset。
        #[arg(value_name = "ACTION")]
        action: Option<String>,
        /// 配置键。
        #[arg(value_name = "KEY")]
        key: Option<String>,
        /// 配置值。
        #[arg(value_name = "VALUE")]
        value: Option<String>,
    },

    /// MCP 桥接管理 (对应 /mcp)。
    Bridges {
        #[command(subcommand)]
        action: Option<BridgesSubCmd>,
    },

    /// 插件管理 (对应 /plugin)。
    Plugin {
        #[command(subcommand)]
        action: Option<PluginSubCmd>,
    },

    /// 安装管理。
    Install,

    /// 初始化新项目。
    Init,
}

/// MCP 桥接子命令。
#[derive(Subcommand, Debug)]
pub enum BridgesSubCmd {
    /// 列出已配置的 MCP 服务器。
    List,
    /// 添加 MCP 服务器。
    Add {
        /// 服务器名称。
        name: String,
        /// 服务器 URI 或命令。
        uri: String,
    },
    /// 移除 MCP 服务器。
    Remove {
        /// 服务器名称。
        name: String,
    },
    /// 显示 MCP 服务器状态。
    Status,
}

/// 插件子命令。
#[derive(Subcommand, Debug)]
pub enum PluginSubCmd {
    /// 安装插件。
    Install {
        /// 插件名称或 URL。
        name: String,
    },
    /// 卸载插件。
    Uninstall {
        /// 插件名称。
        name: String,
    },
    /// 列出已安装插件。
    List,
    /// 启用插件。
    Enable {
        /// 插件名称。
        name: String,
    },
    /// 禁用插件。
    Disable {
        /// 插件名称。
        name: String,
    },
}

impl Cli {
    /// 判断是否为非交互式（oneshot）模式。
    pub fn is_non_interactive(&self) -> bool {
        self.oneshot.is_some() || self.command.is_some() || self.stdin
    }

    /// 判断是否需要恢复会话。
    pub fn should_restore(&self) -> bool {
        self.restore || self.restore_id.is_some() || self.continue_prompt.is_some()
    }

    /// 获取有效的工作目录。
    pub fn effective_cwd(&self) -> std::io::Result<PathBuf> {
        if let Some(ref cwd) = self.cwd {
            Ok(cwd.clone())
        } else {
            std::env::current_dir()
        }
    }
}
