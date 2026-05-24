//! # destructive_command_warning — PowerShell destructive command detection
//!
//! Translates `tools/PowerShellTool/destructiveCommandWarning.ts`.

use regex::Regex;
use std::sync::LazyLock;

struct DestructivePattern {
    pattern: &'static str,
    warning: &'static str,
}

static DESTRUCTIVE_PATTERNS: &[DestructivePattern] = &[
    DestructivePattern {
        pattern: r"(?i)(?:^|[|;&\n({])\s*(Remove-Item|rm|del|rd|rmdir|ri)\b[^|;&\n}]*-Recurse\b[^|;&\n}]*-Force\b",
        warning: "Note: may recursively force-remove files",
    },
    DestructivePattern {
        pattern: r"(?i)(?:^|[|;&\n({])\s*(Remove-Item|rm|del|rd|rmdir|ri)\b[^|;&\n}]*-Force\b[^|;&\n}]*-Recurse\b",
        warning: "Note: may recursively force-remove files",
    },
    DestructivePattern {
        pattern: r"(?i)(?:^|[|;&\n({])\s*(Remove-Item|rm|del|rd|rmdir|ri)\b[^|;&\n}]*-Recurse\b",
        warning: "Note: may recursively remove files",
    },
    DestructivePattern {
        pattern: r"(?i)(?:^|[|;&\n({])\s*(Remove-Item|rm|del|rd|rmdir|ri)\b[^|;&\n}]*-Force\b",
        warning: "Note: may force-remove files",
    },
    DestructivePattern {
        pattern: r"(?i)\bClear-Content\b[^|;&\n]*\*",
        warning: "Note: may clear content of multiple files",
    },
    DestructivePattern {
        pattern: r"(?i)\bFormat-Volume\b",
        warning: "Note: may format a disk volume",
    },
    DestructivePattern {
        pattern: r"(?i)\bClear-Disk\b",
        warning: "Note: may clear a disk",
    },
    DestructivePattern {
        pattern: r"(?i)\bgit\s+reset\s+--hard\b",
        warning: "Note: may discard uncommitted changes",
    },
    DestructivePattern {
        pattern: r"(?i)\bgit\s+push\b[^|;&\n]*\s+(--force|--force-with-lease|-f)\b",
        warning: "Note: may overwrite remote history",
    },
    DestructivePattern {
        pattern: r"(?i)\bgit\s+clean\b(?![^|;&\n]*(?:-[a-zA-Z]*n|--dry-run))[^|;&\n]*-[a-zA-Z]*f",
        warning: "Note: may permanently delete untracked files",
    },
    DestructivePattern {
        pattern: r"(?i)\bgit\s+stash\s+(drop|clear)\b",
        warning: "Note: may permanently remove stashed changes",
    },
    DestructivePattern {
        pattern: r"(?i)\b(DROP|TRUNCATE)\s+(TABLE|DATABASE|SCHEMA)\b",
        warning: "Note: may drop or truncate database objects",
    },
    DestructivePattern {
        pattern: r"(?i)\bStop-Computer\b",
        warning: "Note: will shut down the computer",
    },
    DestructivePattern {
        pattern: r"(?i)\bRestart-Computer\b",
        warning: "Note: will restart the computer",
    },
    DestructivePattern {
        pattern: r"(?i)\bClear-RecycleBin\b",
        warning: "Note: permanently deletes recycled files",
    },
];

static COMPILED_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    DESTRUCTIVE_PATTERNS
        .iter()
        .filter_map(|dp| Regex::new(dp.pattern).ok().map(|re| (re, dp.warning)))
        .collect()
});

/// Checks if a PowerShell command matches known destructive patterns.
/// Returns a human-readable warning string, or None if no destructive pattern is detected.
pub fn get_destructive_command_warning(command: &str) -> Option<&'static str> {
    for (pattern, warning) in COMPILED_PATTERNS.iter() {
        if pattern.is_match(command) {
            return Some(warning);
        }
    }
    None
}
