import type { Command } from '../../commands.js'
import { shouldInferenceConfigCommandBeImmediate } from '../../utils/immediateCommand.js'

export default {
  type: 'local-jsx',
  name: 'profile',
  description: 'Set execution and reasoning profiles for personal workflows',
  argumentHint: '[profile]',
  get immediate() {
    return shouldInferenceConfigCommandBeImmediate()
  },
  load: () => import('./profile.js'),
} satisfies Command
