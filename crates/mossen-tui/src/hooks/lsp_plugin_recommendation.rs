//! LSP plugin recommendation hook (useLspPluginRecommendation.tsx).
//!
//! Recommends LSP plugins based on detected file types in the workspace.

/// State for LSP plugin recommendations.
#[derive(Debug, Clone)]
pub struct LspPluginRecommendationState {
    pub recommendations: Vec<PluginRecommendation>,
    pub dismissed: Vec<String>,
    pub checked: bool,
}

#[derive(Debug, Clone)]
pub struct PluginRecommendation {
    pub plugin_id: String,
    pub plugin_name: String,
    pub language: String,
    pub reason: String,
}

impl LspPluginRecommendationState {
    pub fn new() -> Self {
        Self {
            recommendations: Vec::new(),
            dismissed: Vec::new(),
            checked: false,
        }
    }

    /// Check workspace and generate recommendations.
    pub fn check_workspace(&mut self, detected_languages: &[String], installed_plugins: &[String]) {
        self.checked = true;
        self.recommendations.clear();

        for lang in detected_languages {
            let plugin_id = match lang.as_str() {
                "typescript" | "javascript" => "typescript-lsp",
                "python" => "python-lsp",
                "rust" => "rust-analyzer",
                "go" => "gopls",
                _ => continue,
            };

            if !installed_plugins.contains(&plugin_id.to_string()) && !self.dismissed.contains(&plugin_id.to_string()) {
                self.recommendations.push(PluginRecommendation {
                    plugin_id: plugin_id.to_string(),
                    plugin_name: format!("{} Language Server", lang),
                    language: lang.clone(),
                    reason: format!("Detected {} files in workspace", lang),
                });
            }
        }
    }

    /// Dismiss a recommendation.
    pub fn dismiss(&mut self, plugin_id: &str) {
        self.recommendations.retain(|r| r.plugin_id != plugin_id);
        self.dismissed.push(plugin_id.to_string());
    }

    /// Get active recommendations.
    pub fn active(&self) -> &[PluginRecommendation] {
        &self.recommendations
    }
}

impl Default for LspPluginRecommendationState {
    fn default() -> Self {
        Self::new()
    }
}
