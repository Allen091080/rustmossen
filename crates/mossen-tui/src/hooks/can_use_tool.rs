//! Can-use-tool permission check (useCanUseTool.tsx).
//!
//! Determines whether the current user/session can use a specific tool,
//! checking feature flags, permission mode, and tool allowlists.

/// Result of a can-use-tool check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanUseToolResult {
    Allowed,
    Denied { reason: String },
    RequiresApproval,
    FeatureDisabled,
}

/// State for tool usage permission checking.
#[derive(Debug, Clone)]
pub struct CanUseToolState {
    pub tool_name: String,
    pub result: CanUseToolResult,
    pub checked: bool,
}

impl CanUseToolState {
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            result: CanUseToolResult::Allowed,
            checked: false,
        }
    }

    /// Check if a tool can be used given the current permission context.
    pub fn check(
        &mut self,
        permission_mode: &str,
        is_auto_mode_available: bool,
        tool_allowlist: &[String],
        feature_transcript_classifier: bool,
    ) {
        self.checked = true;

        // Feature flag check
        if !feature_transcript_classifier && self.tool_name == "auto_approve" {
            self.result = CanUseToolResult::FeatureDisabled;
            return;
        }

        // Allowlist check
        if !tool_allowlist.is_empty() && !tool_allowlist.contains(&self.tool_name) {
            self.result = CanUseToolResult::Denied {
                reason: format!("Tool '{}' not in allowlist", self.tool_name),
            };
            return;
        }

        // Permission mode check
        match permission_mode {
            "auto" if is_auto_mode_available => {
                self.result = CanUseToolResult::Allowed;
            }
            "plan" => {
                self.result = CanUseToolResult::RequiresApproval;
            }
            _ => {
                self.result = CanUseToolResult::Allowed;
            }
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.result, CanUseToolResult::Allowed)
    }
}
