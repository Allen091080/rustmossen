//! # Tool Limits (toolLimits.ts)
//!
//! 工具结果大小限制常量。

/// Default maximum size in characters for tool results before they get persisted
/// to disk. When exceeded, the result is saved to a file and the model receives
/// a preview with the file path instead of the full content.
///
/// Individual tools may declare a lower maxResultSizeChars, but this constant
/// acts as a system-wide cap regardless of what tools declare.
pub const DEFAULT_MAX_RESULT_SIZE_CHARS: usize = 50_000;

/// Maximum size for tool results in tokens.
/// Based on analysis of tool result sizes, we set this to a reasonable upper bound
/// to prevent excessively large tool results from consuming too much context.
///
/// This is approximately 400KB of text (assuming ~4 bytes per token).
pub const MAX_TOOL_RESULT_TOKENS: usize = 100_000;

/// Bytes per token estimate for calculating token count from byte size.
/// This is a conservative estimate - actual token count may vary.
pub const BYTES_PER_TOKEN: usize = 4;

/// Maximum size for tool results in bytes (derived from token limit).
pub const MAX_TOOL_RESULT_BYTES: usize = MAX_TOOL_RESULT_TOKENS * BYTES_PER_TOKEN;

/// Default maximum aggregate size in characters for tool_result blocks within
/// a SINGLE user message (one turn's batch of parallel tool results). When a
/// message's blocks together exceed this, the largest blocks in that message
/// are persisted to disk and replaced with previews until under budget.
/// Messages are evaluated independently — a 150K result in one turn and a
/// 150K result in the next are both untouched.
///
/// This prevents N parallel tools from each hitting the per-tool max and
/// collectively producing e.g. 10 × 40K = 400K in one turn's user message.
///
/// Overridable at runtime via GrowthBook flag `mossen_hawthorn_window` — see
/// `getPerMessageBudgetLimit()` in toolResultStorage.ts.
pub const MAX_TOOL_RESULTS_PER_MESSAGE_CHARS: usize = 200_000;

/// Maximum character length for tool summary strings in compact views.
/// Used by getToolUseSummary() implementations to truncate long inputs
/// for display in grouped agent rendering.
pub const TOOL_SUMMARY_MAX_LENGTH: usize = 50;
