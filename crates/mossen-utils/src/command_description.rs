//! Command description localization utilities.
//!
//! Provides localized descriptions for built-in slash commands.

use std::collections::HashMap;

/// Get the localized command description for a command.
pub fn get_localized_command_description(
    name: &str,
    description: &str,
    cmd_type: &str,
    source: Option<&str>,
    product_name: &str,
    assistant_name: &str,
    current_model: &str,
    sandbox_description: &str,
    terminal_setup_description: &str,
    has_i18n_key: impl Fn(&str) -> bool,
    translate: impl Fn(&str, &HashMap<String, String>) -> Option<String>,
    language: &str,
) -> String {
    let should_use_builtin = cmd_type != "prompt" || source == Some("builtin") || source == Some("bundled");

    if should_use_builtin {
        let i18n_key = format!("cmd.{}.description", name);
        if has_i18n_key(&i18n_key) {
            let mut params = HashMap::new();
            params.insert("product".to_string(), product_name.to_string());
            params.insert("assistant".to_string(), assistant_name.to_string());
            if let Some(translated) = translate(&i18n_key, &params) {
                return translated;
            }
        }
    }

    if language == "zh" && should_use_builtin {
        if let Some(zh) = get_chinese_builtin_command_description(
            name, product_name, assistant_name, current_model, sandbox_description, terminal_setup_description,
        ) {
            return zh;
        }
    }

    description.to_string()
}

fn localize_sandbox_description(description: &str) -> String {
    let icon = description.split_whitespace().next().unwrap_or("");
    let mut status = "沙箱已关闭".to_string();

    if description.contains("sandbox enabled (auto-allow)") {
        status = "沙箱已开启（自动允许）".to_string();
    } else if description.contains("sandbox enabled") {
        status = "沙箱已开启".to_string();
    }

    if description.contains("fallback allowed") {
        status.push_str("，允许降级执行");
    }
    if description.contains("(managed)") {
        status.push_str("（受策略管理）");
    }

    format!("{} {}（⏎ 配置）", icon, status).trim().to_string()
}

fn localize_terminal_setup_description(description: &str) -> String {
    if description.contains("Option+Enter") {
        "启用 Option+Enter 换行快捷键和视觉铃".to_string()
    } else {
        "安装 Shift+Enter 换行快捷键".to_string()
    }
}

fn get_chinese_builtin_command_description(
    name: &str,
    product: &str,
    assistant: &str,
    current_model: &str,
    sandbox_desc: &str,
    terminal_setup_desc: &str,
) -> Option<String> {
    let s = match name {
        "add-dir" => "添加新的工作目录".to_string(),
        "advisor" => "配置 advisor 模型".to_string(),
        "agents" => "管理智能体配置".to_string(),
        "assistant" => "连接到正在运行的助手会话".to_string(),
        "batch" => "把大型变更拆成多个隔离 worktree，并行派发给子任务".to_string(),
        "branch" => "在当前节点创建当前对话的分支".to_string(),
        "bridge" => "连接此终端以用于远程控制会话".to_string(),
        "btw" => "快速问一个支线问题，不打断主对话".to_string(),
        "chrome" => format!("{} in Chrome（Beta）设置", product),
        "clear" => "清空对话历史并释放上下文".to_string(),
        "color" => "设置本会话提示栏颜色".to_string(),
        "compact" => "清空对话历史，但保留上下文摘要。可选：/compact [摘要说明]".to_string(),
        "config" => "打开配置面板".to_string(),
        "context" => "显示当前上下文使用情况".to_string(),
        "copy" => format!("复制 {} 的最新回复到剪贴板（或用 /copy N 复制倒数第 N 条）", assistant),
        "cost" => "显示当前会话的总成本和耗时".to_string(),
        "debug" => "为当前会话启用调试日志，并协助诊断问题".to_string(),
        "desktop" => "在桌面配套应用中继续当前会话".to_string(),
        "diff" => "查看未提交变更和每轮 diff".to_string(),
        "doctor" => format!("诊断并验证 {} 的安装与设置", product),
        "effort" => "设置模型使用的推理强度".to_string(),
        "env" => "显示当前环境配置".to_string(),
        "exit" => "退出交互界面".to_string(),
        "export" => "将当前对话导出到文件或剪贴板".to_string(),
        "fast" => "切换快速模式".to_string(),
        "feedback" => format!("提交关于 {} 的反馈", assistant),
        "files" => "列出当前上下文中的全部文件".to_string(),
        "help" => "显示帮助和可用命令".to_string(),
        "heapdump" => "将 JS 堆快照导出到 ~/Desktop".to_string(),
        "hooks" => "查看工具事件的 hook 配置".to_string(),
        "ide" => "管理 IDE 集成并显示状态".to_string(),
        "init" => "初始化新的 MOSSEN.md 代码库文档".to_string(),
        "insights" => format!("生成 {} 会话分析报告", product),
        "keybindings" => "打开或创建按键绑定配置文件".to_string(),
        "lang" => "快速切换运行语言".to_string(),
        "login" => "显示 Mossen 后端凭据配置指引".to_string(),
        "logout" => "清理当前后端的本地认证缓存".to_string(),
        "mcp" => "管理 MCP 服务器".to_string(),
        "memory" => format!("编辑 {} 记忆文件", product),
        "mobile" => format!("显示下载 {} 手机应用的二维码", product),
        "model" => format!("设置 {} 使用的 AI 模型（当前 {}）", assistant, current_model),
        "output-style" => "已弃用：使用 /config 更改输出样式".to_string(),
        "permissions" => "管理工具权限的允许和拒绝规则".to_string(),
        "plan" => "开启计划模式或查看当前会话计划".to_string(),
        "plugin" => format!("管理 {} 插件", product),
        "project" => "管理项目存储（清理会话、保留 memory）".to_string(),
        "proactive" => "切换主动自治模式".to_string(),
        "profile" => "设置个人工作流的执行和推理配置".to_string(),
        "privacy-settings" => "查看当前后端的隐私和数据控制".to_string(),
        "extra-usage" => "配置额外用量，以便达到限制后继续工作".to_string(),
        "passes" => format!("与朋友分享一周免费 {} 并获得额外用量", product),
        "rate-limit-options" => "达到速率限制时显示可选处理方式".to_string(),
        "release-notes" => "查看发布说明".to_string(),
        "reload-plugins" => "激活当前会话中待生效的插件变更".to_string(),
        "remote-env" => "配置 teleport 会话使用的默认远程环境".to_string(),
        "rename" => "重命名当前对话".to_string(),
        "resume" => "恢复之前的对话".to_string(),
        "review" => "审查一个拉取请求".to_string(),
        "rewind" => "将代码和/或对话恢复到之前的时间点".to_string(),
        "sandbox" => return Some(localize_sandbox_description(sandbox_desc)),
        "session" => "显示远程会话链接和二维码".to_string(),
        "skills" => "列出可用技能".to_string(),
        "stats" => format!("显示 {} 使用统计和活动", product),
        "status" => format!("显示 {} 状态，包括版本、模型、后端、API 连接和工具状态", assistant),
        "stickers" => format!("订购 {} 贴纸", product),
        "tag" => "为当前会话切换可搜索标签".to_string(),
        "tasks" => "列出并管理后台任务".to_string(),
        "terminal-setup" => return Some(localize_terminal_setup_description(terminal_setup_desc)),
        "theme" => "更换主题".to_string(),
        "thinkback" | "think-back" => format!("{} 2025 年度回顾", product),
        "thinkback-play" => "播放年度回顾动画".to_string(),
        "upgrade" => "打开当前后端的套餐和计费选项".to_string(),
        "usage" => "显示套餐使用限制".to_string(),
        "vim" => "在 Vim 和普通编辑模式之间切换".to_string(),
        "voice" => "切换语音模式".to_string(),
        _ => return None,
    };
    Some(s)
}
