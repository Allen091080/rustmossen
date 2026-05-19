// Mossen 点阵 Logo 紧凑版 (按用户反馈缩小)
// 尺寸: 10 行 × ~17 列 (原版 18×33 的约 1/2)
// 颜色: color="mossen" (theme 中 = rgb(103,203,134))
//
// 文件名 Clawd.tsx 保留为 backward-compat (callers 不需改 import).
// 真正的导出是 MossenDotLogo, Clawd 是别名.

import * as React from 'react'
import { Box, Text } from '../../ink.js'

export type ClawdPose = 'default'

export const MOSSEN_TEXT_MARK = '◖◗'

const MOSSEN_DOT_LOGO = [
  '          ••',
  '       •••••',
  '     •••••••',
  '   •••••••••',
  '  ••••• ••••',
  ' ••••• ••••',
  ' •••• •••',
  '  •• •••',
  '    ••',
  '     •',
]

export function MossenDotLogo(): React.ReactElement {
  return (
    <Box flexDirection="column">
      {MOSSEN_DOT_LOGO.map((line, index) => (
        <Text key={index} color="mossen">
          {line}
        </Text>
      ))}
    </Box>
  )
}

// Backward-compat alias
export function Clawd(_props?: { pose?: ClawdPose }): React.ReactElement {
  return <MossenDotLogo />
}
