import type { Command } from '../../commands.js'
import { getFeatureValue_CACHED_MAY_BE_STALE } from '../../services/analytics/growthbook.js'
import { isPolicyAllowed } from '../../services/policyLimits/index.js'
import {
  hasConfiguredHostedPlatformUrls,
  isCustomBackendEnabled,
} from '../../utils/customBackend.js'

const web = {
  type: 'local-jsx',
  name: 'web-setup',
  description:
    'Set up hosted remote workspaces and GitHub access',
  get availability() {
    return isCustomBackendEnabled() ? undefined : ['hosted']
  },
  isEnabled: () =>
    getFeatureValue_CACHED_MAY_BE_STALE('tengu_cobalt_lantern', false) &&
    isPolicyAllowed('allow_remote_sessions') &&
    (!isCustomBackendEnabled() || hasConfiguredHostedPlatformUrls()) &&
    !isCustomBackendEnabled(),
  get isHidden() {
    return !isPolicyAllowed('allow_remote_sessions') || isCustomBackendEnabled()
  },
  load: () => import('./remote-setup.js'),
} satisfies Command

export default web
