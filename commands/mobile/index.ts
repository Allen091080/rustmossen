import type { Command } from '../../commands.js'
import {
  hasConfiguredHostedPlatformUrls,
  isCustomBackendEnabled,
} from '../../utils/customBackend.js'

const mobile = {
  type: 'local-jsx',
  name: 'mobile',
  aliases: ['ios', 'android'],
  description: 'Show QR code to download the Mossen mobile app',
  isEnabled: () =>
    !isCustomBackendEnabled() || hasConfiguredHostedPlatformUrls(),
  load: () => import('./mobile.js'),
} satisfies Command

export default mobile
