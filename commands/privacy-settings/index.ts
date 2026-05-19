import type { Command } from '../../commands.js'
import { isConsumerSubscriber } from '../../utils/auth.js'
import {
  hasConfiguredHostedPlatformUrls,
  isCustomBackendEnabled,
} from '../../utils/customBackend.js'

const privacySettings = {
  type: 'local-jsx',
  name: 'privacy-settings',
  description: 'View privacy and data controls for the current backend',
  isEnabled: () => {
    return (
      isConsumerSubscriber() ||
      (isCustomBackendEnabled() && hasConfiguredHostedPlatformUrls())
    )
  },
  load: () => import('./privacy-settings.js'),
} satisfies Command

export default privacySettings
