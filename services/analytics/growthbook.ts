/**
 * services/analytics/growthbook.ts — Mossen 个人版兼容 wrapper (G6-1 + G6-2 重写).
 *
 * 历史:
 *   - 此文件原为 GrowthBook SDK 客户端 (~1228 行), 含远程 init / refresh /
 *     experimentDataByFeature / Statsig fallback / disk cache 等远程能力.
 *   - G6-1: 缩成本地 facade wrapper, 不再 import @growthbook/growthbook
 *   - G6-2: 删除所有远程 init / refresh / reset / processRemoteEvalPayload 内部逻辑
 *   - G6-3: bun remove @growthbook/growthbook (待 Allen 显式确认)
 *   - G6-4: 文案/注释清理
 *
 * 公共 API 保留 22 个 export, 行为完全由 Mossen facade (services/config) 决定:
 *   - getFeatureValue_CACHED_MAY_BE_STALE     → resolveMossenConfig
 *   - getDynamicConfig_CACHED_MAY_BE_STALE    → resolveMossenConfig
 *   - getFeatureValue_CACHED_WITH_REFRESH     → resolveMossenConfig (refresh 参数忽略)
 *   - checkStatsigFeatureGate_CACHED_MAY_BE_STALE → resolveMossenConfig + Boolean
 *   - checkGate_CACHED_OR_BLOCKING            → resolveMossenConfig + Boolean
 *   - getDynamicConfig_BLOCKS_ON_INIT         → resolveMossenConfig (async, 立即返回)
 *   - getFeatureValue_DEPRECATED              → resolveMossenConfig (async)
 *   - checkSecurityRestrictionGate            → 永远返回 false (个人版无 security gate)
 *
 * 远程相关 API 全部 no-op, 保留导出仅为 backward-compat:
 *   - initializeGrowthBook                    → no-op (resolves immediately)
 *   - resetGrowthBook                         → no-op
 *   - refreshGrowthBookFeatures               → no-op
 *   - refreshGrowthBookAfterAuthChange        → no-op
 *   - setupPeriodicGrowthBookRefresh          → no-op
 *   - stopPeriodicGrowthBookRefresh           → no-op
 *   - onGrowthBookRefresh                     → 转发到 onMossenConfigRefresh
 *   - hasGrowthBookEnvOverride                → 检查 MOSSEN_CONFIG_OVERRIDES (新)
 *                                                + MOSSEN_INTERNAL_FC_OVERRIDES (旧 deprecated)
 *   - getAllGrowthBookFeatures                → getAllMossenConfigValues
 *   - getGrowthBookConfigOverrides            → 空对象 (legacy ~/.mossen.json 字段冷冻)
 *   - setGrowthBookConfigOverride             → 转发到 setMossenConfigOverride
 *   - clearGrowthBookConfigOverrides          → 转发到 clearMossenConfigOverrides
 *   - getApiBaseUrlHost                       → utils/customBackend (本地)
 */

import {
  onMossenConfigRefresh,
  resolveMossenConfig,
  setMossenConfigOverride,
  clearMossenConfigOverrides,
  getAllMossenConfigValues,
  resolveAliasedKey,
} from '../config/index.js'
import { getHostedPlatformUrls } from '../../utils/customBackend.js'
import {
  type GitHubActionsMetadata,
} from '../../utils/user.js'

// ============================================================================
// 公共类型 (保留外部调用方可能 import 的 type)
// ============================================================================

/** GrowthBook user attributes; 仅类型保留, 个人版无远程上报 */
export type GrowthBookUserAttributes = {
  id: string
  sessionId: string
  deviceID: string
  platform: 'win32' | 'darwin' | 'linux'
  apiBaseUrlHost?: string
  organizationUUID?: string
  accountUUID?: string
  userType?: string
  subscriptionType?: string
  rateLimitTier?: string
  firstTokenTime?: number
  email?: string
  appVersion?: string
  github?: GitHubActionsMetadata
}

// ============================================================================
// 内部 facade-first helper
// ============================================================================

/**
 * 经 Mossen facade 解析 GrowthBook key.
 *
 * G6-1 重写: 不再 fallback 到远程 GrowthBook. 所有 key 走 facade chain
 * (override > env > project > user > default). MOSSEN_BUILTIN_DEFAULTS 已注入
 * 已迁移 key 的代码默认值 (G3/G4/G5); 未迁移 key fallback 到 caller defaultValue.
 */
function resolveViaFacade<T>(growthbookKey: string, defaultValue: T): T {
  const aliased = resolveAliasedKey(growthbookKey)
  const r = resolveMossenConfig<T>(aliased, defaultValue)
  return r.value
}

// ============================================================================
// Public API: feature value / dynamic config / gate
// ============================================================================

export function getFeatureValue_CACHED_MAY_BE_STALE<T>(
  feature: string,
  defaultValue: T,
): T {
  return resolveViaFacade(feature, defaultValue)
}

/**
 * @deprecated refresh interval 参数被忽略 (Mossen 本地配置无定时刷新需要).
 * 使用 getFeatureValue_CACHED_MAY_BE_STALE.
 */
export function getFeatureValue_CACHED_WITH_REFRESH<T>(
  feature: string,
  defaultValue: T,
  _refreshIntervalMs: number,
): T {
  return resolveViaFacade(feature, defaultValue)
}

export function getDynamicConfig_CACHED_MAY_BE_STALE<T>(
  config: string,
  defaultValue: T,
): T {
  return resolveViaFacade(config, defaultValue)
}

/**
 * G6-2 后行为: 立即解析 (无 init 阻塞), 因为本地 facade 是同步的.
 * 保留 async 签名给 backward-compat.
 */
export async function getDynamicConfig_BLOCKS_ON_INIT<T>(
  config: string,
  defaultValue: T,
): Promise<T> {
  return resolveViaFacade(config, defaultValue)
}

/**
 * @deprecated 远程 GrowthBook 已删除; 仅做本地 facade 解析. 保留 async 签名.
 */
export async function getFeatureValue_DEPRECATED<T>(
  feature: string,
  defaultValue: T,
): Promise<T> {
  return resolveViaFacade(feature, defaultValue)
}

export function checkStatsigFeatureGate_CACHED_MAY_BE_STALE(
  gate: string,
): boolean {
  return Boolean(resolveViaFacade<unknown>(gate, false))
}

/**
 * G6-2 后行为: 同 checkStatsigFeatureGate_CACHED_MAY_BE_STALE 但保留 async 签名.
 */
export async function checkGate_CACHED_OR_BLOCKING(
  gate: string,
): Promise<boolean> {
  return Boolean(resolveViaFacade<unknown>(gate, false))
}

/**
 * Security restriction gate. 个人版永远返回 false (无 hosted security 上报路径).
 */
export async function checkSecurityRestrictionGate(
  _gate: string,
): Promise<boolean> {
  return false
}

// ============================================================================
// 远程相关 API: 全部 no-op (G6-2 删除所有 GrowthBook init/refresh 逻辑)
// ============================================================================

/** No-op: GrowthBook 远程 client 已删. 立即 resolve. */
export const initializeGrowthBook = async (): Promise<void> => {
  // G6-2: 不再初始化远程 client; Mossen facade 是 process-内同步.
}

/** No-op. */
export function resetGrowthBook(): void {
  // G6-2: 远程 client 已删, 无 state 可 reset.
}

/** No-op. */
export async function refreshGrowthBookFeatures(): Promise<void> {
  // G6-2: 无远程刷新.
}

/** No-op. */
export function refreshGrowthBookAfterAuthChange(): void {
  // G6-2: 无 auth-driven refresh.
}

/** No-op. */
export function setupPeriodicGrowthBookRefresh(): void {
  // G6-2: 无 periodic refresh timer.
}

/** No-op. */
export function stopPeriodicGrowthBookRefresh(): void {
  // G6-2: 无 timer 可停.
}

/** Refresh listener — 转发到 Mossen facade refresh listener. */
export function onGrowthBookRefresh(
  listener: () => void | Promise<void>,
): () => void {
  return onMossenConfigRefresh(listener)
}

/**
 * 检查指定 feature 是否被 env override.
 * 个人版只看 MOSSEN_CONFIG_OVERRIDES (新) + MOSSEN_INTERNAL_FC_OVERRIDES (旧 deprecated).
 */
export function hasGrowthBookEnvOverride(feature: string): boolean {
  const newRaw = process.env.MOSSEN_CONFIG_OVERRIDES
  const oldRaw = process.env.MOSSEN_INTERNAL_FC_OVERRIDES
  for (const raw of [newRaw, oldRaw]) {
    if (!raw) continue
    try {
      const parsed = JSON.parse(raw) as Record<string, unknown>
      if (parsed && Object.prototype.hasOwnProperty.call(parsed, feature)) {
        return true
      }
    } catch {
      // ignore malformed JSON
    }
  }
  return false
}

/**
 * 返回当前所有已注入 facade 的 key/value (~Mossen builtin defaults + 任意 override).
 */
export function getAllGrowthBookFeatures(): Record<string, unknown> {
  return getAllMossenConfigValues()
}

/**
 * Legacy 兼容: 返回 ~/.mossen.json 上的 GrowthBook config override 字段.
 * G6-2 后此字段不再被写入; 始终返回空对象.
 */
export function getGrowthBookConfigOverrides(): Record<string, unknown> {
  return {}
}

/** 转发到 setMossenConfigOverride('override' scope). */
export function setGrowthBookConfigOverride(key: string, value: unknown): void {
  setMossenConfigOverride(key, value, 'override')
}

/** 转发到 clearMossenConfigOverrides('override' scope). */
export function clearGrowthBookConfigOverrides(): void {
  clearMossenConfigOverrides('override')
}

/**
 * 当前 API base URL host (来自 customBackend, 本地解析).
 * 个人版用 custom backend (e.g. dashscope), 不走 hosted GrowthBook 端点.
 */
export function getApiBaseUrlHost(): string | undefined {
  try {
    const { remoteBaseUrl } = getHostedPlatformUrls()
    if (!remoteBaseUrl) return undefined
    return new URL(remoteBaseUrl).host
  } catch {
    return undefined
  }
}
