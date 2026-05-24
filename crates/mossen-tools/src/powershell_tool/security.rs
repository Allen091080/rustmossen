//! # security — PowerShell security checks
//!
//! Translates `tools/PowerShellTool/powershellSecurity.ts`.
//! Checks for command substitution attacks, dangerous type usage,
//! and other security-sensitive patterns in PowerShell commands.

use regex::Regex;
use std::sync::LazyLock;

use super::clm_types::is_clm_allowed_type;
use super::git_safety::is_git_internal_path_ps;

/// Security check result.
#[derive(Debug, Clone)]
pub enum SecurityCheckResult {
    /// Command passed all security checks.
    Safe,
    /// Command has a security concern.
    Blocked { reason: String },
}

/// Patterns that indicate potentially dangerous constructs.
static DANGEROUS_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // Invoke-Expression (code injection vector)
        (
            Regex::new(r"(?i)\bInvoke-Expression\b").unwrap(),
            "Invoke-Expression can execute arbitrary code",
        ),
        // iex alias
        (
            Regex::new(r"(?i)\biex\b").unwrap(),
            "iex (Invoke-Expression alias) can execute arbitrary code",
        ),
        // DownloadString/DownloadFile (remote code execution)
        (
            Regex::new(r"(?i)\.(DownloadString|DownloadFile|DownloadData)\s*\(").unwrap(),
            "Downloading and executing remote content is dangerous",
        ),
        // Start-Process with -Verb RunAs (privilege escalation)
        (
            Regex::new(r"(?i)\bStart-Process\b[^;]*-Verb\s+RunAs").unwrap(),
            "Start-Process with RunAs verb attempts privilege escalation",
        ),
        // Registry Run keys (persistence mechanism)
        (
            Regex::new(r"(?i)HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run").unwrap(),
            "Writing to registry Run keys enables persistence",
        ),
        (
            Regex::new(r"(?i)HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run").unwrap(),
            "Writing to registry Run keys enables persistence",
        ),
        // Scheduled tasks (persistence)
        (
            Regex::new(r"(?i)\b(Register-ScheduledTask|New-ScheduledTask)\b").unwrap(),
            "Creating scheduled tasks enables persistence",
        ),
        // WMI event subscriptions (persistence)
        (
            Regex::new(r"(?i)\b(Register-WmiEvent|Set-WmiInstance)\b").unwrap(),
            "WMI event subscriptions enable persistence",
        ),
        // Add-Type -TypeDefinition with inline C# (arbitrary code compilation)
        (
            Regex::new(r"(?i)\bAdd-Type\b[^;]*-TypeDefinition\b").unwrap(),
            "Add-Type with TypeDefinition compiles and executes arbitrary code",
        ),
        // [System.Reflection] (bypass security controls)
        (
            Regex::new(r"(?i)\[System\.Reflection").unwrap(),
            "Reflection can bypass security controls",
        ),
        // DllImport / P/Invoke
        (
            Regex::new(r"(?i)\[DllImport").unwrap(),
            "P/Invoke can execute arbitrary native code",
        ),
        // Named pipes (covert communication)
        (
            Regex::new(r"(?i)\bNew-Object\b[^;]*System\.IO\.(Pipes\.)?NamedPipe").unwrap(),
            "Named pipes can be used for covert communication",
        ),
        // Disable security features
        (
            Regex::new(r"(?i)Set-ExecutionPolicy\s+(Bypass|Unrestricted)").unwrap(),
            "Disabling execution policy weakens security",
        ),
        // Credential theft
        (
            Regex::new(r"(?i)\b(Mimikatz|Invoke-Mimikatz|Get-GPPPassword)\b").unwrap(),
            "Credential theft tools detected",
        ),
    ]
});

/// Type literal extraction pattern.
static TYPE_LITERAL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\[\]]+)\]").unwrap());

/// Run all security checks on a PowerShell command.
pub fn check_security(command: &str) -> SecurityCheckResult {
    // Check dangerous patterns
    for (pattern, reason) in DANGEROUS_PATTERNS.iter() {
        if pattern.is_match(command) {
            return SecurityCheckResult::Blocked {
                reason: reason.to_string(),
            };
        }
    }

    // Check type literals against CLM allowlist
    if let Some(result) = check_type_literals(command) {
        return result;
    }

    // Check for git-internal path writes
    if let Some(result) = check_git_internal_writes(command) {
        return result;
    }

    // Check for environment variable manipulation
    if let Some(result) = check_env_manipulation(command) {
        return result;
    }

    SecurityCheckResult::Safe
}

/// Check type literals in the command against the CLM allowlist.
fn check_type_literals(command: &str) -> Option<SecurityCheckResult> {
    for cap in TYPE_LITERAL_RE.captures_iter(command) {
        let type_name = &cap[1];

        // Skip generic type parameters (e.g., List[string])
        if type_name.contains('[') {
            continue;
        }

        // Skip obvious non-type contexts (array indexing, etc.)
        let trimmed = type_name.trim();
        if trimmed.parse::<i64>().is_ok() || trimmed.starts_with('$') {
            continue;
        }

        if !is_clm_allowed_type(trimmed) {
            return Some(SecurityCheckResult::Blocked {
                reason: format!(
                    "Type literal [{}] is not in the Constrained Language Mode allowlist",
                    trimmed
                ),
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/PowerShellTool/powershellSecurity.ts` export.
// ---------------------------------------------------------------------------

/// `powershellSecurity.ts` `powershellCommandIsSafe`.
pub fn powershell_command_is_safe(command: &str) -> &'static str {
    match check_security(command) {
        SecurityCheckResult::Safe => "safe",
        SecurityCheckResult::Blocked { .. } => "ask",
    }
}

/// Check for writes to git-internal paths.
fn check_git_internal_writes(command: &str) -> Option<SecurityCheckResult> {
    // Look for write cmdlets followed by git-internal paths
    let write_patterns = [
        r"(?i)\b(Set-Content|Out-File|Add-Content|New-Item)\b\s+",
        r"(?i)\b(Copy-Item|Move-Item)\b\s+",
    ];

    for pattern_str in &write_patterns {
        let re = Regex::new(pattern_str).ok()?;
        if let Some(m) = re.find(command) {
            let after = &command[m.end()..];
            let path_arg = after.split_whitespace().next().unwrap_or("");
            let cleaned = path_arg.trim_matches(|c| c == '"' || c == '\'');
            if !cleaned.is_empty() && is_git_internal_path_ps(cleaned) {
                return Some(SecurityCheckResult::Blocked {
                    reason: "Writing to git-internal paths is a potential sandbox escape vector"
                        .to_string(),
                });
            }
        }
    }

    None
}

/// Check for dangerous environment variable manipulation.
fn check_env_manipulation(command: &str) -> Option<SecurityCheckResult> {
    let dangerous_env_vars = [
        "PSModulePath",
        "PATH",
        "PATHEXT",
        "COMSPEC",
        "PROCESSOR_ARCHITECTURE",
    ];

    let re = Regex::new(r"(?i)\$env:(\w+)\s*=").ok()?;
    for cap in re.captures_iter(command) {
        let var_name = &cap[1];
        for dangerous in &dangerous_env_vars {
            if var_name.eq_ignore_ascii_case(dangerous) {
                return Some(SecurityCheckResult::Blocked {
                    reason: format!(
                        "Modifying the {} environment variable can compromise security",
                        dangerous
                    ),
                });
            }
        }
    }

    None
}
