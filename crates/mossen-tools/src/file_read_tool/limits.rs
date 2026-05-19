use std::sync::LazyLock;

/// Default maximum output tokens for file reads.
pub const DEFAULT_MAX_OUTPUT_TOKENS: usize = 25000;

/// Default maximum output size in bytes (256 KB).
pub const MAX_OUTPUT_SIZE: usize = 256 * 1024;

/// File reading limits configuration.
#[derive(Debug, Clone)]
pub struct FileReadingLimits {
    pub max_tokens: usize,
    pub max_size_bytes: usize,
    pub include_max_size_in_prompt: Option<bool>,
    pub targeted_range_nudge: Option<bool>,
}

/// Get environment variable override for max output tokens.
fn get_env_max_tokens() -> Option<usize> {
    std::env::var("MOSSEN_CODE_FILE_READ_MAX_OUTPUT_TOKENS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0)
}

/// Get the default file reading limits.
/// Memoized so the value is fixed at first call.
pub fn get_default_file_reading_limits() -> &'static FileReadingLimits {
    static LIMITS: LazyLock<FileReadingLimits> = LazyLock::new(|| {
        let max_size_bytes = MAX_OUTPUT_SIZE;
        let env_max_tokens = get_env_max_tokens();
        let max_tokens = env_max_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS);

        FileReadingLimits {
            max_size_bytes,
            max_tokens,
            include_max_size_in_prompt: None,
            targeted_range_nudge: None,
        }
    });
    &LIMITS
}
