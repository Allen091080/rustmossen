//! Context-window upgrade hints.
//!
//! Direct translation of `utils/model/contextWindowUpgradeCheck.ts`.

use super::check_1m_access::{check_opus_1m_access, check_sonnet_1m_access};
use super::model::get_user_specified_model_setting;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeContext {
    Warning,
    Tip,
}

struct AvailableUpgrade {
    alias: &'static str,
    name: &'static str,
    multiplier: u32,
}

fn get_available_upgrade() -> Option<AvailableUpgrade> {
    let setting = get_user_specified_model_setting().and_then(|x| x);
    let setting = setting.as_deref()?;
    match setting {
        "opus" if check_opus_1m_access() => Some(AvailableUpgrade {
            alias: "opus[1m]",
            name: "Mossen Frontier 1M",
            multiplier: 5,
        }),
        "sonnet" if check_sonnet_1m_access() => Some(AvailableUpgrade {
            alias: "sonnet[1m]",
            name: "Mossen Balanced 1M",
            multiplier: 5,
        }),
        _ => None,
    }
}

/// `getUpgradeMessage` — produce a localized hint string for the upgrade
/// affordance. Returns `None` when no upgrade is available.
pub fn get_upgrade_message(context: UpgradeContext) -> Option<String> {
    let upgrade = get_available_upgrade()?;
    match context {
        UpgradeContext::Warning => Some(format!("/model {}", upgrade.alias)),
        UpgradeContext::Tip => Some(localized_tip(upgrade.name, upgrade.multiplier)),
    }
}

fn localized_tip(name: &str, multiplier: u32) -> String {
    // The TS code consults `getLocalizedText({en, zh})`; we use the same
    // language hint stored in `MOSSEN_LANG` (set by `ui_language`) to choose
    // between EN/ZH bodies.
    let zh_preferred = matches!(
        std::env::var("MOSSEN_LANG").ok().as_deref(),
        Some("zh") | Some("zh-CN") | Some("zh-Hans") | Some("zh-Hant") | Some("zh-TW")
    );
    if zh_preferred {
        format!(
            "提示：你当前可使用 {}，上下文容量提升到 {} 倍",
            name, multiplier
        )
    } else {
        format!(
            "Tip: You have access to {} with {}x more context",
            name, multiplier
        )
    }
}
