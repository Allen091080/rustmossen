#!/usr/bin/env bun

import { existsSync, readFileSync } from 'fs'
import { resolve } from 'path'
import { getPlatformRuntimeSnapshot } from '../platform/runtime.js'
import { enableConfigs } from '../utils/config.js'

function loadCustomBackendEnv(): void {
  const envPath = resolve('.mossensrc/custom-backend.env')
  if (!existsSync(envPath)) return

  for (const line of readFileSync(envPath, 'utf8').split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    const withoutExport = trimmed.startsWith('export ')
      ? trimmed.slice('export '.length).trim()
      : trimmed
    const match = withoutExport.match(/^([A-Za-z_][A-Za-z0-9_]*)=(.*)$/)
    if (!match) continue
    const [, key, rawValue] = match
    if (process.env[key] !== undefined) continue
    const unquoted = rawValue.replace(/^['"]|['"]$/g, '')
    const defaultExpansion = unquoted.match(
      /^\$\{[A-Za-z_][A-Za-z0-9_]*:-([\s\S]*)\}$/,
    )
    const value = defaultExpansion ? defaultExpansion[1] : unquoted
    process.env[key] = value
  }
}

async function main(): Promise<void> {
  loadCustomBackendEnv()
  process.env.MOSSEN_CODE_ENTRYPOINT ??= 'cli'
  enableConfigs()
  const snapshot = await getPlatformRuntimeSnapshot({ prime: true })
  const checks = [
    {
      name: 'provider',
      ok: snapshot.provider.kind === 'custom-backend',
      detail: snapshot.provider,
    },
    {
      name: 'localGit',
      ok:
        typeof snapshot.localGit.gitInstalled === 'boolean' &&
        typeof snapshot.localGit.ghInstalled === 'boolean' &&
        typeof snapshot.localGit.ghAuthenticated === 'boolean' &&
        typeof snapshot.localGit.localGitReady === 'boolean' &&
        typeof snapshot.localGit.localPrReady === 'boolean',
      detail: snapshot.localGit,
    },
    {
      name: 'directConnect',
      ok:
        typeof snapshot.directConnect.featureEnabled === 'boolean' &&
        typeof snapshot.directConnect.serverRuntimeAvailable === 'boolean' &&
        typeof snapshot.directConnect.openRuntimeAvailable === 'boolean',
      detail: snapshot.directConnect,
    },
    {
      name: 'sshRemote',
      ok:
        typeof snapshot.sshRemote.featureEnabled === 'boolean' &&
        typeof snapshot.sshRemote.localTestAvailable === 'boolean' &&
        typeof snapshot.sshRemote.remoteSessionAvailable === 'boolean',
      detail: snapshot.sshRemote,
    },
    {
      name: 'systemPrompt',
      ok:
        snapshot.systemPrompt.defaultAssembly.length > 0 &&
        snapshot.systemPrompt.effectiveAssembly !== null,
      detail: snapshot.systemPrompt,
    },
    {
      name: 'memory',
      ok: snapshot.memory.promptLoaded,
      detail: snapshot.memory,
    },
    {
      name: 'compression',
      ok: snapshot.compression.available,
      detail: snapshot.compression,
    },
    {
      name: 'skills',
      ok: snapshot.skills.bundledRegistered > 0,
      detail: snapshot.skills,
    },
    {
      name: 'security',
      ok: snapshot.security.availablePermissionModes.length > 0,
      detail: snapshot.security,
    },
    {
      name: 'plugins',
      ok: snapshot.plugins.enabled >= 0 && snapshot.plugins.disabled >= 0,
      detail: snapshot.plugins,
    },
    {
      name: 'mcp',
      ok:
        snapshot.mcp.enterpriseServers >= 0 &&
        snapshot.mcp.userServers >= 0 &&
        snapshot.mcp.projectServers >= 0 &&
        snapshot.mcp.localServers >= 0,
      detail: snapshot.mcp,
    },
    {
      name: 'remote',
      ok:
        typeof snapshot.remote.policyAllowed === 'boolean' &&
        typeof snapshot.remote.bridgeAvailable === 'boolean',
      detail: snapshot.remote,
    },
    {
      name: 'assistant',
      ok:
        typeof snapshot.assistant.featureEnabled === 'boolean' &&
        typeof snapshot.assistant.discoveryAvailable === 'boolean' &&
        typeof snapshot.assistant.attachAvailable === 'boolean',
      detail: snapshot.assistant,
    },
    {
      name: 'chrome',
      ok:
        typeof snapshot.chrome.extensionInstalled === 'boolean' &&
        typeof snapshot.chrome.nativeHostInstalled === 'boolean' &&
        typeof snapshot.chrome.nativeHostManifestCount === 'number',
      detail: snapshot.chrome,
    },
    {
      name: 'voice',
      ok:
        typeof snapshot.voice.streamAvailable === 'boolean' &&
        typeof snapshot.voice.recordingAvailable === 'boolean',
      detail: snapshot.voice,
    },
    {
      name: 'teamMemory',
      ok: typeof snapshot.teamMemory.enabled === 'boolean',
      detail: snapshot.teamMemory,
    },
    {
      name: 'agents',
      ok: snapshot.agents.active >= 0 && snapshot.agents.total >= 0,
      detail: snapshot.agents,
    },
    {
      name: 'sessions',
      ok:
        snapshot.sessions.projectSessions >= 0 &&
        snapshot.sessions.currentTranscriptPath.length > 0,
      detail: snapshot.sessions,
    },
    {
      name: 'swarm',
      ok: typeof snapshot.swarm.teammate === 'boolean',
      detail: snapshot.swarm,
    },
    {
      name: 'featureGates',
      ok:
        typeof snapshot.featureGates.directConnect === 'boolean' &&
        typeof snapshot.featureGates.sshRemote === 'boolean' &&
        typeof snapshot.featureGates.kairos === 'boolean' &&
        typeof snapshot.featureGates.transcriptClassifier === 'boolean' &&
        typeof snapshot.featureGates.chicagoMcp === 'boolean',
      detail: snapshot.featureGates,
    },
  ]

  const failed = checks.filter(check => !check.ok)
  console.log(
    JSON.stringify(
      {
        checks,
        runtime: snapshot,
      },
      null,
      2,
    ),
  )

  if (failed.length > 0) {
    process.exitCode = 1
  }
}

await main()
