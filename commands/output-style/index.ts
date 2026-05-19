import type { Command } from '../../commands.js'
import { isDeferredSlashCommandEnabled } from '../../utils/deferredSlashCommands.js'

const outputStyle = {
  type: 'local-jsx',
  name: 'output-style',
  description: 'Deprecated: use /config to change output style',
  isEnabled: () => isDeferredSlashCommandEnabled('output-style'),
  isHidden: true,
  load: () => import('./output-style.js'),
} satisfies Command

export default outputStyle
