/**
 * /model — 多 profile 列表 + 会话级切换 (S1-09f).
 *
 * 重写自旧 React picker (ModelPicker, sonnet/opus 静态列表). 新实现是 type='local'
 * 的纯文本输出, 走 services/config/profiles facade chain.
 *
 * 旧 src/components/ModelPicker.tsx 不再被本 command 引用; 如需删除留 S2 清理.
 */
import type { LocalCommandCall } from '../../types/command.js'
import {
  desensitizeProfile,
  getCurrentProfile,
  getDefaultProfile,
  getProfileByName,
  listAllProfiles,
  setSessionActiveProfile,
} from '../../services/config/profiles.js'
import { setMainLoopModelOverride } from '../../bootstrap/state.js'
import type { AppState } from '../../state/AppStateStore.js'

function formatList(): string {
  const all = listAllProfiles()
  const current = getCurrentProfile()
  const defaultP = getDefaultProfile()
  const fallbackInList = all.some(item => item.source === 'fallback-env')

  if (all.length === 0) {
    return [
      'No model profiles configured.',
      '',
      'Create one with the CLI (apiKey is required):',
      '  mossen --add-model-profile <name> \\',
      '    --provider openai-compatible \\',
      '    --baseURL <url> \\',
      '    --model <id> \\',
      '    --apiKey <key>',
      '',
      'Then activate it as the global default:',
      '  mossen --set-model-profile <name>',
    ].join('\n')
  }

  const lines: string[] = []
  lines.push(`Model profiles (${all.length}):`)
  lines.push('')
  for (const item of all) {
    const d = desensitizeProfile(item.profile)
    const tags: string[] = []
    if (current && current.name === item.name) tags.push('session')
    if (defaultP && defaultP.name === item.name) tags.push('default')
    if (item.source === 'fallback-env') tags.push('fallback')
    const tagStr = tags.length ? ` [${tags.join(', ')}]` : ''
    const displayName = d.name || item.name
    lines.push(`  ${item.name}${tagStr}`)
    lines.push(`    name:     ${displayName}`)
    lines.push(`    provider: ${d.provider}`)
    lines.push(`    model:    ${d.model}`)
    lines.push(`    baseURL:  ${d.baseURL}`)
    lines.push(`    apiKey:   ${d.apiKey}`)
    lines.push(`    source:   ${item.source === 'fallback-env' ? 'env (MOSSEN_CODE_CUSTOM_*)' : 'settings.json'}`)
    lines.push('')
  }

  if (current) {
    const suffix = current.source === 'fallback-env' ? ' (fallback)' : ''
    lines.push(`Current session profile: ${current.name}${suffix}`)
  } else {
    lines.push('Current session profile: <none>')
  }
  if (defaultP) {
    const suffix = defaultP.source === 'fallback-env' ? ' (fallback)' : ''
    lines.push(`Global default profile:  ${defaultP.name}${suffix}`)
  } else {
    lines.push('Global default profile:  <none>')
  }
  if (current && defaultP && current.name !== defaultP.name) {
    lines.push('')
    lines.push(`Session has been overridden — restart mossen to revert to "${defaultP.name}".`)
  }
  if (fallbackInList) {
    lines.push('')
    lines.push('Tip: this profile comes from legacy env (MOSSEN_CODE_CUSTOM_*).')
    lines.push('     Migrate it to ~/.mossen/settings.json so it lives alongside your other profiles:')
    lines.push('       mossen --migrate-fallback-profile')
  }
  lines.push('')
  lines.push('Usage:')
  lines.push('  /model <profileName>           Switch session profile (this conversation only)')
  lines.push('  mossen --set-model-profile <n> Set global default (persists in ~/.mossen/settings.json)')
  return lines.join('\n')
}

function formatSwitchResult(
  name: string,
  setAppState?: (f: (prev: AppState) => AppState) => void,
): string {
  try {
    const result = setSessionActiveProfile(name)
    // S1-09 闭环修复 (3 层全部要打):
    // 1) setSessionActiveProfile 已写 runtime override (services/config) — customBackend.ts getter 立即看到新 baseURL/apiKey/model
    // 2) setMainLoopModelOverride — bootstrap/state.ts STATE.mainLoopModelOverride, getMainLoopModel() 读它 (server/API 路径)
    // 3) setAppState mainLoopModelForSession — React AppState, useMainLoopModel hook 读它 (statusline/UI 路径)
    // 漏一个: 三处状态不一致 → /model glm 显示切换但请求/UI 仍 qwen.
    // setAppState 在真 REPL context 一定有; harness/SDK headless 可能无, 容错跳过.
    setMainLoopModelOverride(result.profile.model)
    if (typeof setAppState === 'function') {
      setAppState(prev => ({
        ...prev,
        mainLoopModelForSession: result.profile.model,
      }))
    }
    const desensitized = desensitizeProfile(result.profile)
    const defaultP = getDefaultProfile()
    const lines: string[] = []
    const sourceLabel = result.source === 'fallback-env' ? ' (fallback)' : ''
    lines.push(`Switched session profile to "${result.activeProfile}"${sourceLabel}.`)
    lines.push(`  name:     ${desensitized.name || result.activeProfile}`)
    lines.push(`  provider: ${desensitized.provider}`)
    lines.push(`  model:    ${desensitized.model}`)
    lines.push(`  baseURL:  ${desensitized.baseURL}`)
    lines.push(`  apiKey:   ${desensitized.apiKey}`)
    lines.push(`  source:   ${result.source === 'fallback-env' ? 'env (MOSSEN_CODE_CUSTOM_*)' : 'settings.json'}`)
    lines.push('')
    lines.push('Note: this only affects the current session.')
    if (defaultP && defaultP.name !== result.activeProfile) {
      const defaultSuffix = defaultP.source === 'fallback-env' ? ' (fallback)' : ''
      lines.push(`Global default profile remains "${defaultP.name}"${defaultSuffix}. Restart mossen to revert.`)
    } else if (!defaultP) {
      lines.push('No global default profile set. Use `mossen --set-model-profile` to persist.')
    }
    return lines.join('\n')
  } catch (e) {
    const msg = (e as Error).message || String(e)
    const all = listAllProfiles()
    const existing = all.map(item => item.source === 'fallback-env' ? `${item.name} (fallback)` : item.name)
    const lines: string[] = []
    lines.push(`Cannot switch to profile "${name}": ${msg}`)
    lines.push('')
    if (existing.length === 0) {
      lines.push('No profiles configured. Create one with:')
      lines.push('  mossen --add-model-profile <name> --provider openai-compatible \\')
      lines.push('    --baseURL <url> --model <id> --apiKey <key>')
    } else {
      lines.push(`Available profiles: ${existing.join(', ')}`)
      lines.push('')
      lines.push('To list details: /model')
      lines.push('To create new:   mossen --add-model-profile <name> ...')
    }
    return lines.join('\n')
  }
}

export const call: LocalCommandCall = async (args, context) => {
  const trimmed = (args || '').trim()
  if (!trimmed) {
    return { type: 'text', value: formatList() }
  }
  const tokens = trimmed.split(/\s+/)
  const name = tokens[0]!
  const rest = tokens.slice(1)
  if (rest.length > 0) {
    return {
      type: 'text',
      value: [
        `/model: ignoring extra arguments: ${rest.join(' ')}`,
        '',
        formatSwitchResult(name, context.setAppState),
      ].join('\n'),
    }
  }
  // 不存在的 profile 也走 formatSwitchResult, 它内部 catch 会输出可读错误.
  void getProfileByName(name)
  return { type: 'text', value: formatSwitchResult(name, context.setAppState) }
}
