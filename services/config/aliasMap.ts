/**
 * GrowthBook → Mossen key alias map (G-D3 决策的实现).
 *
 * G3-G5 阶段每迁移一个 GrowthBook key 到 Mossen 命名, 同步往这里加 entry.
 * Wrapper (services/analytics/growthbook.ts) 收到 tengu_* 调用时,
 * 先查这个表; 命中 → 转成 mossen.* 后走 facade; 未命中 → fallback 到旧路径.
 */

import type { GrowthBookAliasMap } from './types.js'

export const GROWTHBOOK_TO_MOSSEN_ALIAS: GrowthBookAliasMap = {
  // G3-1: 1P 事件 batch 配置
  'tengu_1p_event_batch_config': 'mossen.analytics.eventBatchConfig',
  // G3-2: per-event sampling 配置
  'tengu_event_sampling_config': 'mossen.analytics.eventSamplingConfig',
  // G3-3: per-sink killswitch (mangled 'frond_boric')
  'tengu_frond_boric': 'mossen.analytics.sinkKillswitch',
  // G4-1: compact 域 7 个 key
  'tengu_slate_heron': 'mossen.compact.timeBasedMCConfig',
  'tengu_compact_cache_prefix': 'mossen.compact.cachePrefixSharing',
  'tengu_compact_streaming_retry': 'mossen.compact.streamingRetryEnabled',
  'tengu_sm_compact_config': 'mossen.compact.sessionMemoryConfig',
  'tengu_session_memory': 'mossen.compact.sessionMemoryEnabled',
  'tengu_sm_compact': 'mossen.compact.sessionMemoryCompactEnabled',
  'tengu_cobalt_raccoon': 'mossen.compact.reactiveAutoCompactKillswitch',
  // G4-2: memory 域 6 个 gate (memdir/**)
  'tengu_coral_fern': 'mossen.memory.coralFernEnabled',
  'tengu_moth_copse': 'mossen.memory.skipDailyLogIndex',
  'tengu_herring_clock': 'mossen.memory.kairosActive',
  'tengu_team_memory': 'mossen.memory.teamMemoryEnabled',
  'tengu_passport_quail': 'mossen.memory.passportQuailEnabled',
  'tengu_slate_thimble': 'mossen.memory.slateThimbleEnabled',
  // G4-3: tool 域 9 个 gate (tools/**)
  'tengu_quartz_lantern': 'mossen.tool.quartzLanternEnabled',
  'tengu_hive_evidence': 'mossen.tool.hiveEvidenceEnabled',
  'tengu_auto_background_agents': 'mossen.tool.autoBackgroundAgentsEnabled',
  'tengu_agent_list_attach': 'mossen.tool.agentListAttachEnabled',
  'tengu_amber_stoat': 'mossen.tool.amberStoatEnabled',
  'tengu_slim_subagent_mossenmd': 'mossen.tool.slimSubagentMossenmdEnabled',
  'tengu_glacier_2xr': 'mossen.tool.glacier2xrEnabled',
  'tengu_surreal_dali': 'mossen.tool.surrealDaliEnabled',
  'tengu_birch_trellis': 'mossen.tool.birchTrellisEnabled',
  // G4-4: permission setup / plan / default 域 3 个 key
  'tengu_destructive_command_warning': 'mossen.permission.destructiveCommandWarningEnabled',
  'tengu_plan_mode_interview_phase': 'mossen.permission.planModeInterviewPhaseEnabled',
  'tengu_pewter_ledger': 'mossen.permission.pewterLedgerVariant',
  // G4-5: bypass / yolo classifier 域 (utils/permissions/**)
  'tengu_scratch': 'mossen.permission.scratchpadEnabled',
  // G4-6: MCP / channel allowlist / channel permissions 域 (services/mcp/**)
  'tengu_harbor': 'mossen.permission.channelsEnabled',
  'tengu_harbor_permissions': 'mossen.permission.channelPermissionsAllowedEnabled',
  'tengu_harbor_ledger': 'mossen.permission.channelAllowlist',
  'tengu_auto_mode_config': 'mossen.ui.autoModeConfig',
  'tengu_vscode_review_upsell': 'mossen.mcp.vscodeReviewUpsellEnabled',
  'tengu_vscode_onboarding': 'mossen.mcp.vscodeOnboardingEnabled',
  'tengu_quiet_fern': 'mossen.mcp.quietFernEnabled',
  'tengu_vscode_cc_auth': 'mossen.mcp.vscodeCcAuthEnabled',
  // G4-7: model / thinking / effort / fallback 域
  'tengu_turtle_carbon': 'mossen.model.ultrathinkEnabled',
  'tengu_marble_sandcastle': 'mossen.model.fastModeRequiresNative',
  'tengu_otk_slot_v1': 'mossen.model.maxTokensCapEnabled',
  // G5-1: plugin / marketplace 域
  'tengu_lapis_finch': 'mossen.plugin.hintRecommendationEnabled',
  // G5-2: browser / chrome / computer-use 域
  'tengu_chrome_auto_enable': 'mossen.browser.chromeAutoEnable',
  'tengu_copper_bridge': 'mossen.browser.copperBridgeEnabled',
  // G5-3: native installer / update / remote session 域
  'tengu_remote_backend': 'mossen.session.remoteBackendEnabled',
  'tengu_desktop_upsell': 'mossen.installer.desktopUpsellConfig',
  'tengu_terminal_panel': 'mossen.ui.terminalPanelEnabled',
  'tengu_terminal_sidebar': 'mossen.ui.terminalSidebarEnabled',
  'tengu_kairos_brief': 'mossen.ui.kairosBriefEnabled',
  'tengu_thinkback': 'mossen.session.thinkbackEnabled',
}

/** 解析 GrowthBook 旧 key → Mossen 新 key. 未命中返回原 key. */
export function resolveAliasedKey(key: string): string {
  return GROWTHBOOK_TO_MOSSEN_ALIAS[key] ?? key
}
