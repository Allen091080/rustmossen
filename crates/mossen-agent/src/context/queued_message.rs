//! Queued message context — layout context for queued messages.
//!
//! Translates: context/QueuedMessageContext.tsx
//! React context → struct.

/// Queued message layout information.
#[derive(Debug, Clone)]
pub struct QueuedMessageContext {
    pub is_queued: bool,
    pub is_first: bool,
    /// Width reduction for container padding (e.g., 4 for paddingX=2).
    pub padding_width: usize,
}

const PADDING_X: usize = 2;

impl QueuedMessageContext {
    /// Create a new queued message context.
    pub fn new(is_first: bool, use_brief_layout: bool) -> Self {
        let padding = if use_brief_layout { 0 } else { PADDING_X };
        Self {
            is_queued: true,
            is_first,
            padding_width: padding * 2,
        }
    }
}

/// Snapshot of the queued-message context. Mirrors React `useQueuedMessage()`.
pub fn use_queued_message(ctx: &QueuedMessageContext) -> QueuedMessageContext {
    ctx.clone()
}

/// Provider entry-point — initialises a fresh context. Mirrors
/// `QueuedMessageProvider` (a React provider component in TS).
pub fn queued_message_provider() -> QueuedMessageContext {
    QueuedMessageContext {
        is_queued: false,
        is_first: false,
        padding_width: 0,
    }
}
