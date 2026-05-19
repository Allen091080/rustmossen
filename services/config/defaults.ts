/**
 * Mossen 内置默认值表 (G1-2 起步).
 *
 * G3-G5 每迁一个 GrowthBook key, 同步往这里添加 entry.
 * tmp/growthbook-audit/keys.json 是审计产物 (在 tmp/, gitignored, 仅参考),
 * 本文件才是 git tracked 的运行时真值.
 *
 * 命名规范 (见 services/config/types.ts MOSSEN_KEY_PATTERN):
 *   mossen.<domain>.<feature>
 */
export const MOSSEN_BUILTIN_DEFAULTS: Record<string, unknown> = {
  // G3-1: tengu_1p_event_batch_config → mossen.analytics.eventBatchConfig
  // 默认与 firstPartyEventLogger.ts 历史 GrowthBook 默认值一致
  // (tmp/growthbook-audit/keys.json: high-risk #1)
  'mossen.analytics.eventBatchConfig': {
    scheduledDelayMillis: 60000,
    maxExportBatchSize: 512,
    maxQueueSize: 2048,
    skipAuth: false,
  },
  // G3-2: tengu_event_sampling_config → mossen.analytics.eventSamplingConfig
  // 真实代码默认是 {} (firstPartyEventLogger.ts:39 caller fallback);
  // 含义: 没有 per-event sampling override, 所有事件 100% 上报.
  // ⚠ 审计 keys.json 的 proposed_default 用了 traceSamplePercentage 等字段,
  // 但代码层的实际 type 是 {[eventName]: {sample_rate: number}}, 两者形状不一致.
  // 这里以代码现实为准, 故 G3-2 不进 R8 STRICT (避免 audit 数据失配引起假 drift).
  'mossen.analytics.eventSamplingConfig': {},
  // G3-3: tengu_frond_boric → mossen.analytics.sinkKillswitch
  // (mangled name 还原为可读名)
  // 默认 {} (sinkKillswitch.ts:18 comment "nothing killed. Fail-open").
  // shape: { datadog?: boolean, firstParty?: boolean }
  'mossen.analytics.sinkKillswitch': {},
  // G3-4: GrowthBook 实验曝光上报开关 (gate)
  // 默认关闭 — Mossen 个人版不上传 GrowthBook 实验数据
  // 调用方: services/analytics/growthbook.ts logExposureForFeature
  'mossen.analytics.gbExperimentExposureLogging': false,
  // ====================================
  // G4-1: Compact 域 (services/compact/**)
  // ====================================
  // tengu_slate_heron → time-based message-compaction config
  // src: services/compact/timeBasedMCConfig.ts:35 TIME_BASED_MC_CONFIG_DEFAULTS
  'mossen.compact.timeBasedMCConfig': {
    enabled: false,
    gapThresholdMinutes: 60,
    keepRecent: 5,
  },
  // tengu_compact_cache_prefix → forked-agent prompt cache reuse killswitch
  // src: services/compact/compact.ts:435/1156 (3P default true)
  'mossen.compact.cachePrefixSharing': true,
  // tengu_compact_streaming_retry → 流式压缩失败重试 (默认 false)
  // src: services/compact/compact.ts:1252
  'mossen.compact.streamingRetryEnabled': false,
  // tengu_sm_compact_config → SessionMemory compact 阈值
  // src: services/compact/sessionMemoryCompact.ts:118 DEFAULT_SM_COMPACT_CONFIG
  // ⚠ 形状与 audit keys.json 不一致 (audit 是 enabled/minMessages/compressionRatio,
  //   代码是 minTokens/minTextBlockMessages/maxTokens), 以代码现实为准, 不进 STRICT
  'mossen.compact.sessionMemoryConfig': {
    minTokens: 10000,
    minTextBlockMessages: 5,
    maxTokens: 40000,
  },
  // tengu_session_memory → session memory 总开关 (默认关闭, Mossen 个人版未默认上)
  // src: services/compact/sessionMemoryCompact.ts:413
  'mossen.compact.sessionMemoryEnabled': false,
  // tengu_sm_compact → session memory compact 子开关
  // src: services/compact/sessionMemoryCompact.ts:417
  'mossen.compact.sessionMemoryCompactEnabled': false,
  // tengu_cobalt_raccoon → REACTIVE_COMPACT 反向 killswitch
  // src: services/compact/autoCompact.ts:196
  'mossen.compact.reactiveAutoCompactKillswitch': false,
  // ====================================
  // G4-2: Memory / session-context 域 (memdir/**)
  // 6 unique gate, 全部默认 false (Mossen 个人版未默认上 auto-memory 实验路径)
  // ====================================
  // tengu_coral_fern: memdir 某 gate (memdir/memdir.ts:383)
  'mossen.memory.coralFernEnabled': false,
  // tengu_moth_copse: skipIndex for assistant daily log (memdir/memdir.ts:430)
  'mossen.memory.skipDailyLogIndex': false,
  // tengu_herring_clock: KAIROS daily-log mode gate (memdir/memdir.ts:440)
  // NOTE: This gate controls ONLY KAIROS daily-log mode, NOT team memory.
  // Team memory has its own independent gate (tengu_team_memory → mossen.memory.teamMemoryEnabled).
  'mossen.memory.kairosActive': false,
  // tengu_team_memory: team memory runtime gate (memdir/teamMemPaths.ts:77)
  // Explicit independent gate for team memory. Default false — team memory is
  // opt-in even when TEAMMEM build flag is present. Must be explicitly enabled
  // via MOSSEN_CONFIG_OVERRIDES or settings to write team memories.
  'mossen.memory.teamMemoryEnabled': false,
  // tengu_passport_quail: memory paths gate (memdir/paths.ts:81)
  'mossen.memory.passportQuailEnabled': false,
  // tengu_slate_thimble: memory paths gate (memdir/paths.ts:90)
  'mossen.memory.slateThimbleEnabled': false,
  // ====================================
  // G4-3: Tool 域 (tools/**) 9 unique gate
  // 仅 amber_stoat / slim_subagent_mossenmd / birch_trellis 默认 true
  // ====================================
  'mossen.tool.quartzLanternEnabled': false,         // tengu_quartz_lantern (Edit/Write)
  'mossen.tool.hiveEvidenceEnabled': false,          // tengu_hive_evidence (TaskUpdate/Agent/Todo)
  'mossen.tool.autoBackgroundAgentsEnabled': false,  // tengu_auto_background_agents
  'mossen.tool.agentListAttachEnabled': false,       // tengu_agent_list_attach
  'mossen.tool.amberStoatEnabled': true,             // tengu_amber_stoat (builtInAgents)
  'mossen.tool.slimSubagentMossenmdEnabled': true,   // tengu_slim_subagent_mossenmd
  'mossen.tool.glacier2xrEnabled': false,            // tengu_glacier_2xr (ToolSearch)
  'mossen.tool.surrealDaliEnabled': false,           // tengu_surreal_dali (RemoteTrigger)
  'mossen.tool.birchTrellisEnabled': true,           // tengu_birch_trellis (bashPermissions)
  // ====================================
  // G4-4: Permission setup / plan / default 域
  // ====================================
  // tengu_destructive_command_warning (BashPermissionRequest:275, PowerShell:61)
  'mossen.permission.destructiveCommandWarningEnabled': false,
  // tengu_plan_mode_interview_phase (utils/planModeV2.ts:59) — interview-phase 5-phase plan
  // env override existed; gate 仅作 fallback
  'mossen.permission.planModeInterviewPhaseEnabled': false,
  // tengu_pewter_ledger (utils/planModeV2.ts:90) — 'trim' | 'cut' | 'cap' | null
  // 默认 null (control arm)
  'mossen.permission.pewterLedgerVariant': null,
  // ====================================
  // G4-5: Bypass / yolo classifier 域 (utils/permissions/**)
  // ====================================
  // tengu_scratch Statsig gate (utils/permissions/filesystem.ts isScratchpadEnabled)
  // 默认 false (Mossen 个人版未默认启用 scratchpad)
  'mossen.permission.scratchpadEnabled': false,
  // ====================================
  // G4-6: MCP / channel allowlist / channel permissions 域 (services/mcp/**)
  // ====================================
  // tengu_harbor → mossen.permission.channelsEnabled
  // src: services/mcp/channelAllowlist.ts:52  default false
  // 与 audit keys.json 完全 parity → 加入 R8 STRICT
  'mossen.permission.channelsEnabled': false,
  // tengu_harbor_permissions (channelPermissions.ts:41) - gate 形式, 代码默认 false
  // 形状与 audit keys.json 失配 (audit 是 {allowChannelCreation,...} 对象, 代码是
  // boolean), 以代码为准, 不进 STRICT
  'mossen.permission.channelPermissionsAllowedEnabled': false,
  // tengu_harbor_ledger (channelAllowlist.ts:39) - 实际类型 ChannelAllowlistEntry[]
  // 代码默认 []; audit 形状失配 (audit 是 {maxChannels, maxMembers} 对象), 不进 STRICT
  'mossen.permission.channelAllowlist': [],
  // tengu_auto_mode_config (vscodeSdkMcp.ts:16) - 默认 {} (caller fallback)
  // ⚠ audit proposed_default 是 {enabled:'opt-in', interactionLimitPerQuery:10, toolNameList:[]},
  //   audit 字段比代码多 (代码只读 ?.enabled), 形状不算严格失配但不完全一致;
  //   按 audit 默认值丰富以满足审计契约 (代码读 .enabled 仍正常工作);
  //   key 走 audit 命名 mossen.ui.autoModeConfig (跨 MCP/UI 域)
  'mossen.ui.autoModeConfig': {
    enabled: 'opt-in',
    interactionLimitPerQuery: 10,
    toolNameList: [],
  },
  // tengu_vscode_review_upsell (vscodeSdkMcp:84) - Statsig gate 默认 false (个人版无 vscode upsell)
  'mossen.mcp.vscodeReviewUpsellEnabled': false,
  // tengu_vscode_onboarding (vscodeSdkMcp:87) - 默认 false
  'mossen.mcp.vscodeOnboardingEnabled': false,
  // tengu_quiet_fern (vscodeSdkMcp:91) - 默认 false (browser support gate)
  'mossen.mcp.quietFernEnabled': false,
  // tengu_vscode_cc_auth (vscodeSdkMcp:96) - 默认 false (in-band OAuth)
  'mossen.mcp.vscodeCcAuthEnabled': false,
  // ====================================
  // G4-7: Model / thinking / effort / fallback 域
  // ====================================
  // tengu_turtle_carbon (utils/thinking.ts:25 isUltrathinkEnabled) - 默认 true
  // ULTRATHINK feature 开启后, 该 gate 控制 ultrathink 是否启用
  'mossen.model.ultrathinkEnabled': true,
  // tengu_marble_sandcastle (utils/fastMode.ts:95) - 默认 false
  // 控制是否要求 fast mode 用 native binary (legacy 兼容)
  'mossen.model.fastModeRequiresNative': false,
  // tengu_otk_slot_v1 (services/api/mossen.ts:3381 isMaxTokensCapEnabled)
  // 默认 false (3P 未验证 max-tokens cap on Bedrock/Vertex)
  'mossen.model.maxTokensCapEnabled': false,
  // ====================================
  // G5-1: Plugin / marketplace / official startup check 域
  // ====================================
  // tengu_lapis_finch (utils/plugins/hintRecommendation.ts:66 maybeRecordPluginHint)
  // 默认 false - 个人版默认隐藏 plugin hint 弹窗 (避免 marketplace upsell)
  'mossen.plugin.hintRecommendationEnabled': false,
  // ====================================
  // G5-2: Browser / Chrome / computer-use 域 (utils/mossenInChrome/**)
  // ====================================
  // tengu_chrome_auto_enable (utils/mossenInChrome/setup.ts:92)
  // 默认 false - 个人版默认不自动启用 mossen-in-chrome 集成
  'mossen.browser.chromeAutoEnable': false,
  // tengu_copper_bridge (utils/mossenInChrome/mcpServer.ts:55)
  // 默认 false - 个人版默认不开 chrome-bridge MCP server
  'mossen.browser.copperBridgeEnabled': false,
  // ====================================
  // G5-3: Native installer / update / remote session 域
  // ====================================
  // tengu_remote_backend (main.tsx:3415 isRemoteTuiEnabled) - 默认 false
  // 个人版无 remote backend
  'mossen.session.remoteBackendEnabled': false,
  // tengu_desktop_upsell (DesktopUpsellStartup.tsx:23) - 配置 DesktopUpsellConfig
  'mossen.installer.desktopUpsellConfig': {
    enable_shortcut_tip: false,
    enable_startup_dialog: false,
  },
  // tengu_terminal_panel (PromptInputHelpMenu.tsx:133, useGlobalKeybindings:212) - 默认 false
  'mossen.ui.terminalPanelEnabled': false,
  // tengu_terminal_sidebar (REPL.tsx:1165, Settings/Config.tsx:457) - 默认 false
  'mossen.ui.terminalSidebarEnabled': false,
  // tengu_kairos_brief (Spinner:112, UserPromptMessage:61, BriefTool:95) - 默认 false
  'mossen.ui.kairosBriefEnabled': false,
  // tengu_thinkback (commands/thinkback/index.ts:10, thinkback-play:11) - 默认 false
  'mossen.session.thinkbackEnabled': false,
}
