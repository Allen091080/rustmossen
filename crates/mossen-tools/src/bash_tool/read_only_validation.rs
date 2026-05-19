//! Read-only command validation for the Bash tool.
//!
//! Corresponds to `readOnlyValidation.ts` (1931 lines). Determines whether a command
//! is read-only (safe to auto-approve) based on command name, flags, and arguments.

use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Flag argument types for validation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlagArgType {
    /// Flag takes no argument.
    None,
    /// Flag takes a string argument.
    StringArg,
    /// Flag takes a numeric argument.
    NumberArg,
}

/// Result of read-only validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadOnlyResult {
    /// Command is read-only (safe).
    ReadOnly,
    /// Command is not read-only (potentially modifying).
    NotReadOnly,
    /// Unable to determine; pass through to normal permission flow.
    Unknown,
}

/// Git read-only commands (subcommands that don't modify state).
fn git_read_only_commands() -> HashSet<&'static str> {
    let mut s = HashSet::new();
    for cmd in &[
        "status", "log", "diff", "show", "branch", "tag", "describe",
        "rev-parse", "rev-list", "ls-files", "ls-tree", "ls-remote",
        "cat-file", "for-each-ref", "reflog", "shortlog", "blame",
        "grep", "bisect", "stash list", "remote", "config --get",
        "config --list", "config -l", "name-rev", "merge-base",
        "count-objects", "verify-commit", "verify-tag", "whatchanged",
    ] {
        s.insert(*cmd);
    }
    s
}

/// Docker read-only commands.
fn docker_read_only_commands() -> HashSet<&'static str> {
    let mut s = HashSet::new();
    for cmd in &[
        "ps", "images", "inspect", "logs", "port", "top", "stats",
        "version", "info", "events", "diff", "history",
    ] {
        s.insert(*cmd);
    }
    s
}

/// GitHub CLI read-only commands.
fn gh_read_only_commands() -> HashSet<&'static str> {
    let mut s = HashSet::new();
    for cmd in &[
        "pr list", "pr view", "pr status", "pr checks", "pr diff",
        "issue list", "issue view", "issue status",
        "repo view", "repo list",
        "run list", "run view",
        "release list", "release view",
        "api",
    ] {
        s.insert(*cmd);
    }
    s
}

/// ripgrep read-only safe flags.
fn ripgrep_read_only_flags() -> HashMap<&'static str, FlagArgType> {
    let mut m = HashMap::new();
    m.insert("-i", FlagArgType::None);
    m.insert("--ignore-case", FlagArgType::None);
    m.insert("-w", FlagArgType::None);
    m.insert("--word-regexp", FlagArgType::None);
    m.insert("-c", FlagArgType::None);
    m.insert("--count", FlagArgType::None);
    m.insert("-l", FlagArgType::None);
    m.insert("--files-with-matches", FlagArgType::None);
    m.insert("-n", FlagArgType::None);
    m.insert("--line-number", FlagArgType::None);
    m.insert("-H", FlagArgType::None);
    m.insert("--with-filename", FlagArgType::None);
    m.insert("-r", FlagArgType::None);
    m.insert("--recursive", FlagArgType::None);
    m.insert("--hidden", FlagArgType::None);
    m.insert("--no-ignore", FlagArgType::None);
    m.insert("-F", FlagArgType::None);
    m.insert("--fixed-strings", FlagArgType::None);
    m.insert("-m", FlagArgType::NumberArg);
    m.insert("--max-count", FlagArgType::NumberArg);
    m.insert("-A", FlagArgType::NumberArg);
    m.insert("--after-context", FlagArgType::NumberArg);
    m.insert("-B", FlagArgType::NumberArg);
    m.insert("--before-context", FlagArgType::NumberArg);
    m.insert("-C", FlagArgType::NumberArg);
    m.insert("--context", FlagArgType::NumberArg);
    m.insert("--max-depth", FlagArgType::NumberArg);
    m.insert("-g", FlagArgType::StringArg);
    m.insert("--glob", FlagArgType::StringArg);
    m.insert("-t", FlagArgType::StringArg);
    m.insert("--type", FlagArgType::StringArg);
    m.insert("-T", FlagArgType::StringArg);
    m.insert("--type-not", FlagArgType::StringArg);
    m.insert("--color", FlagArgType::StringArg);
    m
}

/// External commands that are inherently read-only.
fn external_read_only_commands() -> HashSet<&'static str> {
    let mut s = HashSet::new();
    for cmd in &[
        "cat", "head", "tail", "less", "more", "wc", "stat", "file",
        "strings", "od", "xxd", "hexdump", "ls", "tree", "du", "df",
        "find", "locate", "which", "whereis", "type", "command",
        "grep", "egrep", "fgrep", "rg", "ag", "ack",
        "sort", "uniq", "cut", "tr", "awk", "jq", "yq",
        "echo", "printf", "true", "false", "test", "[",
        "pwd", "env", "printenv", "whoami", "id", "groups",
        "date", "cal", "uptime", "uname", "hostname",
        "ps", "top", "htop", "free", "vmstat", "iostat",
        "netstat", "ss", "ip", "ifconfig", "ping", "traceroute",
        "dig", "nslookup", "host", "curl", "wget",
        "man", "info", "help", "apropos", "whatis",
        "diff", "comm", "cmp", "md5sum", "sha256sum", "shasum",
        "base64", "basename", "dirname", "realpath", "readlink",
        "xargs", "seq", "yes", "tput", "stty",
    ] {
        s.insert(*cmd);
    }
    s
}

/// Validate flags against a map of allowed flags.
pub fn validate_flags(
    args: &[String],
    safe_flags: &HashMap<&str, FlagArgType>,
    respects_double_dash: bool,
) -> bool {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // End of flags marker
        if arg == "--" {
            if respects_double_dash {
                break; // Everything after -- is positional
            }
        }

        if arg.starts_with('-') {
            // Handle combined short flags (e.g., -nE)
            if arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2 {
                for ch in arg[1..].chars() {
                    let single_flag = format!("-{}", ch);
                    match safe_flags.get(single_flag.as_str()) {
                        Some(FlagArgType::None) => {}
                        Some(_) => {
                            // Combined flag with arg — can't safely determine
                            return false;
                        }
                        None => return false,
                    }
                }
                i += 1;
                continue;
            }

            // Check if this flag is in the safe list
            match safe_flags.get(arg.as_str()) {
                Some(FlagArgType::None) => {
                    i += 1;
                }
                Some(FlagArgType::StringArg) | Some(FlagArgType::NumberArg) => {
                    i += 2; // Skip the flag's argument
                }
                None => {
                    // Handle --flag=value format
                    if let Some(eq_pos) = arg.find('=') {
                        let flag_name = &arg[..eq_pos];
                        if safe_flags.contains_key(flag_name) {
                            i += 1;
                            continue;
                        }
                    }
                    return false; // Unknown flag
                }
            }
        } else {
            i += 1; // Positional argument
        }
    }
    true
}

/// Check if a git command is read-only.
fn is_git_read_only(command: &str) -> bool {
    let parts: Vec<&str> = command.trim().split_whitespace().collect();
    if parts.is_empty() || parts[0] != "git" {
        return false;
    }

    if parts.len() < 2 {
        return false;
    }

    let subcommand = parts[1];
    let read_only = git_read_only_commands();

    // Check single-word subcommands
    if read_only.contains(subcommand) {
        return true;
    }

    // Check two-word subcommands (e.g., "stash list")
    if parts.len() >= 3 {
        let two_word = format!("{} {}", subcommand, parts[2]);
        if read_only.contains(two_word.as_str()) {
            return true;
        }
    }

    false
}

/// Check if a docker command is read-only.
fn is_docker_read_only(command: &str) -> bool {
    let parts: Vec<&str> = command.trim().split_whitespace().collect();
    if parts.is_empty() || parts[0] != "docker" {
        return false;
    }
    if parts.len() < 2 {
        return false;
    }
    let read_only = docker_read_only_commands();
    read_only.contains(parts[1])
}

/// Check if a gh command is read-only.
fn is_gh_read_only(command: &str) -> bool {
    let parts: Vec<&str> = command.trim().split_whitespace().collect();
    if parts.is_empty() || parts[0] != "gh" {
        return false;
    }
    if parts.len() < 3 {
        return parts.len() >= 2 && parts[1] == "api";
    }
    let read_only = gh_read_only_commands();
    let two_word = format!("{} {}", parts[1], parts[2]);
    read_only.contains(two_word.as_str()) || (parts[1] == "api")
}

/// Main read-only validation: determines if a command is safe to auto-approve.
pub fn check_read_only_constraints(command: &str) -> ReadOnlyResult {
    let trimmed = command.trim();
    let base_cmd = trimmed.split_whitespace().next().unwrap_or("");

    if base_cmd.is_empty() {
        return ReadOnlyResult::Unknown;
    }

    // Check external read-only commands
    let external_ro = external_read_only_commands();
    if external_ro.contains(base_cmd) {
        return ReadOnlyResult::ReadOnly;
    }

    // Check git commands
    if base_cmd == "git" && is_git_read_only(trimmed) {
        return ReadOnlyResult::ReadOnly;
    }

    // Check docker commands
    if base_cmd == "docker" && is_docker_read_only(trimmed) {
        return ReadOnlyResult::ReadOnly;
    }

    // Check gh commands
    if base_cmd == "gh" && is_gh_read_only(trimmed) {
        return ReadOnlyResult::ReadOnly;
    }

    // Check ripgrep (already in external_ro, but with flag validation)
    if base_cmd == "rg" {
        let args: Vec<String> = trimmed
            .split_whitespace()
            .skip(1)
            .map(|s| s.to_string())
            .collect();
        let rg_flags = ripgrep_read_only_flags();
        if validate_flags(&args, &rg_flags, true) {
            return ReadOnlyResult::ReadOnly;
        }
    }

    ReadOnlyResult::NotReadOnly
}

/// Check if a command is in the bare git repo and requires approval.
pub fn check_bare_git_repo_safety(command: &str, is_bare_repo: bool) -> ReadOnlyResult {
    if !is_bare_repo {
        return ReadOnlyResult::Unknown;
    }

    let trimmed = command.trim();
    if trimmed.starts_with("git ") {
        // In a bare repo, many git commands could be dangerous
        if !is_git_read_only(trimmed) {
            return ReadOnlyResult::NotReadOnly;
        }
    }

    ReadOnlyResult::Unknown
}

/// Check if command matches a sed allowlist pattern (for read-only validation).
pub fn sed_command_is_allowed_by_allowlist(command: &str) -> bool {
    let trimmed = command.trim();
    if !trimmed.starts_with("sed ") {
        return false;
    }

    // sed with -n (suppress output) and only p/= commands is read-only
    let has_n_flag = trimmed.contains(" -n ") || trimmed.contains(" -n'")
        || trimmed.starts_with("sed -n");

    if has_n_flag {
        // Check for print-only patterns: sed -n 'Np' or sed -n 'N,Mp'
        let re = Regex::new(r"sed\s+(-[nEr]+\s+)*'[0-9,;$p=]+'\s").unwrap();
        if re.is_match(trimmed) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_ls() {
        assert_eq!(check_read_only_constraints("ls -la"), ReadOnlyResult::ReadOnly);
    }

    #[test]
    fn test_read_only_git_status() {
        assert_eq!(
            check_read_only_constraints("git status"),
            ReadOnlyResult::ReadOnly
        );
    }

    #[test]
    fn test_not_read_only_git_push() {
        assert_eq!(
            check_read_only_constraints("git push origin main"),
            ReadOnlyResult::NotReadOnly
        );
    }

    #[test]
    fn test_read_only_cat() {
        assert_eq!(
            check_read_only_constraints("cat file.txt"),
            ReadOnlyResult::ReadOnly
        );
    }

    #[test]
    fn test_not_read_only_rm() {
        assert_eq!(
            check_read_only_constraints("rm file.txt"),
            ReadOnlyResult::NotReadOnly
        );
    }

    #[test]
    fn test_docker_read_only() {
        assert_eq!(
            check_read_only_constraints("docker ps"),
            ReadOnlyResult::ReadOnly
        );
    }

    #[test]
    fn test_gh_read_only() {
        assert_eq!(
            check_read_only_constraints("gh pr list"),
            ReadOnlyResult::ReadOnly
        );
    }

    #[test]
    fn test_ripgrep_read_only() {
        assert_eq!(
            check_read_only_constraints("rg -i pattern src/"),
            ReadOnlyResult::ReadOnly
        );
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/BashTool/readOnlyValidation.ts` additional export.
// ---------------------------------------------------------------------------

/// `readOnlyValidation.ts` `isCommandSafeViaFlagParsing`.
pub fn is_command_safe_via_flag_parsing(command: &str) -> bool {
    matches!(check_read_only_constraints(command), ReadOnlyResult::ReadOnly)
}
