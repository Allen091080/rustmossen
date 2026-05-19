/**
 * Mossen multi-profile schema + 读取 / 校验 / 脱敏 (S1-09a).
 *
 * Schema 决策 (D-S09-1=A): settings.json 顶层 flat key
 *   "mossen.profiles": { qwen: {...}, minimax: {...}, glm: {...} }
 *   "mossen.activeProfile": "qwen"
 *
 * 读取走 services/config facade (override > env > project > user > default).
 * apiKey 必须脱敏后才能进入任何 stdout/stderr/log/CLI dump.
 */

import * as fs from 'fs'
import * as os from 'os'
import * as path from 'path'

import {
  resolveMossenConfig,
  setMossenConfigOverride,
  clearMossenConfigOverrides,
} from './facade.js'

export const PROFILE_PROVIDER_VALUES = ['openai-compatible'] as const
export type ProfileProvider = (typeof PROFILE_PROVIDER_VALUES)[number]

export type ProfileSchema = {
  provider: ProfileProvider
  baseURL: string
  model: string
  apiKey: string
  /** 可选, 给 statusline / UI 友好显示; 不填用 profile name */
  name?: string
}

export type ProfilesMap = Record<string, ProfileSchema>

/** 用于 CLI dump / 日志: apiKey 已脱敏 (前 6 + ... + 后 4) */
export type DesensitizedProfile = Omit<ProfileSchema, 'apiKey'> & {
  apiKey: string
}

const PROFILES_KEY = 'mossen.profiles'
const ACTIVE_PROFILE_KEY = 'mossen.activeProfile'

/**
 * Fallback profile (env-based) — D-S09-3=P 兼容 .mossensrc/custom-backend.env
 * 当无 active profile 时, customBackend.ts fallthrough 到 MOSSEN_CODE_CUSTOM_* env.
 *
 * 该虚拟 profile 仅用于 UI 显示 (/model + --list-model-profiles), 不写文件.
 * customBackend.ts 实际数据流不变 (env vars 直读), 这里只暴露给 UI 让用户能看见 + 切回.
 */
const FALLBACK_PROFILE_DEFAULT_NAME = 'qwen'
const FALLBACK_PROFILE_SOURCE = 'fallback-env' as const
const SETTINGS_PROFILE_SOURCE = 'settings' as const

export type ProfileSource = typeof FALLBACK_PROFILE_SOURCE | typeof SETTINGS_PROFILE_SOURCE

export type ListedProfile = {
  name: string
  profile: ProfileSchema
  source: ProfileSource
}

/** apiKey 脱敏: 前 6 + ... + 后 4. 短 key 全 mask. */
export function maskApiKey(apiKey: string | undefined | null): string {
  if (!apiKey || typeof apiKey !== 'string') return ''
  const trimmed = apiKey.trim()
  if (trimmed.length === 0) return ''
  if (trimmed.length <= 12) return '***'
  return `${trimmed.slice(0, 6)}...${trimmed.slice(-4)}`
}

export function desensitizeProfile(profile: ProfileSchema): DesensitizedProfile {
  return { ...profile, apiKey: maskApiKey(profile.apiKey) }
}

export function desensitizeProfiles(profiles: ProfilesMap): Record<string, DesensitizedProfile> {
  const out: Record<string, DesensitizedProfile> = {}
  for (const [name, p] of Object.entries(profiles)) {
    out[name] = desensitizeProfile(p)
  }
  return out
}

/**
 * 校验单个 profile schema. 返回 ok=true 或带原因的失败.
 * 必填: provider, baseURL, model, apiKey 至少一个非空; name 可选.
 */
export function validateProfile(value: unknown): { ok: true; profile: ProfileSchema } | { ok: false; reason: string } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return { ok: false, reason: 'profile must be an object' }
  }
  const v = value as Record<string, unknown>
  const provider = typeof v.provider === 'string' ? v.provider : ''
  if (!(PROFILE_PROVIDER_VALUES as readonly string[]).includes(provider)) {
    return {
      ok: false,
      reason: `provider must be one of ${PROFILE_PROVIDER_VALUES.join('|')}, got "${provider}"`,
    }
  }
  const baseURL = typeof v.baseURL === 'string' ? v.baseURL.trim() : ''
  if (!baseURL) return { ok: false, reason: 'baseURL required (non-empty string)' }
  const model = typeof v.model === 'string' ? v.model.trim() : ''
  if (!model) return { ok: false, reason: 'model required (non-empty string)' }
  const apiKey = typeof v.apiKey === 'string' ? v.apiKey.trim() : ''
  if (!apiKey) return { ok: false, reason: 'apiKey required (non-empty string)' }
  const name = typeof v.name === 'string' && v.name.trim() ? v.name.trim() : undefined
  return {
    ok: true,
    profile: {
      provider: provider as ProfileProvider,
      baseURL,
      model,
      apiKey,
      ...(name ? { name } : {}),
    },
  }
}

/**
 * 读 facade 获取 mossen.profiles, 过滤掉非法 entry.
 * 任何非 object / 缺字段 / provider 不识别的 entry 被静默 skip (不抛错, 因 facade 读链路必须容错).
 */
export function getProfiles(): ProfilesMap {
  const raw = resolveMossenConfig<unknown>(PROFILES_KEY, null).value
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return {}
  }
  const out: ProfilesMap = {}
  for (const [name, entry] of Object.entries(raw as Record<string, unknown>)) {
    const validated = validateProfile(entry)
    if (validated.ok) {
      out[name] = validated.profile
    }
  }
  return out
}

/**
 * 取 active profile name. 返回 settings 里的 mossen.activeProfile (若存在且对应的 profile 真存在), 否则 null.
 * 不会自己挑默认; 如果用户 active=qwen 但 profiles 里没 qwen, 返回 null (让上层决定 fallback).
 */
export function getActiveProfileName(): string | null {
  const raw = resolveMossenConfig<unknown>(ACTIVE_PROFILE_KEY, null).value
  if (typeof raw !== 'string' || !raw.trim()) return null
  const name = raw.trim()
  const profiles = getProfiles()
  return Object.prototype.hasOwnProperty.call(profiles, name) ? name : null
}

/**
 * 取 active profile 完整 schema. 若 activeProfile 字段不存在 / 指向不存在的 profile, 返回 null.
 * 调用方负责在 null 时 fallback 到旧 env 路径 (S1-09b 在 customBackend.ts 实现).
 */
export function getActiveProfile(): ProfileSchema | null {
  const name = getActiveProfileName()
  if (!name) return null
  const profiles = getProfiles()
  return profiles[name] ?? null
}

export function getProfileByName(name: string): ProfileSchema | null {
  return getProfiles()[name] ?? null
}

/**
 * 从旧 env (MOSSEN_CODE_CUSTOM_*) 合成虚拟 fallback profile (D-S09-3=P).
 *
 * 触发条件: baseURL + apiKey 都存在 (二者缺一不视为可用 fallback).
 * 名字: 优先 MOSSEN_CODE_CUSTOM_NAME (须通过 validateProfileName); 否则 'qwen'.
 * provider: 强制 'openai-compatible' (与 ProfileSchema enum 对齐).
 *
 * 注意: 该 profile 仅用于 UI; customBackend.ts 不读它, 仍直接读 env vars.
 *      真切走 fallback 时, getActiveProfile 必须返回 null 才能让 customBackend 落到 env.
 */
export function getFallbackProfile(): ListedProfile | null {
  const baseURL = process.env.MOSSEN_CODE_CUSTOM_BASE_URL?.trim()
  const apiKey = process.env.MOSSEN_CODE_CUSTOM_API_KEY?.trim()
  if (!baseURL || !apiKey) return null
  const model = process.env.MOSSEN_CODE_CUSTOM_MODEL?.trim() || 'unknown'
  const rawName = process.env.MOSSEN_CODE_CUSTOM_NAME?.trim() || ''
  const nameResult = rawName ? validateProfileName(rawName) : { ok: false as const, reason: '' }
  const name = nameResult.ok ? nameResult.name : FALLBACK_PROFILE_DEFAULT_NAME
  const profile: ProfileSchema = {
    provider: 'openai-compatible',
    baseURL: baseURL.replace(/\/+$/, ''),
    model,
    apiKey,
    ...(rawName && nameResult.ok ? { name: rawName } : {}),
  }
  return { name, profile, source: FALLBACK_PROFILE_SOURCE }
}

/**
 * 列出所有"应展示"的 profile. 给 /model + --list-model-profiles allProfiles 字段用.
 *
 * S1-09 收口政策 (Allen 拍板): fallback 仅在 settings 完全空时作为兜底进入列表.
 * 一旦 settings 有任何 profile, 旧 env fallback 不进列表 (避免 fallback 成为主路径).
 * 用户可通过 `mossen --migrate-fallback-profile` 把 fallback 升级为正式 profile.
 *
 * 注意: fallbackProfile 字段 (CLI JSON) 仍始终反映 env 真实存在性, 供 UI 检测迁移机会.
 */
export function listAllProfiles(): ListedProfile[] {
  const settings = getProfiles()
  const settingsList: ListedProfile[] = Object.keys(settings)
    .sort()
    .map(name => ({ name, profile: settings[name]!, source: SETTINGS_PROFILE_SOURCE }))
  if (settingsList.length > 0) return settingsList
  const fallback = getFallbackProfile()
  return fallback ? [fallback] : []
}

/**
 * "当前会话实际在用的 profile". 解析顺序:
 *   1. session active (runtime override 设的) → 真 profile (settings 命中)
 *   2. user-scope active → 真 profile
 *   3. fallback profile (env 存在)
 *   4. null (无任何配置)
 *
 * 给 /model 列表 / --list-model-profiles / statusline 用, 替代 raw activeProfile null.
 */
export function getCurrentProfile(): ListedProfile | null {
  const sessionName = getActiveProfileName()
  if (sessionName) {
    const p = getProfiles()[sessionName]
    if (p) return { name: sessionName, profile: p, source: SETTINGS_PROFILE_SOURCE }
  }
  return getFallbackProfile()
}

/**
 * "全局默认 profile". 直读 user scope settings.json 拿 activeProfile (跳过 runtimeOverride),
 * 若该 name 命中 settings → 返回真 profile; 否则若 fallback 存在 → 返回 fallback.
 *
 * 给 /model 列表的 [default] tag + --list-model-profiles defaultProfile 字段用.
 */
export function getDefaultProfile(): ListedProfile | null {
  const defaultName = getDefaultActiveProfileName()
  if (defaultName) {
    const p = getProfiles()[defaultName]
    if (p) return { name: defaultName, profile: p, source: SETTINGS_PROFILE_SOURCE }
  }
  return getFallbackProfile()
}

const PROFILE_NAME_PATTERN = /^[a-zA-Z][a-zA-Z0-9_-]{0,31}$/

/**
 * 校验 profile name (CLI / UI 写入前必查).
 * 规则: 字母开头, 字母/数字/_/- , 长度 1-32. 防止 stash 控制字符 / 路径符.
 */
export function validateProfileName(name: unknown): { ok: true; name: string } | { ok: false; reason: string } {
  if (typeof name !== 'string') return { ok: false, reason: 'profile name must be a string' }
  const trimmed = name.trim()
  if (!trimmed) return { ok: false, reason: 'profile name must be non-empty' }
  if (!PROFILE_NAME_PATTERN.test(trimmed)) {
    return {
      ok: false,
      reason: `profile name "${trimmed}" must match ${PROFILE_NAME_PATTERN.source} (start with letter, only letters/digits/_/-, 1-32 chars)`,
    }
  }
  return { ok: true, name: trimmed }
}

/**
 * 写入 / 覆盖 profile (CLI 和 UI 都用; 同时支持 create + update).
 * scope 默认 'user' (写 ~/.mossen/settings.json); 'project' 写 <cwd>/.mossen/settings.json.
 *
 * 失败 (校验 fail): 抛 Error, 调用方负责 catch.
 * 成功: 返回最新完整 profiles map.
 */
export function setProfile(
  name: string,
  schema: unknown,
  scope: 'user' | 'project' = 'user',
): ProfilesMap {
  const nameResult = validateProfileName(name)
  if (nameResult.ok !== true) throw new Error(nameResult.reason)
  const profileResult = validateProfile(schema)
  if (profileResult.ok !== true) throw new Error(profileResult.reason)

  const current = getProfiles()
  const next: ProfilesMap = { ...current, [nameResult.name]: profileResult.profile }
  setMossenConfigOverride(PROFILES_KEY, next, scope)
  return next
}

/**
 * 删除 profile. 若指向的 profile 不存在, 返回 deleted=false (no-op, 不抛错).
 * 若被删的 profile 是当前 activeProfile, 同时清掉 activeProfile (避免悬空指向).
 */
export function deleteProfile(
  name: string,
  scope: 'user' | 'project' = 'user',
): { deleted: boolean; activeProfileCleared: boolean; profiles: ProfilesMap } {
  const current = getProfiles()
  if (!Object.prototype.hasOwnProperty.call(current, name)) {
    return { deleted: false, activeProfileCleared: false, profiles: current }
  }
  const next: ProfilesMap = { ...current }
  delete next[name]
  setMossenConfigOverride(PROFILES_KEY, next, scope)

  let activeCleared = false
  if (getActiveProfileName() === null && resolveMossenConfig<unknown>(ACTIVE_PROFILE_KEY, null).value === name) {
    // getActiveProfileName 已返回 null 因为 profile 不在 map 里; 但底层 settings 还有字面 entry, 清干净
    setMossenConfigOverride(ACTIVE_PROFILE_KEY, null, scope)
    activeCleared = true
  } else {
    const rawActive = resolveMossenConfig<unknown>(ACTIVE_PROFILE_KEY, null).value
    if (rawActive === name) {
      setMossenConfigOverride(ACTIVE_PROFILE_KEY, null, scope)
      activeCleared = true
    }
  }
  return { deleted: true, activeProfileCleared: activeCleared, profiles: next }
}

/**
 * 切换 activeProfile (CLI / UI 共用). name 必须对应已存在的 profile 或 fallback.
 * scope 默认 'user'.
 *
 * S1-09 闭环: 若 name 是 fallback profile 名 (env-based, 非 settings 持久化),
 * 则 CLEAR scope 内的 activeProfile (设 null), 让 customBackend.ts fallthrough 到 env.
 * 这样用户从 glm/minimax 切回 qwen (fallback) 时, 全局默认 = "no profile" = fallback.
 */
export function setActiveProfile(
  name: string,
  scope: 'user' | 'project' = 'user',
): { activeProfile: string; profile: ProfileSchema; source: ProfileSource } {
  const nameResult = validateProfileName(name)
  if (nameResult.ok !== true) throw new Error(nameResult.reason)
  const real = getProfileByName(nameResult.name)
  if (real) {
    setMossenConfigOverride(ACTIVE_PROFILE_KEY, nameResult.name, scope)
    return { activeProfile: nameResult.name, profile: real, source: SETTINGS_PROFILE_SOURCE }
  }
  const fallback = getFallbackProfile()
  if (fallback && fallback.name === nameResult.name) {
    setMossenConfigOverride(ACTIVE_PROFILE_KEY, null, scope)
    return { activeProfile: nameResult.name, profile: fallback.profile, source: FALLBACK_PROFILE_SOURCE }
  }
  const settingsNames = Object.keys(getProfiles())
  const existing = fallback && !settingsNames.includes(fallback.name)
    ? [...settingsNames, fallback.name]
    : settingsNames
  throw new Error(
    `cannot activate profile "${nameResult.name}": not found in mossen.profiles (existing: ${existing.join(', ') || '<none>'})`,
  )
}

/**
 * 清掉 activeProfile (CLI --clear-active-profile / UI 重置 用).
 * 不删 profile 本身, 仅清 activeProfile 字段; 之后调用 getActiveProfile 返回 null.
 */
export function clearActiveProfile(scope: 'user' | 'project' = 'user'): void {
  setMossenConfigOverride(ACTIVE_PROFILE_KEY, null, scope)
}

/**
 * 会话级 active profile 切换 (S1-09f, /model <name> 走这里).
 * 用 facade 'override' scope (process-内 RuntimeOverrideProvider, priority 0),
 * 不写文件. 重启 mossen 后 override 失效, 仍用 user scope 的全局默认.
 *
 * S1-09 闭环: 若 name 是 fallback profile 名, runtime override 设 null (mask user-scope active),
 * 让 customBackend.ts fallthrough 到 env. 用户可以从 glm/minimax 切回 qwen (fallback).
 */
export function setSessionActiveProfile(name: string): { activeProfile: string; profile: ProfileSchema; source: ProfileSource } {
  const nameResult = validateProfileName(name)
  if (nameResult.ok !== true) throw new Error(nameResult.reason)
  const real = getProfileByName(nameResult.name)
  if (real) {
    setMossenConfigOverride(ACTIVE_PROFILE_KEY, nameResult.name, 'override')
    return { activeProfile: nameResult.name, profile: real, source: SETTINGS_PROFILE_SOURCE }
  }
  const fallback = getFallbackProfile()
  if (fallback && fallback.name === nameResult.name) {
    setMossenConfigOverride(ACTIVE_PROFILE_KEY, null, 'override')
    return { activeProfile: nameResult.name, profile: fallback.profile, source: FALLBACK_PROFILE_SOURCE }
  }
  const settingsNames = Object.keys(getProfiles())
  const existing = fallback && !settingsNames.includes(fallback.name)
    ? [...settingsNames, fallback.name]
    : settingsNames
  throw new Error(
    `cannot activate profile "${nameResult.name}": not found in mossen.profiles (existing: ${existing.join(', ') || '<none>'})`,
  )
}

/**
 * 清除 session-only override (回归到 user scope 的全局默认).
 */
export function clearSessionActiveProfile(): void {
  clearMossenConfigOverrides('override', ACTIVE_PROFILE_KEY)
}

/**
 * 直接读 user scope settings.json 拿全局默认 activeProfile (跳过 runtimeOverride).
 * /model 无参列表展示用, 区分 "session 当前" vs "global default".
 */
export function getDefaultActiveProfileName(): string | null {
  const configDir = process.env.MOSSEN_CONFIG_DIR ?? path.join(os.homedir(), '.mossen')
  const settingsPath = path.join(configDir, 'settings.json')
  if (!fs.existsSync(settingsPath)) return null
  try {
    const raw = fs.readFileSync(settingsPath, 'utf-8')
    const parsed = JSON.parse(raw) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
    const v = (parsed as Record<string, unknown>)[ACTIVE_PROFILE_KEY]
    if (typeof v !== 'string') return null
    const trimmed = v.trim()
    return trimmed || null
  } catch {
    return null
  }
}

export type MigrateFallbackResult =
  | {
      ok: true
      migrated: true
      profileName: string
      activeProfileSet: boolean
      scope: 'user' | 'project'
    }
  | {
      ok: true
      migrated: false
      reason: 'no-fallback' | 'already-exists'
      profileName?: string
      scope: 'user' | 'project'
    }
  | {
      ok: false
      reason: string
      scope: 'user' | 'project'
    }

/**
 * 一次性迁移 — 把 env fallback (MOSSEN_CODE_CUSTOM_*) 升级为正式 settings profile.
 *
 * 行为:
 *   1. 读 env fallback (getFallbackProfile); 不存在 → ok=true migrated=false reason='no-fallback'.
 *   2. 默认 targetName = fallback.name (常 'qwen'); 若 settings 已有同名 profile + force=false →
 *      ok=true migrated=false reason='already-exists'. force=true 覆盖.
 *   3. 写入 settings (走 setProfile / facade chain, scope 默认 'user' = ~/.mossen/settings.json).
 *   4. activate 决定是否同时设 mossen.activeProfile:
 *      - 'auto'  (默认): 当前 active 为 null 或就是 targetName → 设. 已显式指向其它真 profile → 不动.
 *      - 'always': 强制设
 *      - 'never':  不动 active
 *
 * 不删 .mossensrc/custom-backend.env, 不动 env vars; 旧启动方式继续可用.
 * 写入受 LocalSettingsProvider 强制 chmod 0600 (Stage1 hotfix R10).
 */
export function migrateFallbackProfile(opts?: {
  scope?: 'user' | 'project'
  targetName?: string
  force?: boolean
  activate?: 'auto' | 'always' | 'never'
}): MigrateFallbackResult {
  const scope = opts?.scope ?? 'user'
  const force = opts?.force ?? false
  const activate = opts?.activate ?? 'auto'

  const fallback = getFallbackProfile()
  if (!fallback) {
    return { ok: true, migrated: false, reason: 'no-fallback', scope }
  }

  const targetNameRaw = opts?.targetName?.trim() || fallback.name
  const nameResult = validateProfileName(targetNameRaw)
  if (nameResult.ok !== true) {
    return { ok: false, reason: nameResult.reason, scope }
  }

  const existing = getProfileByName(nameResult.name)
  if (existing && !force) {
    return {
      ok: true,
      migrated: false,
      reason: 'already-exists',
      profileName: nameResult.name,
      scope,
    }
  }

  const profileFinal: ProfileSchema = {
    provider: fallback.profile.provider,
    baseURL: fallback.profile.baseURL,
    model: fallback.profile.model,
    apiKey: fallback.profile.apiKey,
    ...(fallback.profile.name && nameResult.name === fallback.name
      ? { name: fallback.profile.name }
      : {}),
  }

  setProfile(nameResult.name, profileFinal, scope)

  let activeSet = false
  if (activate === 'always') {
    setMossenConfigOverride(ACTIVE_PROFILE_KEY, nameResult.name, scope)
    activeSet = true
  } else if (activate === 'auto') {
    const currentActive = getActiveProfileName()
    if (currentActive === null || currentActive === nameResult.name) {
      setMossenConfigOverride(ACTIVE_PROFILE_KEY, nameResult.name, scope)
      activeSet = true
    }
  }

  return {
    ok: true,
    migrated: true,
    profileName: nameResult.name,
    activeProfileSet: activeSet,
    scope,
  }
}

export type ProfileTestResult = {
  ok: boolean
  /** HTTP status (任何值, 包括 4xx/5xx; ok=false 时可能为 0 = 连接级失败) */
  status: number
  /** 测试用的最终 URL (baseURL + /models 后缀) */
  url: string
  /** 真实测试耗时 (ms) */
  durationMs: number
  /** 失败时填; 成功时为 undefined */
  error?: string
}

/**
 * 测试 profile 连通性 (Workbench UI "测试连接"按钮 + CLI --test-model-profile).
 * 真发 GET 到 baseURL + /models, 验:
 *   - 网络可达 (任何 HTTP status 都算 ok=true 视为 server reachable)
 *   - 携带 Authorization: Bearer <apiKey>
 *
 * 不验 OpenAI 协议正确性 (因 server 可能不实现 /models, 或 schema 不同),
 * 只验"能连上 + 真透 apiKey"; 真链路 chat completion 留给 mossen -p 跑.
 *
 * 超时默认 5000ms; 网络异常 / abort → ok=false + status=0 + error 字段.
 */
export async function testProfile(
  profile: ProfileSchema,
  options?: { timeoutMs?: number },
): Promise<ProfileTestResult> {
  const timeoutMs = options?.timeoutMs ?? 5000
  const baseTrimmed = profile.baseURL.replace(/\/+$/, '')
  const url = `${baseTrimmed}/models`
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  const start = Date.now()
  try {
    const res = await fetch(url, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${profile.apiKey}`,
        'User-Agent': 'mossen-profile-test/1.0',
      },
      signal: controller.signal,
    })
    return {
      ok: true,
      status: res.status,
      url,
      durationMs: Date.now() - start,
    }
  } catch (e) {
    return {
      ok: false,
      status: 0,
      url,
      durationMs: Date.now() - start,
      error: (e as Error).message,
    }
  } finally {
    clearTimeout(timer)
  }
}
