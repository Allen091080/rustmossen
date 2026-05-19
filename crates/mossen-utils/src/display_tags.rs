use regex::Regex;
use std::sync::LazyLock;

/// Matches any XML-like `<tag>…</tag>` block (lowercase tag names, optional
/// attributes, multi-line content). Used to strip system-injected wrapper tags
/// from display titles.
///
/// Only matches lowercase tag names (`[a-z][\w-]*`) so user prose mentioning
/// JSX/HTML components ("fix the <Button> layout") passes through.
static XML_TAG_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // We use a simple approach: find opening tags and their matching close tags.
    // Since Rust regex doesn't support backreferences, we use a replacement function.
    Regex::new(r"<([a-z][\w-]*)(?:\s[^>]*)?>[\s\S]*?</[a-z][\w-]*>\n?").unwrap()
});

/// Matches only IDE-injected context tags (ide_opened_file, ide_selection).
static IDE_CONTEXT_TAGS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(ide_opened_file|ide_selection)(?:\s[^>]*)?>[\s\S]*?</(?:ide_opened_file|ide_selection)>\n?").unwrap()
});

/// Strip XML-like tag blocks from text for use in UI titles.
/// System-injected context — IDE metadata, hook output, task notifications —
/// arrives wrapped in tags and should never surface as a title.
///
/// If stripping would result in empty text, returns the original unchanged
/// (better to show something than nothing).
pub fn strip_display_tags(text: &str) -> String {
    let result = XML_TAG_BLOCK_PATTERN.replace_all(text, "");
    let trimmed = result.trim();
    if trimmed.is_empty() {
        text.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Like `strip_display_tags` but returns empty string when all content is tags.
/// Used to detect command-only prompts (e.g. /clear) so they can fall through
/// to the next title fallback.
pub fn strip_display_tags_allow_empty(text: &str) -> String {
    let result = XML_TAG_BLOCK_PATTERN.replace_all(text, "");
    result.trim().to_string()
}

/// Strip only IDE-injected context tags (ide_opened_file, ide_selection).
/// Used by resubmit so UP-arrow resubmit preserves user-typed content
/// including lowercase HTML like `<code>foo</code>` while dropping IDE noise.
pub fn strip_ide_context_tags(text: &str) -> String {
    let result = IDE_CONTEXT_TAGS_PATTERN.replace_all(text, "");
    result.trim().to_string()
}
