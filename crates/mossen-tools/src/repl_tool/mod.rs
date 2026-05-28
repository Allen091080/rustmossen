//! REPLTool helpers.
//!
//! Translates `tools/REPLTool/*.ts`.

pub mod prompt;

use mossen_agent::tool_registry::Tool;

/// `REPLTool/primitiveTools.ts` `getReplPrimitiveTools` — primitive tools
/// hidden from direct model use when REPL mode is on but still accessible
/// inside the REPL VM context.
///
/// Exported so display-side code (collapseReadSearch, renderers) can
/// classify/render virtual messages for these tools even when they're
/// absent from the filtered execution tools list.
///
/// Referenced directly rather than via `all_p0_tools()` because Glob/Grep
/// can be excluded by feature flags but are still primitive REPL tools.
pub fn get_repl_primitive_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(crate::file_read::FileInspector),
        Box::new(crate::file_write::FileComposer),
        Box::new(crate::file_edit::SourcePatcher),
        Box::new(crate::glob::PathDiscoverer),
        Box::new(crate::grep::ContentScanner),
        Box::new(crate::bash::ShellExecutor),
        Box::new(crate::notebook_edit::NotebookPatcher),
        Box::new(crate::agent::SubagentLauncher),
    ]
}

/// Alias matching the TS export name `getReplPrimitiveTools`.
#[allow(non_snake_case)]
pub fn getReplPrimitiveTools() -> Vec<Box<dyn Tool>> {
    get_repl_primitive_tools()
}
