import type { Command } from '../../commands.js'
import { isDeferredSlashCommandEnabled } from '../../utils/deferredSlashCommands.js'

const heapDump = {
  type: 'local',
  name: 'heapdump',
  description: 'Dump the JS heap to ~/Desktop',
  isEnabled: () => isDeferredSlashCommandEnabled('heapdump'),
  isHidden: true,
  supportsNonInteractive: true,
  load: () => import('./heapdump.js'),
} satisfies Command

export default heapDump
