import { existsSync } from 'fs'
import { mkdir, readFile, rm, writeFile } from 'fs/promises'
import { join } from 'path'
import { enableConfigs } from '../utils/config.js'
import { getMemoryFiles } from '../utils/mossenmd.js'
import { loadMemoryPrompt } from '../memdir/memdir.js'
import {
  getTeamMemEntrypoint,
  getTeamMemPath,
  isTeamMemoryEnabled,
} from '../memdir/teamMemPaths.js'

async function readIfExists(path: string): Promise<string | null> {
  if (!existsSync(path)) return null
  return await readFile(path, 'utf-8')
}

async function restoreFile(path: string, previous: string | null): Promise<void> {
  if (previous === null) {
    await rm(path, { force: true })
  } else {
    await writeFile(path, previous, 'utf-8')
  }
}

async function main(): Promise<void> {
  enableConfigs()

  const teamDir = getTeamMemPath()
  const entry = getTeamMemEntrypoint()
  const note = join(teamDir, 'probe.md')
  const marker = `teammem-probe-marker-${Date.now()}`

  const previousEntry = await readIfExists(entry)
  const previousNote = await readIfExists(note)

  try {
    await mkdir(teamDir, { recursive: true })
    await writeFile(
      note,
      [
        '---',
        'name: Probe',
        'description: Team memory probe',
        'type: project',
        '---',
        marker,
        '',
      ].join('\n'),
      'utf-8',
    )
    await writeFile(entry, `- [Probe](probe.md) - ${marker}\n`, 'utf-8')

    const prompt = await loadMemoryPrompt()
    const files = await getMemoryFiles()
    const teamEntry = files.find(file => file.type === 'TeamMem')

    const result = {
      status: 'ok',
      teamEnabled: isTeamMemoryEnabled(),
      promptHasTeamSection: prompt?.includes('shared team directory') ?? false,
      promptHasTeamDir: prompt?.includes(teamDir) ?? false,
      teamEntryFound: Boolean(teamEntry),
      teamEntryHasMarker: teamEntry?.content.includes(marker) ?? false,
      teamDir,
      entry,
    }

    if (!result.teamEnabled) {
      console.log(
        JSON.stringify(
          {
            ...result,
            status: 'gated',
            classification: 'team-memory-build-flag-disabled',
          },
          null,
          2,
        ),
      )
      return
    }

    if (
      !result.promptHasTeamSection ||
      !result.promptHasTeamDir ||
      !result.teamEntryFound ||
      !result.teamEntryHasMarker
    ) {
      throw new Error(JSON.stringify(result, null, 2))
    }

    console.log(JSON.stringify(result, null, 2))
  } finally {
    await restoreFile(entry, previousEntry)
    await restoreFile(note, previousNote)
  }
}

await main()
