import {
  activateProactive,
  deactivateProactive,
  isProactiveActive,
} from '../../proactive/index.js'
import type { LocalCommandModule } from '../../types/command.js'

const command: LocalCommandModule = {
  async call(args) {
    const normalized = args.trim().toLowerCase()

    if (normalized === 'on' || normalized === 'enable') {
      activateProactive('command')
      return { type: 'text', value: 'Proactive mode enabled.' }
    }

    if (normalized === 'off' || normalized === 'disable') {
      deactivateProactive()
      return { type: 'text', value: 'Proactive mode disabled.' }
    }

    if (isProactiveActive()) {
      deactivateProactive()
      return { type: 'text', value: 'Proactive mode disabled.' }
    }

    activateProactive('command')
    return { type: 'text', value: 'Proactive mode enabled.' }
  },
}

export default command
