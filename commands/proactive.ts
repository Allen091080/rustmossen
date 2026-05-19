import type { Command } from '../types/command.js'
import { isDeferredSlashCommandEnabled } from '../utils/deferredSlashCommands.js'

const proactive: Command = {
  type: 'local-jsx',
  name: 'proactive',
  description: 'Toggle proactive autonomous mode',
  isEnabled: () => isDeferredSlashCommandEnabled('proactive'),
  load: () => import('./proactive/proactive.js'),
}

export default proactive
