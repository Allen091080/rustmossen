//! # read_only_validation — PowerShell read-only command validation
//!
//! Translates `tools/PowerShellTool/readOnlyValidation.ts`.
//! Validates that commands are read-only (no side effects) for safe auto-approval.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::common_parameters::COMMON_PARAMETERS;
use mossen_utils::string_utils::truncate_chars_with_suffix;

/// Result of read-only validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyResult {
    /// Command is confirmed read-only.
    ReadOnly,
    /// Command may have side effects.
    NotReadOnly { reason: String },
}

/// Known read-only PowerShell cmdlets (lowercase).
static READ_ONLY_CMDLETS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Get-* cmdlets (information retrieval)
        "get-childitem",
        "gci",
        "ls",
        "dir",
        "get-content",
        "gc",
        "cat",
        "type",
        "get-item",
        "gi",
        "get-itemproperty",
        "gp",
        "get-location",
        "gl",
        "pwd",
        "get-process",
        "gps",
        "ps",
        "get-service",
        "gsv",
        "get-date",
        "get-help",
        "help",
        "man",
        "get-command",
        "gcm",
        "get-module",
        "gmo",
        "get-variable",
        "gv",
        "get-alias",
        "gal",
        "get-member",
        "gm",
        "get-history",
        "h",
        "history",
        "get-psdrive",
        "gdr",
        "get-eventlog",
        "gel",
        "get-winevent",
        "get-counter",
        "get-hotfix",
        "get-computerinfo",
        "get-culture",
        "get-uiculture",
        "get-timezone",
        "get-clipboard",
        "gcb",
        "get-filehash",
        "get-acl",
        // Test-* cmdlets (boolean checks)
        "test-path",
        "test-connection",
        "test-netconnection",
        "test-computersecurechannel",
        // Measure/Count
        "measure-object",
        "measure",
        "measure-command",
        // Format/Select/Sort (pipeline operators - read-only)
        "format-table",
        "ft",
        "format-list",
        "fl",
        "format-wide",
        "fw",
        "format-custom",
        "fc",
        "select-object",
        "select",
        "sort-object",
        "sort",
        "where-object",
        "where",
        "?",
        "foreach-object",
        "foreach",
        "%",
        "group-object",
        "group",
        "compare-object",
        "diff",
        "compare",
        "tee-object",
        "tee",
        // String operations
        "select-string",
        "sls",
        "convertfrom-json",
        "convertto-json",
        "convertfrom-csv",
        "convertto-csv",
        "convertfrom-xml",
        "convertto-xml",
        "convertfrom-stringdata",
        // Output (display only, no file write)
        "write-output",
        "echo",
        "write",
        "write-host",
        "write-verbose",
        "write-debug",
        "write-information",
        "write-warning",
        "write-error",
        "out-string",
        "oss",
        "out-host",
        "oh",
        "out-null",
        // Resolve/Split (path operations, read-only)
        "resolve-path",
        "rvpa",
        "split-path",
        "join-path",
        "convert-path",
        "cvpa",
        // Environment reading
        "get-childitem env:",
    ]
    .iter()
    .copied()
    .collect()
});

/// Known read-only external commands.
static READ_ONLY_EXTERNALS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "git status",
        "git log",
        "git diff",
        "git show",
        "git branch",
        "git tag",
        "git remote",
        "git rev-parse",
        "git describe",
        "git ls-files",
        "git ls-tree",
        "git blame",
        "git shortlog",
        "git stash list",
        "git reflog",
        "git config --get",
        "git config --list",
        "git config -l",
        "node --version",
        "node -v",
        "npm --version",
        "npm -v",
        "npm list",
        "npm ls",
        "python --version",
        "python -V",
        "python3 --version",
        "pip list",
        "pip3 list",
        "pip show",
        "pip3 show",
        "rustc --version",
        "cargo --version",
        "dotnet --version",
        "dotnet --list-sdks",
        "dotnet --list-runtimes",
        "java --version",
        "java -version",
        "javac --version",
        "go version",
        "go env",
        "docker --version",
        "docker ps",
        "docker images",
        "kubectl version",
        "kubectl get",
        "terraform --version",
        "terraform plan",
        "az --version",
        "aws --version",
        "which",
        "where.exe",
        "whoami",
        "hostname",
        "ipconfig",
        "systeminfo",
        "ver",
        "winver",
    ]
    .iter()
    .copied()
    .collect()
});

/// Check if a PowerShell command is read-only.
pub fn is_read_only_command(command: &str) -> ReadOnlyResult {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return ReadOnlyResult::ReadOnly;
    }

    // Split on statement separators
    let statements: Vec<&str> = trimmed.split([';', '\n']).collect();

    for stmt in statements {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }

        if !is_single_statement_read_only(s) {
            return ReadOnlyResult::NotReadOnly {
                reason: format!("Statement may have side effects: {}", truncate(s, 60)),
            };
        }
    }

    ReadOnlyResult::ReadOnly
}

/// Check if a single statement is read-only.
fn is_single_statement_read_only(stmt: &str) -> bool {
    let lower = stmt.to_lowercase();
    let first_token = lower.split_whitespace().next().unwrap_or("");

    // Strip leading & call operator
    let effective = if first_token == "&" {
        lower.split_whitespace().nth(1).unwrap_or("")
    } else {
        first_token
    };

    // Check cmdlet allowlist
    if READ_ONLY_CMDLETS.contains(effective) {
        return true;
    }

    // Check external command prefixes
    for prefix in READ_ONLY_EXTERNALS.iter() {
        if lower.starts_with(prefix) {
            return true;
        }
    }

    // Variable assignment without side effects (simple $var = expr)
    if effective.starts_with('$') && lower.contains('=') {
        // Check RHS doesn't call write cmdlets
        if let Some(rhs_start) = lower.find('=') {
            let rhs = &lower[rhs_start + 1..];
            // If RHS is a simple expression or read cmdlet, it's safe
            let rhs_first = rhs.trim().split_whitespace().next().unwrap_or("");
            if READ_ONLY_CMDLETS.contains(rhs_first)
                || rhs_first.starts_with('$')
                || rhs_first.starts_with('"')
                || rhs_first.starts_with('\'')
            {
                return true;
            }
        }
    }

    // Pipeline: check if the pipeline starts with a read-only cmdlet
    // and doesn't pipe to a write cmdlet
    if lower.contains('|') {
        let segments: Vec<&str> = lower.split('|').collect();
        let all_safe = segments.iter().all(|seg| {
            let seg_first = seg.trim().split_whitespace().next().unwrap_or("");
            READ_ONLY_CMDLETS.contains(seg_first)
        });
        if all_safe {
            return true;
        }
    }

    // Check for output redirection (not read-only)
    if lower.contains('>') {
        return false;
    }

    false
}

/// Truncate a string for display.
fn truncate(s: &str, max: usize) -> String {
    truncate_chars_with_suffix(s, max, "...")
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/PowerShellTool/readOnlyValidation.ts` exports.
// ---------------------------------------------------------------------------

/// `readOnlyValidation.ts` `CommandConfig`.
#[derive(Debug, Clone)]
pub struct CommandConfig {
    pub canonical_name: &'static str,
    pub cwd_changing: bool,
    pub safe_output: bool,
    pub disallowed_args: &'static [&'static str],
}

/// `readOnlyValidation.ts` `CMDLET_ALLOWLIST` — minimal canonical entries.
pub fn cmdlet_allowlist(name: &str) -> Option<CommandConfig> {
    match name.to_lowercase().as_str() {
        "get-childitem" | "gci" | "ls" | "dir" => Some(CommandConfig {
            canonical_name: "Get-ChildItem",
            cwd_changing: false,
            safe_output: true,
            disallowed_args: &[],
        }),
        "get-content" | "cat" | "type" | "gc" => Some(CommandConfig {
            canonical_name: "Get-Content",
            cwd_changing: false,
            safe_output: true,
            disallowed_args: &[],
        }),
        "set-location" | "cd" | "chdir" | "sl" => Some(CommandConfig {
            canonical_name: "Set-Location",
            cwd_changing: true,
            safe_output: false,
            disallowed_args: &[],
        }),
        "write-output" | "echo" | "write" => Some(CommandConfig {
            canonical_name: "Write-Output",
            cwd_changing: false,
            safe_output: true,
            disallowed_args: &[],
        }),
        "out-host" | "format-table" | "format-list" | "ft" | "fl" => Some(CommandConfig {
            canonical_name: "Out-Host",
            cwd_changing: false,
            safe_output: true,
            disallowed_args: &[],
        }),
        _ => None,
    }
}

/// `readOnlyValidation.ts` `resolveToCanonical` — map an alias to its
/// canonical cmdlet name. Returns the input unchanged when unknown.
pub fn resolve_to_canonical(name: &str) -> String {
    cmdlet_allowlist(name)
        .map(|c| c.canonical_name.to_string())
        .unwrap_or_else(|| name.to_string())
}

/// `readOnlyValidation.ts` `argLeaksValue` — true if an argument looks like
/// it routes/exfiltrates command output (e.g. `-OutFile`, redirections).
pub fn arg_leaks_value(arg: &str) -> bool {
    let lower = arg.to_lowercase();
    matches!(
        lower.as_str(),
        "-outfile"
            | "-outvariable"
            | "-out"
            | "-passthru"
            | "-tee-object"
            | "-redirectstandardoutput"
            | "-encodedcommand"
    )
}

/// `readOnlyValidation.ts` `isCwdChangingCmdlet`.
pub fn is_cwd_changing_cmdlet(name: &str) -> bool {
    cmdlet_allowlist(name)
        .map(|c| c.cwd_changing)
        .unwrap_or(false)
}

/// `readOnlyValidation.ts` `isSafeOutputCommand`.
pub fn is_safe_output_command(name: &str) -> bool {
    cmdlet_allowlist(name)
        .map(|c| c.safe_output)
        .unwrap_or(false)
}

/// `readOnlyValidation.ts` `isAllowlistedPipelineTail` — pipelines that end
/// in safe output sinks (`Out-Host`, `Format-*`).
pub fn is_allowlisted_pipeline_tail(name: &str) -> bool {
    matches!(
        resolve_to_canonical(name).as_str(),
        "Out-Host" | "Format-Table" | "Format-List" | "Write-Output"
    )
}

/// Parsed PowerShell statement skeleton used by `isProvablySafeStatement`.
#[derive(Debug, Clone)]
pub struct ParsedStatement {
    pub command: String,
    pub args: Vec<String>,
    pub pipeline: Vec<String>,
}

/// `readOnlyValidation.ts` `isProvablySafeStatement`.
pub fn is_provably_safe_statement(stmt: &ParsedStatement) -> bool {
    if cmdlet_allowlist(&stmt.command).is_none() {
        return false;
    }
    for a in &stmt.args {
        if arg_leaks_value(a) {
            return false;
        }
    }
    for cmd in &stmt.pipeline {
        if !is_allowlisted_pipeline_tail(cmd) && cmdlet_allowlist(cmd).is_none() {
            return false;
        }
    }
    true
}

/// `readOnlyValidation.ts` `hasSyncSecurityConcerns` — quick syntactic gate.
pub fn has_sync_security_concerns(command: &str) -> bool {
    let lower = command.to_lowercase();
    lower.contains("invoke-expression")
        || lower.contains(" iex ")
        || lower.starts_with("iex ")
        || lower.contains("download")
        || lower.contains("invoke-webrequest")
        || lower.contains("invoke-restmethod")
        || lower.contains("start-process")
        || lower.contains("new-object net.webclient")
}

/// `readOnlyValidation.ts` `isAllowlistedCommand` — true if every parsed
/// statement is `isProvablySafeStatement`.
pub fn is_allowlisted_command(statements: &[ParsedStatement]) -> bool {
    statements.iter().all(is_provably_safe_statement)
}
