//! IDE at-mention hook (useIdeAtMentioned.ts).
//!
//! Tracks when the IDE sends an @-mention for a file or symbol.

/// State for IDE at-mention tracking.
#[derive(Debug, Clone)]
pub struct IdeAtMentionedState {
    pub mentions: Vec<AtMention>,
    pub last_mention: Option<AtMention>,
}

#[derive(Debug, Clone)]
pub struct AtMention {
    pub text: String,
    pub file_path: Option<String>,
    pub symbol: Option<String>,
    pub line_range: Option<(u32, u32)>,
    pub timestamp: u64,
}

impl IdeAtMentionedState {
    pub fn new() -> Self {
        Self {
            mentions: Vec::new(),
            last_mention: None,
        }
    }

    /// Add a new mention from the IDE.
    pub fn add_mention(&mut self, mention: AtMention) {
        self.last_mention = Some(mention.clone());
        self.mentions.push(mention);
    }

    /// Clear all mentions.
    pub fn clear(&mut self) {
        self.mentions.clear();
        self.last_mention = None;
    }

    /// Get pending mentions and clear them.
    pub fn take_mentions(&mut self) -> Vec<AtMention> {
        let taken = std::mem::take(&mut self.mentions);
        self.last_mention = None;
        taken
    }

    pub fn has_mentions(&self) -> bool {
        !self.mentions.is_empty()
    }
}

impl Default for IdeAtMentionedState {
    fn default() -> Self {
        Self::new()
    }
}
