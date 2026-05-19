import type { Command } from '../../types/command.js'
import { isDeferredSlashCommandEnabled } from '../../utils/deferredSlashCommands.js'

const assistant: Command = {
  type: 'local-jsx',
  name: 'assistant',
  description: 'Connect to a running assistant session',
  isEnabled: () => isDeferredSlashCommandEnabled('assistant'),
  load: () => import('./assistant.js'),
}

export default assistant
