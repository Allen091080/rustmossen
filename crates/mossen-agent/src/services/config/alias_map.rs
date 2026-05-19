//! GrowthBook → Mossen key alias map.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Mapping from legacy tengu_* keys to mossen.* keys.
pub static GROWTHBOOK_TO_MOSSEN_ALIAS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m: HashMap<&'static str, &'static str> = HashMap::new();
    // G3-1: 1P event batch config
    m.insert("tengu_1p_event_batch_config", "mossen.analytics.eventBatchConfig");
    m.insert("tengu_event_sampling_config", "mossen.analytics.eventSamplingConfig");
    m.insert("tengu_frond_boric", "mossen.analytics.sinkKillswitch");
    // G4-1: compact domain
    m.insert("tengu_slate_heron", "mossen.compact.timeBasedMCConfig");
    m.insert("tengu_compact_cache_prefix", "mossen.compact.cachePrefixSharing");
    m.insert("tengu_compact_streaming_retry", "mossen.compact.streamingRetryEnabled");
    m.insert("tengu_sm_compact_config", "mossen.compact.sessionMemoryConfig");
    m.insert("tengu_session_memory", "mossen.compact.sessionMemoryEnabled");
    m.insert("tengu_sm_compact", "mossen.compact.sessionMemoryCompactEnabled");
    m.insert("tengu_cobalt_raccoon", "mossen.compact.reactiveAutoCompactKillswitch");
    // G4-2: memory domain
    m.insert("tengu_coral_fern", "mossen.memory.coralFernEnabled");
    m.insert("tengu_moth_copse", "mossen.memory.skipDailyLogIndex");
    m.insert("tengu_herring_clock", "mossen.memory.kairosActive");
    m.insert("tengu_team_memory", "mossen.memory.teamMemoryEnabled");
    m.insert("tengu_passport_quail", "mossen.memory.passportQuailEnabled");
    m.insert("tengu_slate_thimble", "mossen.memory.slateThimbleEnabled");
    // G4-3: tool domain
    m.insert("tengu_quartz_lantern", "mossen.tool.quartzLanternEnabled");
    m.insert("tengu_hive_evidence", "mossen.tool.hiveEvidenceEnabled");
    m.insert("tengu_auto_background_agents", "mossen.tool.autoBackgroundAgentsEnabled");
    m.insert("tengu_agent_list_attach", "mossen.tool.agentListAttachEnabled");
    m.insert("tengu_amber_stoat", "mossen.tool.amberStoatEnabled");
    m.insert("tengu_slim_subagent_mossenmd", "mossen.tool.slimSubagentMossenmdEnabled");
    m.insert("tengu_glacier_2xr", "mossen.tool.glacier2xrEnabled");
    m.insert("tengu_surreal_dali", "mossen.tool.surrealDaliEnabled");
    m.insert("tengu_birch_trellis", "mossen.tool.birchTrellisEnabled");
    // G4-4: permission setup
    m.insert("tengu_destructive_command_warning", "mossen.permission.destructiveCommandWarningEnabled");
    m.insert("tengu_plan_mode_interview_phase", "mossen.permission.planModeInterviewPhaseEnabled");
    m.insert("tengu_pewter_ledger", "mossen.permission.pewterLedgerVariant");
    // G4-5: bypass
    m.insert("tengu_scratch", "mossen.permission.scratchpadEnabled");
    // G4-6: MCP / channel
    m.insert("tengu_harbor", "mossen.permission.channelsEnabled");
    m.insert("tengu_harbor_permissions", "mossen.permission.channelPermissionsAllowedEnabled");
    m.insert("tengu_harbor_ledger", "mossen.permission.channelAllowlist");
    m.insert("tengu_auto_mode_config", "mossen.ui.autoModeConfig");
    m.insert("tengu_vscode_review_upsell", "mossen.mcp.vscodeReviewUpsellEnabled");
    m.insert("tengu_vscode_onboarding", "mossen.mcp.vscodeOnboardingEnabled");
    m.insert("tengu_quiet_fern", "mossen.mcp.quietFernEnabled");
    m.insert("tengu_vscode_cc_auth", "mossen.mcp.vscodeCcAuthEnabled");
    // G4-7: model / thinking
    m.insert("tengu_turtle_carbon", "mossen.model.ultrathinkEnabled");
    m.insert("tengu_marble_sandcastle", "mossen.model.fastModeRequiresNative");
    m.insert("tengu_otk_slot_v1", "mossen.model.maxTokensCapEnabled");
    // G5-1: plugin
    m.insert("tengu_lapis_finch", "mossen.plugin.hintRecommendationEnabled");
    // G5-2: browser
    m.insert("tengu_chrome_auto_enable", "mossen.browser.chromeAutoEnable");
    m.insert("tengu_copper_bridge", "mossen.browser.copperBridgeEnabled");
    // G5-3: native installer
    m.insert("tengu_remote_backend", "mossen.session.remoteBackendEnabled");
    m.insert("tengu_desktop_upsell", "mossen.installer.desktopUpsellConfig");
    m.insert("tengu_terminal_panel", "mossen.ui.terminalPanelEnabled");
    m.insert("tengu_terminal_sidebar", "mossen.ui.terminalSidebarEnabled");
    m.insert("tengu_kairos_brief", "mossen.ui.kairosBriefEnabled");
    m.insert("tengu_thinkback", "mossen.session.thinkbackEnabled");
    m
});

/// Resolve a GrowthBook legacy key to Mossen key. Returns original key if not aliased.
pub fn resolve_aliased_key(key: &str) -> &str {
    match GROWTHBOOK_TO_MOSSEN_ALIAS.get(key) {
        Some(resolved) => resolved,
        None => key,
    }
}
