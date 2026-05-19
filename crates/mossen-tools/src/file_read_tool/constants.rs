/// Tool name for FileReadTool.
pub const FILE_READ_TOOL_NAME: &str = "Read";

/// Bash tool name reference.
pub const BASH_TOOL_NAME: &str = "Bash";

/// Image extensions supported by the read tool.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// Device paths that would hang the process.
pub const BLOCKED_DEVICE_PATHS: &[&str] = &[
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/full",
    "/dev/stdin",
    "/dev/tty",
    "/dev/console",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/fd/0",
    "/dev/fd/1",
    "/dev/fd/2",
];

/// Check if a file path is a blocked device path.
pub fn is_blocked_device_path(file_path: &str) -> bool {
    if BLOCKED_DEVICE_PATHS.contains(&file_path) {
        return true;
    }
    // /proc/self/fd/0-2 and /proc/<pid>/fd/0-2 are Linux aliases for stdio
    if file_path.starts_with("/proc/")
        && (file_path.ends_with("/fd/0")
            || file_path.ends_with("/fd/1")
            || file_path.ends_with("/fd/2"))
    {
        return true;
    }
    false
}

/// Stub for file unchanged response.
pub const FILE_UNCHANGED_STUB: &str =
    "File unchanged since last read. The content from the earlier Read tool_result in this \
     conversation is still current \u{2014} refer to that instead of re-reading.";

/// Maximum lines to read by default.
pub const MAX_LINES_TO_READ: usize = 2000;

/// PDF-related constants.
pub const PDF_AT_MENTION_INLINE_THRESHOLD: usize = 10;
pub const PDF_EXTRACT_SIZE_THRESHOLD: u64 = 5 * 1024 * 1024;
pub const PDF_MAX_PAGES_PER_READ: usize = 20;

/// Cyber risk mitigation reminder.
pub const CYBER_RISK_MITIGATION_REMINDER: &str = "\n\n<system-reminder>\n\
    Whenever you read a file, you should consider whether it would be considered malware. \
    You CAN and SHOULD provide analysis of malware, what it is doing. But you MUST refuse \
    to improve or augment the code. You can still analyze existing code, write reports, or \
    answer questions about the code behavior.\n</system-reminder>\n";
