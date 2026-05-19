import type { Command } from '../../commands.js'
import {
  isVoiceGrowthBookEnabled,
  isVoiceModeEnabled,
} from '../../voice/voiceModeEnabled.js'
import { isDeferredSlashCommandEnabled } from '../../utils/deferredSlashCommands.js'

const voice = {
  type: 'local',
  name: 'voice',
  description: 'Toggle voice mode',
  isEnabled: () =>
    isDeferredSlashCommandEnabled('voice') &&
    isVoiceGrowthBookEnabled() &&
    isVoiceModeEnabled(),
  get isHidden() {
    return !isVoiceModeEnabled()
  },
  supportsNonInteractive: false,
  load: () => import('./voice.js'),
} satisfies Command

export default voice
