//! GrowthBook → Mossen key alias map.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Mapping from legacy mossen_* keys to mossen.* keys.
pub static GROWTHBOOK_TO_MOSSEN_ALIAS: Lazy<HashMap<&'static str, &'static str>> =
    Lazy::new(|| {
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();
        // G3-1: 1P event batch config
        m.insert(
            "mossen_1p_event_batch_config",
            "mossen.analytics.eventBatchConfig",
        );
        m.insert(
            "mossen_event_sampling_config",
            "mossen.analytics.eventSamplingConfig",
        );
        m.insert("mossen_frond_boric", "mossen.analytics.sinkKillswitch");
        // G4-1: compact domain
        m.insert("mossen_slate_heron", "mossen.compact.timeBasedMCConfig");
        m.insert(
            "mossen_compact_cache_prefix",
            "mossen.compact.cachePrefixSharing",
        );
        m.insert(
            "mossen_compact_streaming_retry",
            "mossen.compact.streamingRetryEnabled",
        );
        m.insert(
            "mossen_sm_compact_config",
            "mossen.compact.sessionMemoryConfig",
        );
        m.insert(
            "mossen_session_memory",
            "mossen.compact.sessionMemoryEnabled",
        );
        m.insert(
            "mossen_sm_compact",
            "mossen.compact.sessionMemoryCompactEnabled",
        );
        m.insert(
            "mossen_cobalt_raccoon",
            "mossen.compact.reactiveAutoCompactKillswitch",
        );
        // G4-2: memory domain
        m.insert("mossen_coral_fern", "mossen.memory.coralFernEnabled");
        m.insert("mossen_moth_copse", "mossen.memory.skipDailyLogIndex");
        m.insert("mossen_herring_clock", "mossen.memory.kairosActive");
        m.insert("mossen_team_memory", "mossen.memory.teamMemoryEnabled");
        m.insert(
            "mossen_passport_quail",
            "mossen.memory.passportQuailEnabled",
        );
        m.insert("mossen_slate_thimble", "mossen.memory.slateThimbleEnabled");
        // G4-3: tool domain
        m.insert("mossen_quartz_lantern", "mossen.tool.quartzLanternEnabled");
        m.insert("mossen_hive_evidence", "mossen.tool.hiveEvidenceEnabled");
        m.insert(
            "mossen_auto_background_agents",
            "mossen.tool.autoBackgroundAgentsEnabled",
        );
        m.insert(
            "mossen_agent_list_attach",
            "mossen.tool.agentListAttachEnabled",
        );
        m.insert("mossen_amber_stoat", "mossen.tool.amberStoatEnabled");
        m.insert(
            "mossen_slim_subagent_mossenmd",
            "mossen.tool.slimSubagentMossenmdEnabled",
        );
        m.insert("mossen_glacier_2xr", "mossen.tool.glacier2xrEnabled");
        m.insert("mossen_surreal_dali", "mossen.tool.surrealDaliEnabled");
        m.insert("mossen_birch_trellis", "mossen.tool.birchTrellisEnabled");
        // G4-4: permission setup
        m.insert(
            "mossen_destructive_command_warning",
            "mossen.permission.destructiveCommandWarningEnabled",
        );
        m.insert(
            "mossen_plan_mode_interview_phase",
            "mossen.permission.planModeInterviewPhaseEnabled",
        );
        m.insert(
            "mossen_pewter_ledger",
            "mossen.permission.pewterLedgerVariant",
        );
        // G4-5: bypass
        m.insert("mossen_scratch", "mossen.permission.scratchpadEnabled");
        // G4-6: MCP / channel
        m.insert("mossen_harbor", "mossen.permission.channelsEnabled");
        m.insert(
            "mossen_harbor_permissions",
            "mossen.permission.channelPermissionsAllowedEnabled",
        );
        m.insert("mossen_harbor_ledger", "mossen.permission.channelAllowlist");
        m.insert("mossen_auto_mode_config", "mossen.ui.autoModeConfig");
        m.insert(
            "mossen_vscode_review_upsell",
            "mossen.mcp.vscodeReviewUpsellEnabled",
        );
        m.insert(
            "mossen_vscode_onboarding",
            "mossen.mcp.vscodeOnboardingEnabled",
        );
        m.insert("mossen_quiet_fern", "mossen.mcp.quietFernEnabled");
        m.insert("mossen_vscode_cc_auth", "mossen.mcp.vscodeCcAuthEnabled");
        // G4-7: model / thinking
        m.insert("mossen_turtle_carbon", "mossen.model.ultrathinkEnabled");
        m.insert(
            "mossen_marble_sandcastle",
            "mossen.model.fastModeRequiresNative",
        );
        m.insert("mossen_otk_slot_v1", "mossen.model.maxTokensCapEnabled");
        // G5-1: plugin
        m.insert(
            "mossen_lapis_finch",
            "mossen.plugin.hintRecommendationEnabled",
        );
        // G5-2: browser
        m.insert(
            "mossen_chrome_auto_enable",
            "mossen.browser.chromeAutoEnable",
        );
        m.insert("mossen_copper_bridge", "mossen.browser.copperBridgeEnabled");
        // G5-3: native installer
        m.insert(
            "mossen_remote_backend",
            "mossen.session.remoteBackendEnabled",
        );
        m.insert(
            "mossen_desktop_upsell",
            "mossen.installer.desktopUpsellConfig",
        );
        m.insert("mossen_terminal_panel", "mossen.ui.terminalPanelEnabled");
        m.insert(
            "mossen_terminal_sidebar",
            "mossen.ui.terminalSidebarEnabled",
        );
        m.insert("mossen_kairos_brief", "mossen.ui.kairosBriefEnabled");
        m.insert("mossen_thinkback", "mossen.session.thinkbackEnabled");
        m
    });

/// Resolve a GrowthBook legacy key to Mossen key. Returns original key if not aliased.
pub fn resolve_aliased_key(key: &str) -> &str {
    match GROWTHBOOK_TO_MOSSEN_ALIAS.get(key) {
        Some(resolved) => resolved,
        None => key,
    }
}
