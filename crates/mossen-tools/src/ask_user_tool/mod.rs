pub mod prompt;

// ---------------------------------------------------------------------------
// TS-mirror — `tools/AskUserQuestionTool/AskUserQuestionTool.tsx` exports.
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

/// SDK-shape input schema marker (TS `_sdkInputSchema`).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default)]
pub struct _sdkInputSchema;

/// SDK-shape output schema marker (TS `_sdkOutputSchema`).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Default)]
pub struct _sdkOutputSchema;

/// `AskUserQuestionTool.tsx` `AskUserQuestionTool` — value-shape constant.
#[derive(Debug, Clone, Default)]
pub struct AskUserQuestionTool;

impl AskUserQuestionTool {
    pub const TOOL_NAME: &'static str = "AskUserQuestion";
}

/// `AskUserQuestionTool.tsx` `Output`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Output {
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    #[serde(default)]
    pub aborted: bool,
}
