/// Tool name for FileEditTool.
pub const FILE_EDIT_TOOL_NAME: &str = "Edit";

/// Permission pattern for granting session-level access to the project's .mossen/ folder.
pub const MOSSEN_FOLDER_PERMISSION_PATTERN: &str = "/.mossen/**";

/// Permission pattern for granting session-level access to the global ~/.mossen/ folder.
pub const GLOBAL_MOSSEN_FOLDER_PERMISSION_PATTERN: &str = "~/.mossen/**";

/// Error message when file has been unexpectedly modified.
pub const FILE_UNEXPECTEDLY_MODIFIED_ERROR: &str =
    "File has been unexpectedly modified. Read it again before attempting to write it.";
