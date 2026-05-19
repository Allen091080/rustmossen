import figures from 'figures'
import * as React from 'react'
import { useEffect } from 'react'
import { Box, Text } from '../../ink.js'
import {
  filterMcpPromptsByServer,
  filterResourcesByServer,
  filterToolsByServer,
} from '../../services/mcp/utils.js'
import { useAppState } from '../../state/AppState.js'
import { getLocalizedText } from '../../utils/uiLanguage.js'

type McpSnapshot = ReturnType<typeof useMcpSnapshot>

function useMcpSnapshot() {
  return useAppState(state => state.mcp)
}

function statusLabel(type: string): string {
  switch (type) {
    case 'connected':
      return getLocalizedText({ en: 'connected', zh: '已连接' })
    case 'disabled':
      return getLocalizedText({ en: 'disabled', zh: '已禁用' })
    case 'pending':
      return getLocalizedText({ en: 'connecting', zh: '连接中' })
    case 'needs-auth':
      return getLocalizedText({ en: 'needs auth', zh: '需要认证' })
    case 'failed':
      return getLocalizedText({ en: 'failed', zh: '失败' })
    default:
      return type
  }
}

function formatStatus(mcp: McpSnapshot): string {
  const counts = {
    connected: 0,
    disabled: 0,
    pending: 0,
    needsAuth: 0,
    failed: 0,
  }
  for (const client of mcp.clients) {
    switch (client.type) {
      case 'connected':
        counts.connected += 1
        break
      case 'disabled':
        counts.disabled += 1
        break
      case 'pending':
        counts.pending += 1
        break
      case 'needs-auth':
        counts.needsAuth += 1
        break
      case 'failed':
        counts.failed += 1
        break
    }
  }

  const lines: string[] = []
  lines.push(
    getLocalizedText({
      en: `${figures.info} MCP status (read-only)`,
      zh: `${figures.info} MCP 状态（只读）`,
    }),
  )
  lines.push('')
  lines.push(
    getLocalizedText({
      en:
        `Servers: ${mcp.clients.length} total, ${counts.connected} connected, ` +
        `${counts.disabled} disabled, ${counts.pending} connecting, ` +
        `${counts.needsAuth} needs auth, ${counts.failed} failed`,
      zh:
        `服务器: 共 ${mcp.clients.length} 个，${counts.connected} 个已连接，` +
        `${counts.disabled} 个已禁用，${counts.pending} 个连接中，` +
        `${counts.needsAuth} 个需要认证，${counts.failed} 个失败`,
    }),
  )
  lines.push(
    getLocalizedText({
      en: `Capabilities: ${mcp.tools.length} tools, ${mcp.commands.length} prompts/skills, ${Object.values(mcp.resources).flat().length} resources`,
      zh: `能力: ${mcp.tools.length} 个工具，${mcp.commands.length} 个 prompts/skills，${Object.values(mcp.resources).flat().length} 个资源`,
    }),
  )
  lines.push('')

  if (mcp.clients.length === 0) {
    lines.push(
      getLocalizedText({
        en: 'No MCP servers are configured in the current session.',
        zh: '当前会话没有配置 MCP server。',
      }),
    )
  } else {
    for (const client of [...mcp.clients].sort((a, b) =>
      a.name.localeCompare(b.name),
    )) {
      const tools = filterToolsByServer(mcp.tools, client.name)
      const prompts = filterMcpPromptsByServer(mcp.commands, client.name)
      const resources = filterResourcesByServer(
        mcp.resources[client.name] ?? [],
        client.name,
      )
      const scope = client.config.scope
      const transport = client.config.type ?? 'stdio'
      lines.push(`${figures.pointer} ${client.name}`)
      lines.push(
        getLocalizedText({
          en: `  status:    ${statusLabel(client.type)}`,
          zh: `  状态:      ${statusLabel(client.type)}`,
        }),
      )
      lines.push(
        getLocalizedText({
          en: `  scope:     ${scope}  transport: ${transport}`,
          zh: `  作用域:    ${scope}  传输: ${transport}`,
        }),
      )
      lines.push(
        getLocalizedText({
          en: `  exposes:   ${tools.length} tools, ${prompts.length} prompts, ${resources.length} resources`,
          zh: `  暴露能力:  ${tools.length} 个工具，${prompts.length} 个 prompt，${resources.length} 个资源`,
        }),
      )
      if (client.type === 'failed' && client.error) {
        lines.push(`  error:     ${client.error}`)
      }
      if (
        client.type === 'pending' &&
        client.reconnectAttempt &&
        client.maxReconnectAttempts
      ) {
        lines.push(
          getLocalizedText({
            en: `  reconnect: ${client.reconnectAttempt}/${client.maxReconnectAttempts}`,
            zh: `  重连:      ${client.reconnectAttempt}/${client.maxReconnectAttempts}`,
          }),
        )
      }
    }
  }
  lines.push('')
  lines.push(
    getLocalizedText({
      en: 'This command does not reconnect, enable, disable, authenticate, or modify MCP config.',
      zh: '本命令不会 reconnect、启用、禁用、认证或修改 MCP 配置。',
    }),
  )
  return lines.join('\n')
}

export function McpStatus({
  onComplete,
}: {
  onComplete: (result?: string) => void
}): React.ReactNode {
  const mcp = useMcpSnapshot()
  useEffect(() => {
    onComplete(formatStatus(mcp))
  }, [mcp, onComplete])

  return (
    <Box>
      <Text dimColor>
        {getLocalizedText({
          en: 'Reading MCP status…',
          zh: '正在读取 MCP 状态…',
        })}
      </Text>
    </Box>
  )
}
