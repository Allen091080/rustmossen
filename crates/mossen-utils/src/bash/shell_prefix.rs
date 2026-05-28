//! Shell prefix command formatting.
//!
//! Translated from `shellPrefix.ts` (28 lines).

use crate::bash::shell_quote;

/// Parses a shell prefix that may contain an executable path and arguments.
///
/// Examples:
/// - "bash" -> quotes as 'bash'
/// - "/usr/bin/bash -c" -> quotes as '/usr/bin/bash' -c
/// - "C:\Program Files\Git\bin\bash.exe -c" -> quotes as 'C:\Program Files\Git\bin\bash.exe' -c
pub fn format_shell_prefix_command(prefix: &str, command: &str) -> String {
    // Split on the last space before a dash to separate executable from arguments
    if let Some(space_before_dash) = prefix.rfind(" -") {
        if space_before_dash > 0 {
            let exec_path = &prefix[..space_before_dash];
            let args = &prefix[space_before_dash + 1..];
            return format!(
                "{} {} {}",
                shell_quote::quote(&[exec_path]),
                args,
                shell_quote::quote(&[command])
            );
        }
    }
    format!(
        "{} {}",
        shell_quote::quote(&[prefix]),
        shell_quote::quote(&[command])
    )
}
