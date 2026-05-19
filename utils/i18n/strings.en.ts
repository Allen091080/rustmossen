/**
 * STRINGS_EN — English source-of-truth dictionary for Mossen UI.
 *
 * This module has NO imports. It is the root of the i18n type chain:
 *   strings.en.ts (no import)
 *     └── keys.ts          (import { STRINGS_EN }; export I18nKey = keyof typeof STRINGS_EN)
 *           └── strings.zh.ts (import type { I18nKey }; satisfies Record<I18nKey, string>)
 *                 └── index.ts (combines all + getInteractiveLanguageTag)
 *
 * Naming convention (W1-D5 = A): <scope>.<feature>.<element>
 *   scope     ∈ { cmd, ui, ctx, compact, onboarding, hosted, lang, statusline, spinner }
 *   feature   ∈ command name / component name / region (lowercase, hyphen or camelCase)
 *   element   ∈ { title, description, hint, label, note, count, line, placeholder, ... }
 *
 * Placeholder syntax: '{name}'. Example: 'Welcome to {product}' + { product: 'Mossen' }.
 *
 * Migration policy (UX-Wave1):
 *   - New user-visible text MUST go through `t(key)` with a key registered here.
 *   - Existing inline `getLocalizedText({en, zh})` calls remain compat; do NOT bulk-migrate.
 *   - Slices that touch a file MUST migrate text in that file to `t()`.
 */

export const STRINGS_EN = {
  // --- cmd.* — command registry metadata (description / hint) ---
  // S2A 已迁 9 个 builtin cmd description；后续 slice 会把
  // utils/commandDescription.ts switch 中的剩余 case 逐步迁过来。
  'cmd.help.description': 'Show help and available commands',
  'cmd.exit.description': 'Exit the REPL',
  'cmd.files.description': 'List all files currently in context',
  'cmd.memory.description': 'Edit {product} memory files',
  'cmd.mcp.description': 'Manage MCP servers',
  'cmd.skills.description': 'List available skills',
  'cmd.hooks.description': 'View hook configurations for tool events',
  'cmd.resume.description': 'Resume a previous conversation',
  'cmd.lang.description': 'Quickly switch interface language',
  // W2-S1 高频会话基础 10 命令（9 A + 1 B）
  'cmd.clear.description': 'Clear conversation history and free up context',
  'cmd.compact.description':
    'Clear conversation history but keep a summary in context. Optional: /compact [instructions for summarization]',
  'cmd.diff.description': 'View uncommitted changes and per-turn diffs',
  'cmd.copy.description':
    "Copy {product}'s last response to clipboard (or /copy N for the Nth-latest)",
  'cmd.export.description':
    'Export the current conversation to a file or clipboard',
  'cmd.branch.description':
    'Create a branch of the current conversation at this point',
  'cmd.rename.description': 'Rename the current conversation',
  'cmd.tasks.description': 'List and manage background tasks',
  'cmd.usage.description': 'Show plan usage limits',
  'cmd.rewind.description':
    'Restore the code and/or conversation to a previous point',
  // W2-S2 编辑 / 配置 8 命令（全 A）
  'cmd.config.description': 'Open config panel',
  'cmd.theme.description': 'Change the theme',
  'cmd.color.description': 'Set the prompt bar color for this session',
  'cmd.keybindings.description':
    'Open or create your keybindings configuration file',
  'cmd.vim.description': 'Toggle between Vim and Normal editing modes',
  'cmd.effort.description': 'Set effort level for model usage',
  'cmd.profile.description':
    'Set execution and reasoning profiles for personal workflows',
  'cmd.plan.description':
    'Enable plan mode or view the current session plan',
  // W2-S3 PR / Review / 安全 / 登录 / 顾问 4 命令（3 A + 1 B）
  // /review 暂缓 — smoke_check.py L10802 单一 en 字面量硬比较，待 R-20 解禁
  'cmd.advisor.description': 'Configure the advisor model',
  'cmd.security-review.description':
    'Complete a security review of the pending changes on the current branch',
  'cmd.permissions.description': 'Manage allow & deny tool permission rules',
  'cmd.login.description': 'Show {product} backend credential setup guidance',
  // W2-S4 Plugin / Skill / IDE 5 命令（全 A；/plugin 暂缓）
  // /plugin 暂缓 — smoke_check.py L10854-10855 pluginDescription 单一 en 字面量硬比较，与 /review 同模式，待 R-20 解禁
  'cmd.reload-plugins.description':
    'Activate pending plugin changes in the current session',
  'cmd.agents.description': 'Manage agent configurations',
  'cmd.ide.description': 'Manage IDE integrations and show status',
  'cmd.init-verifiers.description':
    'Create verifier skill(s) for automated verification of code changes',
  'cmd.add-dir.description': 'Add a new working directory',
  // W2-S5 系统/杂项 1 命令（仅 /btw 安全可迁）
  // /context 推迟（multi-variant resolver 阻塞，待 W3）；/brief 归 D（feature KAIROS gate）；
  // /logout 归 D（isCustomBackendEnabled → isUsing3PServices → /logout 个人版不可见）
  'cmd.btw.description':
    'Ask a quick side question without interrupting the main conversation',

  // --- ui.* — generic UI surfaces ---
  // Subsequent slices (S4A) will append goodbye / interrupted / welcome.fallback keys.
  'ui.welcome.title': 'Welcome to {product}',

  // --- ui.taskSummary.* / ui.task.blockedByLabel — TaskListV2 文案 (S3) ---
  // 仅展示层；不影响 task.status 字段值或状态机。
  'ui.taskSummary.tasks': 'tasks',
  'ui.taskSummary.done': 'done',
  'ui.taskSummary.inProgress': 'in progress',
  'ui.taskSummary.open': 'open',
  'ui.taskSummary.pending': 'pending',
  'ui.taskSummary.completed': 'completed',
  'ui.task.blockedByLabel': 'blocked by',

  // --- ui.taskActivity.* — describeTeammateActivity 返回值 (S3 续) ---
  // 仅 UI 展示用途；callers 不做 enum 等值比较 (已 grep 确认)。
  'ui.taskActivity.stopping': 'stopping',
  'ui.taskActivity.awaitingApproval': 'awaiting approval',
  'ui.taskActivity.idle': 'idle',
  'ui.taskActivity.working': 'working',

  // --- lang.* — /lang command + 语言偏好 (S4A) ---
  // lang.switched.message 在 lang.tsx 中通过 t(key, _, langOverride) 强制按
  // 用户选择的目标语言渲染，不依赖 set→get 同步性。
  'lang.cleared.message':
    'Interface language preference cleared. Runtime UI now follows your recent conversation or system language.',
  'lang.current.label': 'Current interface language: {language}',
  'lang.preference.label': 'Preference: {preference}',
  'lang.preference.auto': 'Auto',
  'lang.usage.line': 'Usage: /lang [zh|中文|en|english|auto]',
  'lang.usage.shortcut':
    'Shortcut: /lang toggle switches between Chinese and English UI.',
  'lang.usage.note':
    'Note: /lang changes UI text only. Assistant replies follow the conversation unless you set a response language in /config.',
  'lang.switched.message':
    'Interface language switched to English. Assistant replies still follow the conversation language.',

  // --- ui.exit.* / ui.interrupted.* — Exit + Interrupted 提示 (S4B) ---
  'ui.exit.goodbye1': 'Goodbye!',
  'ui.exit.goodbye2': 'See ya!',
  'ui.exit.goodbye3': 'Bye!',
  'ui.exit.goodbye4': 'Catch you later!',
  'ui.interrupted.label': 'Interrupted ',
  'ui.interrupted.hint': 'What should {product} do instead?',

  // --- ui.compact.* — CompactSummary (S4C) ---
  'ui.compact.summarizedTitle': 'Summarized conversation',
  'ui.compact.summarizedDetailUpTo':
    'Summarized {count} messages up to this point',
  'ui.compact.summarizedDetailFrom':
    'Summarized {count} messages from this point',
  'ui.compact.contextLabel': 'Context: ',
  'ui.compact.summaryTitle': 'Compact summary',
  'ui.compact.expandHistoryHint': 'expand history',
  'ui.compact.expandHint': 'expand',
} as const
