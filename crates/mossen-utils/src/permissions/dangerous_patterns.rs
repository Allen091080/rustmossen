//! Dangerous shell-tool allow-rule pattern lists.
//!
//! Translates `utils/permissions/dangerousPatterns.ts`.
//!
//! An allow rule like `Bash(python:*)` or `PowerShell(node:*)` lets the model
//! run arbitrary code via that interpreter, bypassing the auto-mode classifier.

/// Cross-platform code-execution entry points present on both Unix and Windows.
pub const CROSS_PLATFORM_CODE_EXEC: &[&str] = &[
    // Interpreters
    "python",
    "python3",
    "python2",
    "node",
    "deno",
    "tsx",
    "ruby",
    "perl",
    "php",
    "lua",
    // Package runners
    "npx",
    "bunx",
    "npm run",
    "yarn run",
    "pnpm run",
    "bun run",
    // Shells reachable from both (Git Bash / WSL on Windows, native on Unix)
    "bash",
    "sh",
    // Remote arbitrary-command wrapper (native OpenSSH on Win10+)
    "ssh",
];

/// Returns the dangerous Bash patterns list.
/// Includes ant-only patterns when `is_ant_user` is true.
pub fn dangerous_bash_patterns(is_ant_user: bool) -> Vec<&'static str> {
    let mut patterns: Vec<&str> = CROSS_PLATFORM_CODE_EXEC.to_vec();
    patterns.extend_from_slice(&[
        "zsh",
        "fish",
        "eval",
        "exec",
        "env",
        "xargs",
        "sudo",
        // Network/exfil & cloud writes
        "gh",
        "gh api",
        "curl",
        "wget",
        "git",
        "kubectl",
        "aws",
        "gcloud",
        "gsutil",
    ]);
    if is_ant_user {
        patterns.extend_from_slice(&["fa run", "coo"]);
    }
    patterns
}
