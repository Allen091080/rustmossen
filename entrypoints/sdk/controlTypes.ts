// Minimal SDK control type stubs for reconstructed source builds.
export type SDKControlRequest = Record<string, unknown>
export type SDKControlResponse = Record<string, unknown>

import type { z } from 'zod/v4'
import type { SDKControlRequestInnerSchema } from './controlSchemas.js'

export type SDKControlRequestInner = z.infer<
  ReturnType<typeof SDKControlRequestInnerSchema>
>
