import * as React from 'react'
import { Box } from '../../ink.js'
import { Clawd } from './Clawd.js'

// Mossen 点阵 logo 是静态 (无 pose / 无 jump 动画).
// 文件名 AnimatedClawd 保留为 backward-compat (CondensedLogo 仍 import).
// 容器高度 = MOSSEN_DOT_LOGO 行数 (10, 紧凑版) 防止 layout 抖动.
const LEAF_HEIGHT = 10

export function AnimatedClawd(): React.ReactElement {
  return (
    <Box height={LEAF_HEIGHT} flexDirection="column">
      <Clawd />
    </Box>
  )
}
