//! Mossen built-in defaults table.

use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;

/// All built-in default configuration values keyed by mossen.<domain>.<feature>.
pub static MOSSEN_BUILTIN_DEFAULTS: Lazy<HashMap<&'static str, Value>> = Lazy::new(|| {
    let mut m: HashMap<&'static str, Value> = HashMap::new();

    // G3-1: analytics event batch config
    m.insert(
        "mossen.analytics.eventBatchConfig",
        json!({
            "scheduledDelayMillis": 60000,
            "maxExportBatchSize": 512,
            "maxQueueSize": 2048,
            "skipAuth": false
        }),
    );
    // G3-2: event sampling config
    m.insert("mossen.analytics.eventSamplingConfig", json!({}));
    // G3-3: sink killswitch
    m.insert("mossen.analytics.sinkKillswitch", json!({}));
    // G3-4: GB experiment exposure logging
    m.insert("mossen.analytics.gbExperimentExposureLogging", json!(false));

    // G4-1: Compact domain
    m.insert(
        "mossen.compact.timeBasedMCConfig",
        json!({
            "enabled": false,
            "gapThresholdMinutes": 60,
            "keepRecent": 5
        }),
    );
    m.insert("mossen.compact.cachePrefixSharing", json!(true));
    m.insert("mossen.compact.streamingRetryEnabled", json!(false));
    m.insert(
        "mossen.compact.sessionMemoryConfig",
        json!({
            "minTokens": 10000,
            "minTextBlockMessages": 5,
            "maxTokens": 40000
        }),
    );
    m.insert("mossen.compact.sessionMemoryEnabled", json!(false));
    m.insert("mossen.compact.sessionMemoryCompactEnabled", json!(false));
    m.insert("mossen.compact.reactiveAutoCompactKillswitch", json!(false));

    // G4-2: Memory domain
    m.insert("mossen.memory.coralFernEnabled", json!(false));
    m.insert("mossen.memory.skipDailyLogIndex", json!(false));
    m.insert("mossen.memory.kairosActive", json!(false));
    m.insert("mossen.memory.teamMemoryEnabled", json!(false));
    m.insert("mossen.memory.passportQuailEnabled", json!(false));
    m.insert("mossen.memory.slateThimbleEnabled", json!(false));

    // G4-3: Tool domain
    m.insert("mossen.tool.quartzLanternEnabled", json!(false));
    m.insert("mossen.tool.hiveEvidenceEnabled", json!(false));
    m.insert("mossen.tool.autoBackgroundAgentsEnabled", json!(false));
    m.insert("mossen.tool.agentListAttachEnabled", json!(false));
    m.insert("mossen.tool.amberStoatEnabled", json!(true));
    m.insert("mossen.tool.slimSubagentMossenmdEnabled", json!(true));
    m.insert("mossen.tool.glacier2xrEnabled", json!(false));
    m.insert("mossen.tool.surrealDaliEnabled", json!(false));
    m.insert("mossen.tool.birchTrellisEnabled", json!(true));

    // G4-4: Permission setup / plan / default domain
    m.insert(
        "mossen.permission.destructiveCommandWarningEnabled",
        json!(false),
    );
    m.insert(
        "mossen.permission.planModeInterviewPhaseEnabled",
        json!(false),
    );
    m.insert("mossen.permission.pewterLedgerVariant", Value::Null);

    // G4-5: Bypass / yolo classifier
    m.insert("mossen.permission.scratchpadEnabled", json!(false));

    // G4-6: MCP / channel allowlist
    m.insert("mossen.permission.channelsEnabled", json!(false));
    m.insert(
        "mossen.permission.channelPermissionsAllowedEnabled",
        json!(false),
    );
    m.insert("mossen.permission.channelAllowlist", json!([]));
    m.insert(
        "mossen.ui.autoModeConfig",
        json!({
            "enabled": "opt-in",
            "interactionLimitPerQuery": 10,
            "toolNameList": []
        }),
    );
    m.insert("mossen.mcp.vscodeReviewUpsellEnabled", json!(false));
    m.insert("mossen.mcp.vscodeOnboardingEnabled", json!(false));
    m.insert("mossen.mcp.quietFernEnabled", json!(false));
    m.insert("mossen.mcp.vscodeCcAuthEnabled", json!(false));

    // G4-7: Model / thinking / effort / fallback
    m.insert("mossen.model.ultrathinkEnabled", json!(true));
    m.insert("mossen.model.fastModeRequiresNative", json!(false));
    m.insert("mossen.model.maxTokensCapEnabled", json!(false));

    // G5-1: Plugin / marketplace
    m.insert("mossen.plugin.hintRecommendationEnabled", json!(false));

    // G5-2: Browser / Chrome / computer-use
    m.insert("mossen.browser.chromeAutoEnable", json!(false));
    m.insert("mossen.browser.copperBridgeEnabled", json!(false));

    // G5-3: Native installer / update / remote session
    m.insert("mossen.session.remoteBackendEnabled", json!(false));
    m.insert(
        "mossen.installer.desktopUpsellConfig",
        json!({
            "enable_shortcut_tip": false,
            "enable_startup_dialog": false
        }),
    );
    m.insert("mossen.ui.terminalPanelEnabled", json!(false));
    m.insert("mossen.ui.terminalSidebarEnabled", json!(false));
    m.insert("mossen.ui.kairosBriefEnabled", json!(false));
    m.insert("mossen.session.thinkbackEnabled", json!(false));

    m
});
