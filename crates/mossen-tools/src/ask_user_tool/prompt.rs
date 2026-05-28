//! AskUserQuestionTool prompt and constants.
//!
//! Translated from tools/AskUserQuestionTool/prompt.ts

use crate::exit_plan_mode_tool::constants::EXIT_PLAN_MODE_TOOL_NAME;

pub const ASK_USER_QUESTION_TOOL_NAME: &str = "AskUserQuestion";

pub const ASK_USER_QUESTION_TOOL_CHIP_WIDTH: usize = 12;

pub const DESCRIPTION: &str =
    "Asks the user multiple choice questions to gather information, clarify ambiguity, understand preferences, make decisions or offer them choices.";

/// `AskUserQuestionTool/prompt.ts` `ASK_USER_QUESTION_TOOL_PROMPT` — static
/// version of the prompt with `{exit_plan}` placeholder pre-rendered to the
/// canonical ExitPlanModeTool name. For dynamic rendering, see
/// `get_ask_user_question_tool_prompt`.
pub const ASK_USER_QUESTION_TOOL_PROMPT: &str = "Use this tool when you need to ask the user questions during execution. This allows you to:
1. Gather user preferences or requirements
2. Clarify ambiguous instructions
3. Get decisions on implementation choices as you work
4. Offer choices to the user about what direction to take.

Usage notes:
- Users will always be able to select \"Other\" to provide custom text input
- Use multiSelect: true to allow multiple answers to be selected for a question
- If you recommend a specific option, make that the first option in the list and add \"(Recommended)\" at the end of the label

Plan mode note: In plan mode, use this tool to clarify requirements or choose between approaches BEFORE finalizing your plan. Do NOT use this tool to ask \"Is my plan ready?\" or \"Should I proceed?\" - use ExitPlanMode for plan approval. IMPORTANT: Do not reference \"the plan\" in your questions (e.g., \"Do you have feedback about the plan?\", \"Does the plan look good?\") because the user cannot see the plan in the UI until you call ExitPlanMode. If you need plan approval, use ExitPlanMode instead.";

pub struct PreviewFeaturePrompt {
    pub markdown: &'static str,
    pub html: &'static str,
}

pub const PREVIEW_FEATURE_PROMPT: PreviewFeaturePrompt = PreviewFeaturePrompt {
    markdown: r#"
Preview feature:
Use the optional `preview` field on options when presenting concrete artifacts that users need to visually compare:
- ASCII mockups of UI layouts or components
- Code snippets showing different implementations
- Diagram variations
- Configuration examples

Preview content is rendered as markdown in a monospace box. Multi-line text with newlines is supported. When any option has a preview, the UI switches to a side-by-side layout with a vertical option list on the left and preview on the right. Do not use previews for simple preference questions where labels and descriptions suffice. Note: previews are only supported for single-select questions (not multiSelect).
"#,
    html: r#"
Preview feature:
Use the optional `preview` field on options when presenting concrete artifacts that users need to visually compare:
- HTML mockups of UI layouts or components
- Formatted code snippets showing different implementations
- Visual comparisons or diagrams

Preview content must be a self-contained HTML fragment (no <html>/<body> wrapper, no <script> or <style> tags — use inline style attributes instead). Do not use previews for simple preference questions where labels and descriptions suffice. Note: previews are only supported for single-select questions (not multiSelect).
"#,
};

pub fn get_ask_user_question_tool_prompt() -> String {
    format!(
        r#"Use this tool when you need to ask the user questions during execution. This allows you to:
1. Gather user preferences or requirements
2. Clarify ambiguous instructions
3. Get decisions on implementation choices as you work
4. Offer choices to the user about what direction to take.

Usage notes:
- Users will always be able to select "Other" to provide custom text input
- Use multiSelect: true to allow multiple answers to be selected for a question
- If you recommend a specific option, make that the first option in the list and add "(Recommended)" at the end of the label

Plan mode note: In plan mode, use this tool to clarify requirements or choose between approaches BEFORE finalizing your plan. Do NOT use this tool to ask "Is my plan ready?" or "Should I proceed?" - use {exit_plan} for plan approval. IMPORTANT: Do not reference "the plan" in your questions (e.g., "Do you have feedback about the plan?", "Does the plan look good?") because the user cannot see the plan in the UI until you call {exit_plan}. If you need plan approval, use {exit_plan} instead."#,
        exit_plan = EXIT_PLAN_MODE_TOOL_NAME
    )
}
