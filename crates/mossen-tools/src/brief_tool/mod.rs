pub mod attachments;
pub mod prompt;

pub use attachments::{resolve_attachments, validate_attachment_paths, ResolvedAttachment};
pub use prompt::*;

// ---------------------------------------------------------------------------
// TS-mirror — `tools/BriefTool/BriefTool.ts` exports.
// ---------------------------------------------------------------------------

/// `BriefTool.ts` `Output` shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Output {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bullets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// `BriefTool.ts` `BriefTool` — value-shape constant.
#[derive(Debug, Clone, Default)]
pub struct BriefTool;

impl BriefTool {
    pub const TOOL_NAME: &'static str = "Brief";
}

/// `BriefTool.ts` `isBriefEntitled`.
pub fn is_brief_entitled() -> bool {
    matches!(
        std::env::var("MOSSEN_BRIEF_ENTITLED").as_deref(),
        Ok("1" | "true" | "TRUE")
    )
}

/// `BriefTool.ts` `isBriefEnabled`.
pub fn is_brief_enabled() -> bool {
    is_brief_entitled()
        && !matches!(
            std::env::var("MOSSEN_DISABLE_BRIEF").as_deref(),
            Ok("1" | "true" | "TRUE")
        )
}
