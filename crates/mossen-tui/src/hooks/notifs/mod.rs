//! Notification hooks — one-shot or reactive notifications shown in the UI.

mod auto_mode_unavailable;
mod can_switch_subscription;
mod deprecation_warning;
mod fast_mode;
mod ide_status_indicator;
mod install_messages;
mod lsp_initialization;
mod mcp_connectivity_status;
mod model_migration;
mod npm_deprecation;
mod plugin_autoupdate;
mod plugin_installation_status;
mod rate_limit_warning;
mod settings_errors;
mod startup;
mod teammate_shutdown;

pub use auto_mode_unavailable::*;
pub use can_switch_subscription::*;
pub use deprecation_warning::*;
pub use fast_mode::*;
pub use ide_status_indicator::*;
pub use install_messages::*;
pub use lsp_initialization::*;
pub use mcp_connectivity_status::*;
pub use model_migration::*;
pub use npm_deprecation::*;
pub use plugin_autoupdate::*;
pub use plugin_installation_status::*;
pub use rate_limit_warning::*;
pub use settings_errors::*;
pub use startup::*;
pub use teammate_shutdown::*;
