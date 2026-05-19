//! CursorDeclContext component (cursor_declaration_context.ts/tsx).
//! Manages declared cursor positions from children.

#[derive(Debug, Clone)]
pub struct CursorDeclContextState {
    pub active: bool,
}
impl CursorDeclContextState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for CursorDeclContextState { fn default() -> Self { Self::new() } }

/// Declaration of a cursor's intended position and style.
#[derive(Debug, Clone, Default)]
pub struct CursorDeclaration {
    pub x: u16,
    pub y: u16,
    pub visible: bool,
    pub style: u8, // 0=block, 1=underline, 2=bar
    pub blinking: bool,
}

/// Setter signature for the cursor declaration context.
pub type CursorDeclarationSetter = std::sync::Arc<dyn Fn(CursorDeclaration) + Send + Sync>;
