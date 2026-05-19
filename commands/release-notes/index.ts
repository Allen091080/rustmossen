import type { Command } from '../../commands.js'
import { isDeferredSlashCommandEnabled } from '../../utils/deferredSlashCommands.js'

const releaseNotes: Command = {
  description: 'View release notes',
  name: 'release-notes',
  type: 'local',
  isEnabled: () => isDeferredSlashCommandEnabled('release-notes'),
  supportsNonInteractive: true,
  load: () => import('./release-notes.js'),
}

export default releaseNotes
