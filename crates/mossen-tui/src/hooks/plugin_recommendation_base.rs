//! Plugin recommendation base.
//! Base logic for recommending plugins based on workspace analysis.

#[derive(Debug, Clone)]
pub struct PluginRecommendationEntry {
    pub plugin_id: String,
    pub reason: String,
    pub priority: u8,
    pub auto_install: bool,
}

#[derive(Debug, Clone)]
pub struct PluginRecommendationBaseState {
    pub recommendations: Vec<PluginRecommendationEntry>,
    pub dismissed: Vec<String>,
    pub auto_installed: Vec<String>,
}

impl PluginRecommendationBaseState {
    pub fn new() -> Self {
        Self {
            recommendations: Vec::new(),
            dismissed: Vec::new(),
            auto_installed: Vec::new(),
        }
    }
    pub fn add_recommendation(&mut self, rec: PluginRecommendationEntry) {
        if !self.dismissed.contains(&rec.plugin_id) && !self.auto_installed.contains(&rec.plugin_id)
        {
            self.recommendations.push(rec);
        }
    }
    pub fn dismiss(&mut self, plugin_id: &str) {
        self.recommendations.retain(|r| r.plugin_id != plugin_id);
        self.dismissed.push(plugin_id.to_string());
    }
    pub fn mark_auto_installed(&mut self, plugin_id: &str) {
        self.recommendations.retain(|r| r.plugin_id != plugin_id);
        self.auto_installed.push(plugin_id.to_string());
    }
    pub fn pending(&self) -> &[PluginRecommendationEntry] {
        &self.recommendations
    }
}
impl Default for PluginRecommendationBaseState {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of an `install_plugin_and_notify` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPluginNotification {
    pub key: String,
    pub text: String,
    pub color: String,
}

/// Result type representing the success or failure of the install
/// callback. The TS version awaits an arbitrary `install(pluginData)`
/// callback; the Rust port asks the caller to translate the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed,
    NotFound,
    Failed(String),
}

/// Look up a plugin, run the install callback, and emit the standard
/// success/failure notification.
///
/// TS source: `installPluginAndNotify(...)`.
pub fn install_plugin_and_notify(
    plugin_name: &str,
    key_prefix: &str,
    outcome: InstallOutcome,
) -> InstallPluginNotification {
    match outcome {
        InstallOutcome::Installed => InstallPluginNotification {
            key: format!("{}-installed", key_prefix),
            text: format!("✓ {} installed · restart to apply", plugin_name),
            color: "success".to_string(),
        },
        InstallOutcome::NotFound | InstallOutcome::Failed(_) => InstallPluginNotification {
            key: format!("{}-install-failed", key_prefix),
            text: format!("Failed to install {}", plugin_name),
            color: "error".to_string(),
        },
    }
}
