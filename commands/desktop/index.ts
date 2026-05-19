import type { Command } from '../../commands.js'
import {
  hasConfiguredHostedPlatformUrls,
  isCustomBackendEnabled,
} from '../../utils/customBackend.js'

function isSupportedPlatform(): boolean {
  if (process.platform === 'darwin') {
    return true
  }
  if (process.platform === 'win32' && process.arch === 'x64') {
    return true
  }
  return false
}

const desktop = {
  type: 'local-jsx',
  name: 'desktop',
  aliases: ['app'],
  description: 'Continue the current session in the desktop companion app',
  get availability() {
    return isCustomBackendEnabled() ? undefined : ['hosted']
  },
  isEnabled: () =>
    isSupportedPlatform() &&
    (!isCustomBackendEnabled() || hasConfiguredHostedPlatformUrls()) &&
    !isCustomBackendEnabled(),
  get isHidden() {
    return !isSupportedPlatform() || isCustomBackendEnabled()
  },
  load: () => import('./desktop.js'),
} satisfies Command

export default desktop
