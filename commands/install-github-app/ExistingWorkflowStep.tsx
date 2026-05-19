import React from 'react'
import { Select } from 'src/components/CustomSelect/index.js'
import {
  GITHUB_ACTION_SETUP_DOCS_URL,
  getPrimaryGitHubWorkflowPath,
} from '../../constants/github-app.js'
import { Box, Text } from '../../ink.js'

interface ExistingWorkflowStepProps {
  repoName: string
  onSelectAction: (action: 'update' | 'skip' | 'exit') => void
}

export function ExistingWorkflowStep({
  repoName,
  onSelectAction,
}: ExistingWorkflowStepProps): React.ReactNode {
  return (
    <Box flexDirection="column" borderStyle="round" borderDimColor paddingX={1}>
      <Box flexDirection="column" marginBottom={1}>
        <Text bold>Existing Workflow Found</Text>
        <Text dimColor>Repository: {repoName}</Text>
      </Box>
      <Box flexDirection="column" marginBottom={1}>
        <Text>
          A platform workflow file already exists at{' '}
          <Text color="mossen">{getPrimaryGitHubWorkflowPath()}</Text>
        </Text>
        <Text dimColor>What would you like to do?</Text>
      </Box>
      <Box flexDirection="column">
        <Select
          options={[
            {
              label: 'Update workflow file with latest version',
              value: 'update',
            },
            {
              label: 'Skip workflow update (configure secrets only)',
              value: 'skip',
            },
            {
              label: 'Exit without making changes',
              value: 'exit',
            },
          ]}
          onChange={value => onSelectAction(value as 'update' | 'skip' | 'exit')}
          onCancel={() => onSelectAction('exit')}
        />
      </Box>
      <Box marginTop={1}>
        <Text dimColor>
          View the latest platform workflow template at:{' '}
          <Text color="mossen">{GITHUB_ACTION_SETUP_DOCS_URL}</Text>
        </Text>
      </Box>
    </Box>
  )
}
