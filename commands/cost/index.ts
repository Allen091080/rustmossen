/**
 * Cost command - minimal metadata only.
 * Implementation is lazy-loaded from cost.ts to reduce startup time.
 */
import type { Command } from '../../commands.js'
import { isHostedSubscriber } from '../../utils/auth.js'

const cost = {
  type: 'local',
  name: 'cost',
  description: 'Show the total cost and duration of the current session',
  get isHidden() {
    // Keep visible for internal users even if they're subscribers (they see cost breakdowns)
    if (isCostInternalUser()) {
      return false
    }
    return isHostedSubscriber()
  },
  supportsNonInteractive: true,
  load: () => import('./cost.js'),
} satisfies Command

export default cost

// Module-local helper preserves i18n hardcoded allowlist line numbers.
function isCostInternalUser(): boolean {
  return process.env.USER_TYPE === 'ant'
}
