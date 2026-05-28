//! Chinese strings — translated from utils/i18n/strings.zh.ts

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Chinese (Simplified) translations for Mossen UI
pub static STRINGS_ZH: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // --- cmd.* ---
    m.insert("cmd.help.description", "显示帮助和可用命令");
    m.insert("cmd.exit.description", "退出交互界面");
    m.insert("cmd.files.description", "列出当前上下文中的全部文件");
    m.insert("cmd.memory.description", "编辑 {product} 记忆文件");
    m.insert("cmd.mcp.description", "管理 MCP 服务器");
    m.insert("cmd.skills.description", "列出可用技能");
    m.insert("cmd.hooks.description", "查看工具事件的 hook 配置");
    m.insert("cmd.resume.description", "恢复之前的对话");
    m.insert("cmd.lang.description", "快速切换运行语言");
    m.insert("cmd.clear.description", "清空对话历史并释放上下文");
    m.insert(
        "cmd.compact.description",
        "清空对话历史，但保留上下文摘要。可选：/compact [摘要说明]",
    );
    m.insert("cmd.diff.description", "查看未提交变更和每轮 diff");
    m.insert(
        "cmd.copy.description",
        "复制 {product} 的最新回复到剪贴板（或用 /copy N 复制倒数第 N 条）",
    );
    m.insert("cmd.export.description", "将当前对话导出到文件或剪贴板");
    m.insert("cmd.branch.description", "在当前节点创建当前对话的分支");
    m.insert("cmd.rename.description", "重命名当前对话");
    m.insert("cmd.tasks.description", "列出并管理后台任务");
    m.insert("cmd.usage.description", "显示套餐使用限制");
    m.insert(
        "cmd.rewind.description",
        "将代码和/或对话恢复到之前的时间点",
    );
    m.insert("cmd.config.description", "打开配置面板");
    m.insert("cmd.theme.description", "更换主题");
    m.insert("cmd.color.description", "设置本会话提示栏颜色");
    m.insert("cmd.keybindings.description", "打开或创建按键绑定配置文件");
    m.insert("cmd.vim.description", "在 Vim 和普通编辑模式之间切换");
    m.insert("cmd.effort.description", "设置模型使用的推理强度");
    m.insert("cmd.profile.description", "设置个人工作流的执行和推理配置");
    m.insert("cmd.plan.description", "开启计划模式或查看当前会话计划");
    m.insert("cmd.advisor.description", "配置 advisor 模型");
    m.insert(
        "cmd.security-review.description",
        "对当前分支的待提交变更进行安全审查",
    );
    m.insert(
        "cmd.permissions.description",
        "管理工具权限的允许和拒绝规则",
    );
    m.insert("cmd.login.description", "显示 {product} 后端凭据配置指引");
    m.insert(
        "cmd.reload-plugins.description",
        "激活当前会话中待生效的插件变更",
    );
    m.insert("cmd.agents.description", "管理智能体配置");
    m.insert("cmd.ide.description", "管理 IDE 集成并显示状态");
    m.insert(
        "cmd.init-verifiers.description",
        "为自动验证代码变更创建验证者技能",
    );
    m.insert("cmd.add-dir.description", "添加新的工作目录");
    m.insert("cmd.btw.description", "快速问一个支线问题，不打断主对话");
    // --- ui.* ---
    m.insert("ui.welcome.title", "欢迎使用 {product}");
    m.insert("ui.taskSummary.tasks", "个任务");
    m.insert("ui.taskSummary.done", "已完成");
    m.insert("ui.taskSummary.inProgress", "进行中");
    m.insert("ui.taskSummary.open", "待处理");
    m.insert("ui.taskSummary.pending", "待处理");
    m.insert("ui.taskSummary.completed", "已完成");
    m.insert("ui.task.blockedByLabel", "阻塞依赖");
    m.insert("ui.taskActivity.stopping", "停止中");
    m.insert("ui.taskActivity.awaitingApproval", "等待批准");
    m.insert("ui.taskActivity.idle", "空闲");
    m.insert("ui.taskActivity.working", "工作中");
    // --- lang.* ---
    m.insert(
        "lang.cleared.message",
        "已清除界面语言偏好。运行态界面会跟随你最近的对话语言或系统语言。",
    );
    m.insert("lang.current.label", "当前界面语言：{language}");
    m.insert("lang.preference.label", "当前偏好：{preference}");
    m.insert("lang.preference.auto", "自动");
    m.insert("lang.usage.line", "用法：/lang [zh|中文|en|english|auto]");
    m.insert(
        "lang.usage.shortcut",
        "快捷用法：/lang toggle 会在中文和英文界面之间切换。",
    );
    m.insert(
        "lang.usage.note",
        "说明：/lang 只切换界面文案。模型回复会跟随当前对话，除非你在 /config 里单独设置回复语言。",
    );
    m.insert(
        "lang.switched.message",
        "界面语言已切换为中文。模型回复仍会优先跟随当前对话语言。",
    );
    // --- ui.exit.* / ui.interrupted.* ---
    m.insert("ui.exit.goodbye1", "再见！");
    m.insert("ui.exit.goodbye2", "回头见！");
    m.insert("ui.exit.goodbye3", "拜！");
    m.insert("ui.exit.goodbye4", "下次见！");
    m.insert("ui.interrupted.label", "已中断 ");
    m.insert("ui.interrupted.hint", "{product} 应该改做什么？");
    // --- ui.compact.* ---
    m.insert("ui.compact.summarizedTitle", "对话已压缩");
    m.insert(
        "ui.compact.summarizedDetailUpTo",
        "已压缩到此处之前的 {count} 条消息",
    );
    m.insert(
        "ui.compact.summarizedDetailFrom",
        "已压缩从此处开始的 {count} 条消息",
    );
    m.insert("ui.compact.contextLabel", "上下文：");
    m.insert("ui.compact.summaryTitle", "压缩摘要");
    m.insert("ui.compact.expandHistoryHint", "展开历史");
    m.insert("ui.compact.expandHint", "展开");
    m
});
