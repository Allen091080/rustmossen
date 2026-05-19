/**
 * /model — 多 profile 列表 + 会话级切换 (S1-09f).
 *
 * 旧路径 (sonnet/opus 静态 React picker) 已删: 违反 D-S09-1 schema (要求统一走
 * mossen.profiles facade). 新路径走 type='local' 文本输出, 简单 + 可测.
 *
 * 用法:
 *   /model                — 列出 profiles, 标 session active vs global default
 *   /model <profileName>  — 切换"当前会话" profile (session-only, 不改全局默认)
 *
 * 修改全局默认必须用 CLI: mossen --set-model-profile <name>.
 */
import type { Command } from '../../commands.js'

const model = {
  type: 'local',
  name: 'model',
  description: 'List or switch model profiles (session-only; use mossen --set-model-profile for global default)',
  argumentHint: '[profileName]',
  supportsNonInteractive: false,
  load: () => import('./model.js'),
} satisfies Command

export default model
