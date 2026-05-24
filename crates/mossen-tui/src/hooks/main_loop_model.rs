//! Main loop model hook (useMainLoopModel.ts).
//!
//! Manages the active model selection for the main conversation loop.

/// State for main loop model selection.
#[derive(Debug, Clone)]
pub struct MainLoopModelState {
    pub current_model: String,
    pub available_models: Vec<ModelInfo>,
    pub fallback_model: Option<String>,
    pub is_fast_mode: bool,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub max_tokens: u32,
    pub supports_vision: bool,
    pub supports_tools: bool,
}

impl MainLoopModelState {
    pub fn new(default_model: &str) -> Self {
        Self {
            current_model: default_model.to_string(),
            available_models: Vec::new(),
            fallback_model: None,
            is_fast_mode: false,
        }
    }

    /// Set the current model.
    pub fn set_model(&mut self, model_id: String) {
        self.current_model = model_id;
    }

    /// Toggle fast mode.
    pub fn toggle_fast_mode(&mut self) {
        self.is_fast_mode = !self.is_fast_mode;
    }

    /// Set available models.
    pub fn set_available_models(&mut self, models: Vec<ModelInfo>) {
        self.available_models = models;
    }

    /// Get current model info.
    pub fn current_model_info(&self) -> Option<&ModelInfo> {
        self.available_models
            .iter()
            .find(|m| m.id == self.current_model)
    }

    /// Get the effective model (considering fast mode and fallback).
    pub fn effective_model(&self) -> &str {
        if self.is_fast_mode {
            self.fallback_model
                .as_deref()
                .unwrap_or(&self.current_model)
        } else {
            &self.current_model
        }
    }
}

impl Default for MainLoopModelState {
    fn default() -> Self {
        Self::new("default")
    }
}
