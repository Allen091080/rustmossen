import type { Command } from '../../commands.js'
import { getSubscriptionType } from '../../utils/auth.js'
import {
  hasConfiguredHostedPlatformUrls,
  isCustomBackendEnabled,
} from '../../utils/customBackend.js'
import { isEnvTruthy } from '../../utils/envUtils.js'

const upgrade = {
  type: 'local-jsx',
  name: 'upgrade',
  description: 'Open plan and billing options for the current backend',
  get availability() {
    return isCustomBackendEnabled() ? undefined : ['hosted']
  },
  isEnabled: () =>
    !isEnvTruthy(process.env.DISABLE_UPGRADE_COMMAND) &&
    (!isCustomBackendEnabled() || hasConfiguredHostedPlatformUrls()) &&
    (isCustomBackendEnabled() || getSubscriptionType() !== 'enterprise') &&
    !isCustomBackendEnabled(),
  get isHidden() {
    return isCustomBackendEnabled()
  },
  load: () => import('./upgrade.js'),
} satisfies Command

export default upgrade
