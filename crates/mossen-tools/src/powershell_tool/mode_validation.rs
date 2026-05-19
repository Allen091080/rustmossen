//! # mode_validation — PowerShell mode-based command validation
//!
//! Translates `tools/PowerShellTool/modeValidation.ts`.
//! Validates commands based on the current permission mode (plan, read-only, etc.).

use regex::Regex;
use std::sync::LazyLock;

/// Validation result from mode checking.
#[derive(Debug, Clone)]
pub enum ModeValidationResult {
    /// Command is allowed in this mode.
    Allowed,
    /// Command is blocked with reason.
    Blocked { reason: String },
}

/// Commands that are always safe regardless of mode (read-only operations).
static ALWAYS_SAFE_COMMANDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)^\s*(Get-ChildItem|ls|dir|gci)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Content|cat|type|gc)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Item|gi)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-ItemProperty|gp)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Location|pwd|gl)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Process|ps|gps)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Service|gsv)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Date)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Help|help|man)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Command|gcm)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Get-Module|gmo)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Test-Path)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Test-Connection)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Select-String|sls)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Measure-Object|measure)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Sort-Object|sort)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Where-Object|where|\?)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Select-Object|select)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Format-Table|ft)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Format-List|fl)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Write-Output|echo|write)\b").unwrap(),
        Regex::new(r"(?i)^\s*(Write-Host)\b").unwrap(),
        Regex::new(r"(?i)^\s*\$").unwrap(), // Variable assignments/reads
    ]
});

/// Commands blocked in plan/read-only mode.
static WRITE_COMMANDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)\b(New-Item|ni|mkdir)\b").unwrap(),
        Regex::new(r"(?i)\b(Remove-Item|rm|del|rd|rmdir|ri)\b").unwrap(),
        Regex::new(r"(?i)\b(Set-Content|sc)\b").unwrap(),
        Regex::new(r"(?i)\b(Add-Content|ac)\b").unwrap(),
        Regex::new(r"(?i)\b(Out-File)\b").unwrap(),
        Regex::new(r"(?i)\b(Copy-Item|cp|copy|cpi)\b").unwrap(),
        Regex::new(r"(?i)\b(Move-Item|mv|move|mi)\b").unwrap(),
        Regex::new(r"(?i)\b(Rename-Item|ren|rni)\b").unwrap(),
        Regex::new(r"(?i)\b(Clear-Content|clc)\b").unwrap(),
        Regex::new(r"(?i)\b(Set-ItemProperty|sp)\b").unwrap(),
        Regex::new(r"(?i)\b(New-ItemProperty)\b").unwrap(),
        Regex::new(r"(?i)\b(Remove-ItemProperty|rp)\b").unwrap(),
        Regex::new(r"(?i)\b(Start-Process|saps|start)\b").unwrap(),
        Regex::new(r"(?i)\b(Stop-Process|spps|kill)\b").unwrap(),
        Regex::new(r"(?i)\b(Invoke-WebRequest|iwr|curl|wget)\b").unwrap(),
        Regex::new(r"(?i)\b(Invoke-RestMethod|irm)\b").unwrap(),
        Regex::new(r"(?i)\bgit\s+(add|commit|push|merge|rebase|reset|checkout|branch\s+-[dD])\b").unwrap(),
        Regex::new(r"(?i)\bnpm\s+(install|uninstall|update|publish|run)\b").unwrap(),
        Regex::new(r"(?i)\b(pip|pip3)\s+(install|uninstall)\b").unwrap(),
        Regex::new(r"(?i)\b(docker|podman)\s+(run|build|push|pull|rm|rmi|stop|kill)\b").unwrap(),
    ]
});

/// `modeValidation.ts` `PowerShellCommand` — minimal cmd descriptor accepted
/// by `is_symlink_creating_command` (mirrors the TS anonymous arg shape).
#[derive(Debug, Clone)]
pub struct PowerShellCommand {
    pub name: String,
    pub args: Vec<String>,
}

/// Unicode dash chars recognized by PowerShell's tokenizer as parameter
/// markers (`-`, en-dash, em-dash, horizontal-bar). Plus forward-slash, which
/// PS 5.1 accepts as a parameter prefix.
const PS_TOKENIZER_DASH_CHARS: &[char] = &['-', '\u{2013}', '\u{2014}', '\u{2015}'];

/// New-Item `-ItemType` values that create filesystem links (reparse points
/// or hard links). All three redirect path resolution at runtime — symbolic
/// links and junctions are directory/file reparse points; hard links alias a
/// file's inode. Any of these let a later relative-path write land outside
/// the validator's view.
const LINK_ITEM_TYPES: &[&str] = &["symboliclink", "junction", "hardlink"];

/// Resolve a PowerShell cmdlet alias to its canonical lowercase form.
/// Only the alias entries needed by `is_symlink_creating_command`. Other
/// callsites use the larger table in `readOnlyValidation.ts` / `read_only_validation.rs`.
fn resolve_to_canonical(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "ni" => "new-item".to_string(),
        _ => lower,
    }
}

/// Check if a lowered, dash-normalized arg (colon-value stripped) is an
/// unambiguous PowerShell abbreviation of `New-Item`'s `-ItemType` / `-Type`
/// parameter. Minimum prefixes: `-it` (avoids ambiguity with other New-Item
/// params), `-ty` (avoids `-t` colliding with `-Target`).
fn is_item_type_param_abbrev(p: &str) -> bool {
    let len = p.len();
    (len >= 3 && "-itemtype".starts_with(p))
        || (len >= 3 && "-type".starts_with(p))
}

/// `modeValidation.ts` `isSymlinkCreatingCommand` — detects `New-Item`
/// creating a filesystem link (`-ItemType SymbolicLink` / `Junction` /
/// `HardLink`, or the `-Type` alias).
///
/// Links poison subsequent path resolution the same way `Set-Location` /
/// `New-PSDrive` do: a relative path through the link resolves to the link
/// target, not the validator's view. (Finding #18.)
///
/// Handles PS parameter abbreviation (`-it`, `-ite`, ... `-itemtype`; `-ty`,
/// `-typ`, `-type`), unicode dash prefixes (en-dash / em-dash /
/// horizontal-bar), and colon-bound values (`-it:Junction`).
pub fn is_symlink_creating_command(cmd: &PowerShellCommand) -> bool {
    let canonical = resolve_to_canonical(&cmd.name);
    if canonical != "new-item" {
        return false;
    }
    for i in 0..cmd.args.len() {
        let raw = &cmd.args[i];
        if raw.is_empty() {
            continue;
        }
        let first_char = raw.chars().next().unwrap();
        let normalized = if PS_TOKENIZER_DASH_CHARS.contains(&first_char) || first_char == '/' {
            let mut chars = raw.chars();
            chars.next();
            format!("-{}", chars.collect::<String>())
        } else {
            raw.clone()
        };
        let lower = normalized.to_lowercase();
        // Split colon-bound value: -it:SymbolicLink → param='-it', val='symboliclink'
        let colon_idx = lower[1..].find(':').map(|i| i + 1);
        let param_raw = match colon_idx {
            Some(idx) => &lower[..idx],
            None => &lower,
        };
        // Strip backtick escapes: -Item`Type → -ItemType.
        let param = param_raw.replace('`', "");
        if !is_item_type_param_abbrev(&param) {
            continue;
        }
        let raw_val = match colon_idx {
            Some(idx) => lower[idx + 1..].to_string(),
            None => cmd
                .args
                .get(i + 1)
                .map(|s| s.to_lowercase())
                .unwrap_or_default(),
        };
        // Strip backticks and surrounding single/double quotes.
        let mut val = raw_val.replace('`', "");
        if val.len() >= 2 {
            let bytes = val.as_bytes();
            let first = bytes[0] as char;
            let last = bytes[bytes.len() - 1] as char;
            if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
                val = val[1..val.len() - 1].to_string();
            }
        }
        if LINK_ITEM_TYPES.contains(&val.as_str()) {
            return true;
        }
    }
    false
}

/// Alias matching the TS export name.
#[allow(non_snake_case)]
pub fn isSymlinkCreatingCommand(cmd: &PowerShellCommand) -> bool {
    is_symlink_creating_command(cmd)
}

/// Validate a command against the current permission mode.
pub fn validate_mode(command: &str, mode: &str) -> ModeValidationResult {
    match mode {
        "plan" | "read-only" => validate_read_only_mode(command),
        "acceptEdits" | "dontAsk" => ModeValidationResult::Allowed,
        _ => ModeValidationResult::Allowed,
    }
}

/// Validate that a command is read-only (for plan mode).
fn validate_read_only_mode(command: &str) -> ModeValidationResult {
    // Split on pipeline/statement separators and check each segment
    let segments: Vec<&str> = command.split([';', '\n']).collect();

    for segment in segments {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if it's an always-safe command
        let is_safe = ALWAYS_SAFE_COMMANDS.iter().any(|re| re.is_match(trimmed));
        if is_safe {
            continue;
        }

        // Check for git read-only commands
        if is_git_read_only(trimmed) {
            continue;
        }

        // Check for write commands
        for re in WRITE_COMMANDS.iter() {
            if re.is_match(trimmed) {
                return ModeValidationResult::Blocked {
                    reason: format!(
                        "Command appears to modify the filesystem or system state, \
                         which is not allowed in read-only/plan mode: {}",
                        truncate_for_display(trimmed, 80)
                    ),
                };
            }
        }

        // Check for output redirection
        if has_output_redirection(trimmed) {
            return ModeValidationResult::Blocked {
                reason: "Output redirection (>, >>) is not allowed in read-only/plan mode"
                    .to_string(),
            };
        }
    }

    ModeValidationResult::Allowed
}

/// Check if a git command is read-only.
fn is_git_read_only(command: &str) -> bool {
    let git_ro = Regex::new(
        r"(?i)^\s*git\s+(status|log|diff|show|branch|tag|remote|config\s+--get|ls-files|ls-tree|rev-parse|describe|blame|shortlog|stash\s+list|reflog)"
    ).unwrap();
    git_ro.is_match(command)
}

/// Check for output redirection operators.
fn has_output_redirection(command: &str) -> bool {
    // Simple heuristic: look for > or >> not inside quotes
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let chars: Vec<char> = command.chars().collect();

    for i in 0..chars.len() {
        match chars[i] {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '>' if !in_single_quote && !in_double_quote => {
                // Not a comparison operator in PS context
                // Check it's not part of -gt, -ge, etc.
                if i > 0 && chars[i - 1] == '-' {
                    continue;
                }
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Truncate a string for display purposes.
fn truncate_for_display(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
