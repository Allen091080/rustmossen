//! Permission approval state for the active App flow.
//!
//! This module intentionally contains interaction state only. Rendering turns
//! it into `ApprovalRenderModel` and then uses `widgets::approval`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionKind {
    Shell { command: String },
    FileEdit { path: String },
    FileWrite { path: String },
    FileRead { path: String },
    WebFetch { url: String },
    Skill { name: String },
    UserQuestion { question: String },
    PlanMode { enter: bool },
    ComputerUse,
    Notebook { path: String },
    PowerShell { command: String },
    Filesystem { paths: Vec<String> },
    ToolUse { name: String },
}

impl PermissionKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Shell { .. } => "Shell Command",
            Self::FileEdit { .. } => "File Edit",
            Self::FileWrite { .. } => "File Write",
            Self::FileRead { .. } => "File Read",
            Self::WebFetch { .. } => "Web Fetch",
            Self::Skill { .. } => "Skill",
            Self::UserQuestion { .. } => "User Question",
            Self::PlanMode { enter: true } => "Enter Plan Mode",
            Self::PlanMode { enter: false } => "Exit Plan Mode",
            Self::ComputerUse => "Computer Use",
            Self::Notebook { .. } => "Notebook Edit",
            Self::PowerShell { .. } => "PowerShell",
            Self::Filesystem { .. } => "Filesystem Access",
            Self::ToolUse { .. } => "Tool Use",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Shell { command } | Self::PowerShell { command } => command.clone(),
            Self::FileEdit { path }
            | Self::FileWrite { path }
            | Self::FileRead { path }
            | Self::Notebook { path } => path.clone(),
            Self::WebFetch { url } => url.clone(),
            Self::Skill { name } | Self::ToolUse { name } => name.clone(),
            Self::UserQuestion { question } => question.clone(),
            Self::PlanMode { .. } => String::new(),
            Self::ComputerUse => "Computer interaction".into(),
            Self::Filesystem { paths } => paths.join(", "),
        }
    }

    pub fn detail_label(&self) -> &'static str {
        match self {
            Self::Shell { .. } | Self::PowerShell { .. } => "Command",
            Self::FileEdit { .. } => "Edit Path",
            Self::FileWrite { .. } => "Write Path",
            Self::FileRead { .. } => "Read Path",
            Self::WebFetch { .. } => "URL",
            Self::Skill { .. } => "Skill",
            Self::UserQuestion { .. } => "Question",
            Self::PlanMode { .. } => "Mode",
            Self::ComputerUse => "Target",
            Self::Notebook { .. } => "Notebook",
            Self::Filesystem { .. } => "Paths",
            Self::ToolUse { .. } => "Tool",
        }
    }

    pub fn editable_command(&self) -> Option<&str> {
        match self {
            Self::Shell { command } | Self::PowerShell { command } => {
                (!command.trim().is_empty()).then_some(command.as_str())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    Allow,
    AllowAlways,
    EditCommand,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessVerdict {
    Permit,
    Block,
}

#[derive(Debug, Clone)]
pub struct PermissionPromptState {
    pub kind: PermissionKind,
    pub tool_name: String,
    pub explanation: Option<String>,
    pub selected_action: PermissionAction,
    pub result: Option<AccessVerdict>,
    pub show_details: bool,
}

impl PermissionPromptState {
    pub fn new(kind: PermissionKind, tool_name: impl Into<String>) -> Self {
        Self {
            kind,
            tool_name: tool_name.into(),
            explanation: None,
            selected_action: PermissionAction::Allow,
            result: None,
            show_details: false,
        }
    }

    pub fn cycle_action(&mut self) {
        self.move_action(1);
    }

    pub fn cycle_action_back(&mut self) {
        self.move_action(-1);
    }

    pub fn available_actions(&self) -> Vec<PermissionAction> {
        let mut actions = vec![PermissionAction::Allow, PermissionAction::AllowAlways];
        if self.kind.editable_command().is_some() {
            actions.push(PermissionAction::EditCommand);
        }
        actions.push(PermissionAction::Deny);
        actions
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    pub fn confirm(&mut self) {
        self.result = Some(match self.selected_action {
            PermissionAction::Allow | PermissionAction::AllowAlways => AccessVerdict::Permit,
            PermissionAction::EditCommand | PermissionAction::Deny => AccessVerdict::Block,
        });
    }

    fn move_action(&mut self, delta: isize) {
        let actions = self.available_actions();
        if actions.is_empty() {
            return;
        }
        let index = actions
            .iter()
            .position(|action| *action == self.selected_action)
            .unwrap_or_default();
        let len = actions.len() as isize;
        let next = (index as isize + delta).rem_euclid(len) as usize;
        self.selected_action = actions[next];
    }
}

#[derive(Debug, Clone)]
pub struct ToolUseConfirm {
    pub tool_use_id: String,
    pub tool_name: String,
    pub raw_input: serde_json::Value,
    pub input_summary: String,
    pub risk_level: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_permissions_offer_edit_command_action() {
        let mut prompt = PermissionPromptState::new(
            PermissionKind::Shell {
                command: "cargo test".to_string(),
            },
            "Bash",
        );

        assert_eq!(
            prompt.available_actions(),
            vec![
                PermissionAction::Allow,
                PermissionAction::AllowAlways,
                PermissionAction::EditCommand,
                PermissionAction::Deny,
            ]
        );

        prompt.cycle_action();
        assert_eq!(prompt.selected_action, PermissionAction::AllowAlways);
        prompt.cycle_action();
        assert_eq!(prompt.selected_action, PermissionAction::EditCommand);
        prompt.cycle_action();
        assert_eq!(prompt.selected_action, PermissionAction::Deny);
    }

    #[test]
    fn non_command_permissions_do_not_offer_edit_command_action() {
        let mut prompt = PermissionPromptState::new(
            PermissionKind::FileWrite {
                path: "src/lib.rs".to_string(),
            },
            "Write",
        );

        assert_eq!(
            prompt.available_actions(),
            vec![
                PermissionAction::Allow,
                PermissionAction::AllowAlways,
                PermissionAction::Deny,
            ]
        );

        prompt.selected_action = PermissionAction::EditCommand;
        prompt.cycle_action();
        assert_eq!(prompt.selected_action, PermissionAction::AllowAlways);
    }
}
