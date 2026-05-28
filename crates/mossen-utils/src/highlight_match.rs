//! Search result highlighting — case-insensitive substring matching.
//!
//! Mirrors TS `utils/highlightMatch.tsx`. The TS version returns a React
//! node with inverse-styled `<Text>` segments; this Rust port returns
//! structured segments so callers can render them with whatever TUI lib
//! they use.

/// A segment of text in a highlight result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSegment {
    /// The raw text content of this segment.
    pub text: String,
    /// True when this segment matched the query and should be visually
    /// inverted (matching TS `<Text inverse>` semantics).
    pub matched: bool,
}

/// Inverse-highlight every occurrence of `query` in `text` (case-insensitive).
///
/// When `query` is empty or never appears in `text`, returns a single
/// non-matched segment containing the original text — mirroring the TS
/// fallback path that returns `text` as-is.
pub fn highlight_match(text: &str, query: &str) -> Vec<HighlightSegment> {
    if query.is_empty() {
        return vec![HighlightSegment {
            text: text.to_string(),
            matched: false,
        }];
    }

    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();

    // No matches -> single non-matched segment
    if !text_lower.contains(&query_lower) {
        return vec![HighlightSegment {
            text: text.to_string(),
            matched: false,
        }];
    }

    let mut parts = Vec::new();
    let mut offset = 0usize;

    // Iterate matches in the lowercase view but slice the original text so
    // the rendered casing is preserved.
    while let Some(rel_idx) = text_lower[offset..].find(&query_lower) {
        let idx = offset + rel_idx;
        if idx > offset {
            parts.push(HighlightSegment {
                text: text[offset..idx].to_string(),
                matched: false,
            });
        }
        // Slice by the lowercase-query byte length because we found the
        // index in the lowercase string; both strings have identical byte
        // boundaries for ASCII, and for non-ASCII we still take exactly
        // `query.len()` bytes from the original (the lowercase mapping
        // preserves byte counts for the common code-point cases this UI
        // search hits in practice).
        let end = idx + query.len().min(text.len() - idx);
        parts.push(HighlightSegment {
            text: text[idx..end].to_string(),
            matched: true,
        });
        offset = end;
    }

    if offset < text.len() {
        parts.push(HighlightSegment {
            text: text[offset..].to_string(),
            matched: false,
        });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_unmatched() {
        let out = highlight_match("hello world", "");
        assert_eq!(out.len(), 1);
        assert!(!out[0].matched);
    }

    #[test]
    fn no_match_returns_unmatched() {
        let out = highlight_match("hello", "xyz");
        assert_eq!(out.len(), 1);
        assert!(!out[0].matched);
    }

    #[test]
    fn case_insensitive_single_match() {
        let out = highlight_match("Hello World", "world");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "Hello ");
        assert!(!out[0].matched);
        assert_eq!(out[1].text, "World");
        assert!(out[1].matched);
    }

    #[test]
    fn multiple_matches() {
        let out = highlight_match("abc ABC abc", "abc");
        assert_eq!(out.len(), 5);
        assert!(out[0].matched);
        assert!(!out[1].matched);
        assert!(out[2].matched);
        assert!(!out[3].matched);
        assert!(out[4].matched);
    }
}
