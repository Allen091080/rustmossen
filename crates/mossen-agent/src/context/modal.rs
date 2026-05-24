//! Modal context — modal sizing and scroll coordination.
//!
//! Translates: context/modalContext.tsx
//! React context → struct.

/// Modal context — provides available content area size when inside a modal slot.
///
/// Set by FullscreenLayout when rendering content in its `modal` slot.
/// Consumers use this to:
/// - Suppress top-level framing
/// - Size Select pagination to available rows
/// - Reset scroll on tab switch
///
/// None = not inside the modal slot.
#[derive(Debug, Clone, Copy)]
pub struct ModalContext {
    pub rows: usize,
    pub columns: usize,
}

/// Terminal size fallback.
#[derive(Debug, Clone, Copy)]
pub struct TerminalSize {
    pub rows: usize,
    pub columns: usize,
}

/// Get the effective content size — modal size if inside modal, else terminal size.
///
/// Translates: useModalOrTerminalSize()
pub fn modal_or_terminal_size(
    modal: Option<&ModalContext>,
    fallback: TerminalSize,
) -> TerminalSize {
    match modal {
        Some(ctx) => TerminalSize {
            rows: ctx.rows,
            columns: ctx.columns,
        },
        None => fallback,
    }
}

/// Check if we are inside a modal.
///
/// Translates: useIsInsideModal()
pub fn is_inside_modal(modal: Option<&ModalContext>) -> bool {
    modal.is_some()
}

/// Scroll-box handle stored in the modal context. The React port holds a
/// `RefObject<ScrollBoxHandle | null>`; the Rust port uses a stable opaque id
/// shared with the renderer so scroll resets on tab switch work the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollBoxHandle {
    pub id: u64,
}

/// Extended modal context that includes the optional scroll-box handle —
/// mirrors TS `type ModalCtx = { rows, columns, scrollRef }`.
#[derive(Debug, Clone, Copy)]
pub struct ModalContextWithScroll {
    pub rows: usize,
    pub columns: usize,
    pub scroll_ref: Option<ScrollBoxHandle>,
}

/// TS `useIsInsideModal()` — verbatim-named wrapper for `is_inside_modal`.
pub fn use_is_inside_modal(modal: Option<&ModalContextWithScroll>) -> bool {
    modal.is_some()
}

/// TS `useModalOrTerminalSize(fallback)` — verbatim-named wrapper.
pub fn use_modal_or_terminal_size(
    modal: Option<&ModalContextWithScroll>,
    fallback: TerminalSize,
) -> TerminalSize {
    match modal {
        Some(ctx) => TerminalSize {
            rows: ctx.rows,
            columns: ctx.columns,
        },
        None => fallback,
    }
}

/// TS `useModalScrollRef()` — returns the scroll handle if currently inside
/// a modal slot, else `None`.
pub fn use_modal_scroll_ref(modal: Option<&ModalContextWithScroll>) -> Option<ScrollBoxHandle> {
    modal.and_then(|m| m.scroll_ref)
}
