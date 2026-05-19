import { stat } from 'fs/promises'
import { join } from 'path'
import { logForDebugging } from '../debug.js'
import { getMossenConfigHomeDir } from '../envUtils.js'
import { summarizePluginCache, type PluginCacheSummary } from './cacheUtils.js'
import { loadInstalledPluginsFromDisk } from './installedPluginsManager.js'
import { getMarketplacesCacheDir } from './marketplaceManager.js'

// ---------------------------------------------------------------------------
// W56 read-only /plugin status summary. Pure metadata: no .orphaned_at write,
// no cache modification, no installed registry mutation. Wraps the W55 R1
// orphan classifier (summarizePluginCache) and adds installed-registry plus
// marketplace metadata.
// ---------------------------------------------------------------------------

export type PluginStatusSummary = {
  /** ~/.mossen/plugins/ root. */
  pluginRootPath: string
  /** Whether ~/.mossen/plugins/ exists on disk. */
  pluginRootExists: boolean
  /** Cache summary from cacheUtils.summarizePluginCache (W55 R1 helper). */
  cache: PluginCacheSummary
  /** ~/.mossen/plugins/marketplaces/. */
  marketplacesDir: string
  /** Whether marketplaces dir exists. */
  marketplacesDirExists: boolean
  /** Number of plugin records loaded from installed_plugins.json. */
  installedRecordCount: number
  /** Sum across all plugins of installed-version count. */
  installedVersionCount: number
  /** True iff loading installed_plugins.json succeeded. */
  installedRegistryLoadable: boolean
  /** Path to installed_plugins.json (best-effort; resolved by manager). */
  installedRegistryPath: string
  /** True iff there is at least one cached version that is not in registry. */
  pruneEligible: boolean
  /** Suggested next command for the user. */
  suggestedCommand: string
}

async function dirExists(path: string): Promise<boolean> {
  try {
    const st = await stat(path)
    return st.isDirectory()
  } catch {
    return false
  }
}

/**
 * Read-only summary for /plugin status. Never modifies disk state.
 *
 * Reuses summarizePluginCache (W55 R1 helper) for orphan classification —
 * this avoids drift between the prune surface and the status surface.
 */
export async function describePluginStatus(): Promise<PluginStatusSummary> {
  const configHome = getMossenConfigHomeDir()
  const pluginRootPath = join(configHome, 'plugins')
  const pluginRootExists = await dirExists(pluginRootPath)

  const cache = await summarizePluginCache()
  const marketplacesDir = getMarketplacesCacheDir()
  const marketplacesDirExists = await dirExists(marketplacesDir)

  let installedRecordCount = 0
  let installedVersionCount = 0
  let installedRegistryLoadable = true
  let installedRegistryPath = join(pluginRootPath, 'installed_plugins.json')
  try {
    const data = loadInstalledPluginsFromDisk()
    installedRecordCount = Object.keys(data.plugins).length
    for (const installations of Object.values(data.plugins)) {
      installedVersionCount += installations.length
    }
  } catch (error) {
    logForDebugging(`statusOps: failed to load installed_plugins: ${String(error)}`)
    installedRegistryLoadable = false
  }

  const pruneEligible =
    !cache.zipCacheMode &&
    (cache.expiredOrphanCount > 0 || cache.unmarkedOrphanCount > 0)
  const suggestedCommand = cache.zipCacheMode
    ? '(zip-cache mode active — /plugin prune does not apply)'
    : pruneEligible
      ? '/plugin prune'
      : '(no orphans — /plugin prune would no-op)'

  return {
    pluginRootPath,
    pluginRootExists,
    cache,
    marketplacesDir,
    marketplacesDirExists,
    installedRecordCount,
    installedVersionCount,
    installedRegistryLoadable,
    installedRegistryPath,
    pruneEligible,
    suggestedCommand,
  }
}
