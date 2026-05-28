//! Permission context (PermissionContext.ts).
//! Provides the permission mode and tool approval state.

/// Permission mode for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    Plan,
    Auto,
}

impl PermissionMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "plan" => Self::Plan,
            "auto" => Self::Auto,
            _ => Self::Default,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Default => "default",
            Self::Plan => "plan",
            Self::Auto => "auto",
        }
    }
}

/// State for the permission context.
#[derive(Debug, Clone)]
pub struct PermissionContextState {
    pub mode: PermissionMode,
    pub is_auto_mode_available: bool,
    pub always_approved_tools: Vec<String>,
    pub session_approved: Vec<String>,
}

impl PermissionContextState {
    pub fn new() -> Self {
        Self {
            mode: PermissionMode::Default,
            is_auto_mode_available: false,
            always_approved_tools: Vec::new(),
            session_approved: Vec::new(),
        }
    }

    /// Set the permission mode.
    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Cycle to the next permission mode.
    pub fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            PermissionMode::Default => PermissionMode::Plan,
            PermissionMode::Plan => {
                if self.is_auto_mode_available {
                    PermissionMode::Auto
                } else {
                    PermissionMode::Default
                }
            }
            PermissionMode::Auto => PermissionMode::Default,
        };
    }

    /// Check if a tool is approved (always or for this session).
    pub fn is_tool_approved(&self, tool_name: &str) -> bool {
        self.mode == PermissionMode::Auto
            || self.always_approved_tools.contains(&tool_name.to_string())
            || self.session_approved.contains(&tool_name.to_string())
    }

    /// Approve a tool for this session.
    pub fn approve_for_session(&mut self, tool_name: &str) {
        if !self.session_approved.contains(&tool_name.to_string()) {
            self.session_approved.push(tool_name.to_string());
        }
    }

    /// Approve a tool permanently.
    pub fn approve_always(&mut self, tool_name: &str) {
        if !self.always_approved_tools.contains(&tool_name.to_string()) {
            self.always_approved_tools.push(tool_name.to_string());
        }
    }
}

impl Default for PermissionContextState {
    fn default() -> Self {
        Self::new()
    }
}
