/**
 * STRINGS_ZH — Chinese (Simplified) translations.
 *
 * Type-only import of I18nKey + `satisfies Record<I18nKey, string>` enforces
 * that every key in STRINGS_EN has a matching zh entry; missing keys produce
 * a TypeScript compile error.
 *
 * Match order with strings.en.ts for diff readability.
 */

import type { I18nKey } from './keys.js'

export const STRINGS_ZH = {
  // --- cmd.* ---
  'cmd.help.description': '显示帮助和可用命令',
  'cmd.exit.description': '退出交互界面',
  'cmd.files.description': '列出当前上下文中的全部文件',
  'cmd.memory.description': '编辑 {product} 记忆文件',
  'cmd.mcp.description': '管理 MCP 服务器',
  'cmd.skills.description': '列出可用技能',
  'cmd.hooks.description': '查看工具事件的 hook 配置',
  'cmd.resume.description': '恢复之前的对话',
  'cmd.lang.description': '快速切换运行语言',
  // W2-S1 高频会话基础 10 命令
  'cmd.clear.description': '清空对话历史并释放上下文',
  'cmd.compact.description':
    '清空对话历史，但保留上下文摘要。可选：/compact [摘要说明]',
  'cmd.diff.description': '查看未提交变更和每轮 diff',
  'cmd.copy.description':
    '复制 {product} 的最新回复到剪贴板（或用 /copy N 复制倒数第 N 条）',
  'cmd.export.description': '将当前对话导出到文件或剪贴板',
  'cmd.branch.description': '在当前节点创建当前对话的分支',
  'cmd.rename.description': '重命名当前对话',
  'cmd.tasks.description': '列出并管理后台任务',
  'cmd.usage.description': '显示套餐使用限制',
  'cmd.rewind.description': '将代码和/或对话恢复到之前的时间点',
  // W2-S2 编辑 / 配置 8 命令
  'cmd.config.description': '打开配置面板',
  'cmd.theme.description': '更换主题',
  'cmd.color.description': '设置本会话提示栏颜色',
  'cmd.keybindings.description': '打开或创建按键绑定配置文件',
  'cmd.vim.description': '在 Vim 和普通编辑模式之间切换',
  'cmd.effort.description': '设置模型使用的推理强度',
  'cmd.profile.description': '设置个人工作流的执行和推理配置',
  'cmd.plan.description': '开启计划模式或查看当前会话计划',
  // W2-S3 PR / Review / 安全 / 登录 / 顾问 4 命令（/review 暂缓）
  'cmd.advisor.description': '配置 advisor 模型',
  'cmd.security-review.description': '对当前分支的待提交变更进行安全审查',
  'cmd.permissions.description': '管理工具权限的允许和拒绝规则',
  'cmd.login.description': '显示 {product} 后端凭据配置指引',
  // W2-S4 Plugin / Skill / IDE 5 命令（/plugin 暂缓）
  'cmd.reload-plugins.description': '激活当前会话中待生效的插件变更',
  'cmd.agents.description': '管理智能体配置',
  'cmd.ide.description': '管理 IDE 集成并显示状态',
  'cmd.init-verifiers.description': '为自动验证代码变更创建验证者技能',
  'cmd.add-dir.description': '添加新的工作目录',
  // W2-S5 系统/杂项 1 命令（/context 推迟、/brief→D、/logout→D）
  'cmd.btw.description': '快速问一个支线问题，不打断主对话',

  // --- ui.* ---
  'ui.welcome.title': '欢迎使用 {product}',

  // --- ui.taskSummary.* / ui.task.blockedByLabel (S3) ---
  'ui.taskSummary.tasks': '个任务',
  'ui.taskSummary.done': '已完成',
  'ui.taskSummary.inProgress': '进行中',
  'ui.taskSummary.open': '待处理',
  'ui.taskSummary.pending': '待处理',
  'ui.taskSummary.completed': '已完成',
  'ui.task.blockedByLabel': '阻塞依赖',

  // --- ui.taskActivity.* (S3 续) ---
  'ui.taskActivity.stopping': '停止中',
  'ui.taskActivity.awaitingApproval': '等待批准',
  'ui.taskActivity.idle': '空闲',
  'ui.taskActivity.working': '工作中',

  // --- lang.* — /lang command + 语言偏好 (S4A) ---
  'lang.cleared.message':
    '已清除界面语言偏好。运行态界面会跟随你最近的对话语言或系统语言。',
  'lang.current.label': '当前界面语言：{language}',
  'lang.preference.label': '当前偏好：{preference}',
  'lang.preference.auto': '自动',
  'lang.usage.line': '用法：/lang [zh|中文|en|english|auto]',
  'lang.usage.shortcut': '快捷用法：/lang toggle 会在中文和英文界面之间切换。',
  'lang.usage.note':
    '说明：/lang 只切换界面文案。模型回复会跟随当前对话，除非你在 /config 里单独设置回复语言。',
  'lang.switched.message': '界面语言已切换为中文。模型回复仍会优先跟随当前对话语言。',

  // --- ui.exit.* / ui.interrupted.* (S4B) ---
  'ui.exit.goodbye1': '再见！',
  'ui.exit.goodbye2': '回头见！',
  'ui.exit.goodbye3': '拜！',
  'ui.exit.goodbye4': '下次见！',
  'ui.interrupted.label': '已中断 ',
  'ui.interrupted.hint': '{product} 应该改做什么？',

  // --- ui.compact.* (S4C) ---
  'ui.compact.summarizedTitle': '对话已压缩',
  'ui.compact.summarizedDetailUpTo': '已压缩到此处之前的 {count} 条消息',
  'ui.compact.summarizedDetailFrom': '已压缩从此处开始的 {count} 条消息',
  'ui.compact.contextLabel': '上下文：',
  'ui.compact.summaryTitle': '压缩摘要',
  'ui.compact.expandHistoryHint': '展开历史',
  'ui.compact.expandHint': '展开',
} as const satisfies Record<I18nKey, string>
