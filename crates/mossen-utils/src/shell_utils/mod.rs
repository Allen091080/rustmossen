use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FlagArgType {
    None,
    Number,
    StringArg,
    Char,
    Braces,
    Eof,
}

#[derive(Debug, Clone)]
pub struct ExternalCommandConfig {
    pub safe_flags: HashMap<String, FlagArgType>,
    pub additional_command_is_dangerous_callback:
        Option<fn(raw_command: &str, args: &[String]) -> bool>,
    pub respects_double_dash: bool,
}

impl Default for ExternalCommandConfig {
    fn default() -> Self {
        Self {
            safe_flags: HashMap::new(),
            additional_command_is_dangerous_callback: None,
            respects_double_dash: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShellType {
    Bash,
    PowerShell,
}

pub const DEFAULT_HOOK_SHELL: ShellType = ShellType::Bash;

pub struct ShellProvider {
    pub shell_type: ShellType,
    pub shell_path: String,
    pub detached: bool,
}

#[derive(Debug, Clone)]
pub struct CommandPrefixResult {
    pub command_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommandSubcommandPrefixResult {
    pub command_prefix: Option<String>,
    pub subcommand_prefixes: HashMap<String, CommandPrefixResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PowerShellEdition {
    Core,
    Desktop,
}

// ─── Constants ───────────────────────────────────────────────────────────────

pub const BASH_MAX_OUTPUT_UPPER_LIMIT: usize = 150_000;
pub const BASH_MAX_OUTPUT_DEFAULT: usize = 30_000;

pub static DANGEROUS_SHELL_PREFIXES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("sh");
    s.insert("bash");
    s.insert("zsh");
    s.insert("fish");
    s.insert("csh");
    s.insert("tcsh");
    s.insert("ksh");
    s.insert("dash");
    s.insert("cmd");
    s.insert("cmd.exe");
    s.insert("powershell");
    s.insert("powershell.exe");
    s.insert("pwsh");
    s.insert("pwsh.exe");
    s.insert("bash.exe");
    s
});

pub static FLAG_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-[a-zA-Z0-9_-]").unwrap());

pub const EXTERNAL_READONLY_COMMANDS: &[&str] = &["docker ps", "docker images"];

// ─── Depth rules for spec-based prefix ───────────────────────────────────────

pub static DEPTH_RULES: Lazy<HashMap<&'static str, usize>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("rg", 2);
    m.insert("pre-commit", 2);
    m.insert("gcloud", 4);
    m.insert("gcloud compute", 6);
    m.insert("gcloud beta", 6);
    m.insert("aws", 4);
    m.insert("az", 4);
    m.insert("kubectl", 3);
    m.insert("docker", 3);
    m.insert("dotnet", 3);
    m.insert("git push", 2);
    m
});

// ─── Output limits ───────────────────────────────────────────────────────────

pub fn get_max_output_length() -> usize {
    match env::var("BASH_MAX_OUTPUT_LENGTH") {
        Ok(val) => match val.parse::<usize>() {
            Ok(n) => {
                if n > BASH_MAX_OUTPUT_UPPER_LIMIT {
                    BASH_MAX_OUTPUT_UPPER_LIMIT
                } else if n == 0 {
                    BASH_MAX_OUTPUT_DEFAULT
                } else {
                    n
                }
            }
            Err(_) => BASH_MAX_OUTPUT_DEFAULT,
        },
        Err(_) => BASH_MAX_OUTPUT_DEFAULT,
    }
}

// ─── Shell tool utils ────────────────────────────────────────────────────────

pub fn is_powershell_tool_enabled() -> bool {
    if cfg!(not(target_os = "windows")) {
        return false;
    }
    let user_type = env::var("USER_TYPE").unwrap_or_default();
    let ps_env = env::var("MOSSEN_CODE_USE_POWERSHELL_TOOL").unwrap_or_default();
    if user_type == "internal" {
        // Internal defaults on; opt-out via false/0/no
        let lower = ps_env.to_lowercase();
        !(lower == "false" || lower == "0" || lower == "no")
    } else {
        // External defaults off; opt-in via true/1/yes
        let lower = ps_env.to_lowercase();
        lower == "true" || lower == "1" || lower == "yes"
    }
}

pub fn resolve_default_shell() -> ShellType {
    // settings.defaultShell → 'bash'
    ShellType::Bash
}

// ─── PowerShell detection ────────────────────────────────────────────────────

pub async fn find_powershell() -> Option<String> {
    let pwsh = which_command("pwsh").await;
    if let Some(ref path) = pwsh {
        if cfg!(target_os = "linux") {
            let resolved = tokio::fs::canonicalize(path).await.ok();
            let resolved_str = resolved
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());

            if path.starts_with("/snap/") || resolved_str.starts_with("/snap/") {
                // Try direct binary paths
                for probe_path in &["/opt/microsoft/powershell/7/pwsh", "/usr/bin/pwsh"] {
                    if let Ok(meta) = tokio::fs::metadata(probe_path).await {
                        if meta.is_file() {
                            let direct_resolved = tokio::fs::canonicalize(probe_path)
                                .await
                                .ok()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| probe_path.to_string());
                            if !probe_path.starts_with("/snap/")
                                && !direct_resolved.starts_with("/snap/")
                            {
                                return Some(probe_path.to_string());
                            }
                        }
                    }
                }
            }
        }
        return pwsh;
    }

    let powershell = which_command("powershell").await;
    if powershell.is_some() {
        return powershell;
    }

    None
}

async fn which_command(name: &str) -> Option<String> {
    let cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let output = tokio::process::Command::new(cmd)
        .arg(name)
        .output()
        .await
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .trim()
            .to_string();
        if !stdout.is_empty() {
            Some(stdout)
        } else {
            None
        }
    } else {
        None
    }
}

static CACHED_POWERSHELL_PATH: Lazy<tokio::sync::OnceCell<Option<String>>> =
    Lazy::new(tokio::sync::OnceCell::new);

pub async fn get_cached_powershell_path() -> Option<String> {
    CACHED_POWERSHELL_PATH
        .get_or_init(|| async { find_powershell().await })
        .await
        .clone()
}

pub async fn get_powershell_edition() -> Option<PowerShellEdition> {
    let path = get_cached_powershell_path().await?;
    let base = Path::new(&path)
        .file_name()?
        .to_string_lossy()
        .to_lowercase()
        .replace(".exe", "");
    if base == "pwsh" {
        Some(PowerShellEdition::Core)
    } else {
        Some(PowerShellEdition::Desktop)
    }
}

// ─── PowerShell provider helpers ─────────────────────────────────────────────

pub fn build_powershell_args(cmd: &str) -> Vec<String> {
    vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        cmd.to_string(),
    ]
}

pub fn encode_powershell_command(ps_command: &str) -> String {
    // UTF-16LE encode then base64
    let utf16: Vec<u8> = ps_command
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(&utf16)
}

// ─── Bash provider helpers ───────────────────────────────────────────────────

pub fn get_disable_extglob_command(shell_path: &str) -> Option<String> {
    if env::var("MOSSEN_CODE_SHELL_PREFIX").is_ok() {
        return Some(
            "{ shopt -u extglob || setopt NO_EXTENDED_GLOB; } >/dev/null 2>&1 || true".to_string(),
        );
    }
    if shell_path.contains("bash") {
        Some("shopt -u extglob 2>/dev/null || true".to_string())
    } else if shell_path.contains("zsh") {
        Some("setopt NO_EXTENDED_GLOB 2>/dev/null || true".to_string())
    } else {
        None
    }
}

// ─── UNC path detection ──────────────────────────────────────────────────────

pub fn contains_vulnerable_unc_path(path_or_command: &str) -> bool {
    if cfg!(not(target_os = "windows")) {
        return false;
    }

    static BACKSLASH_UNC: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\\\\[^\s\\/]+(?:@(?:\d+|ssl))?(?:[\\/]|$|\s)").unwrap());
    if BACKSLASH_UNC.is_match(path_or_command) {
        return true;
    }

    // Forward-slash UNC (without preceding colon)
    static FORWARD_SLASH_UNC: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"//?[^\s\\/]+(?:@(?:\d+|ssl))?(?:[\\/]|$|\s)").unwrap());
    if FORWARD_SLASH_UNC
        .find_iter(path_or_command)
        .any(|m| m.start() == 0 || path_or_command.as_bytes().get(m.start() - 1) != Some(&b':'))
    {
        return true;
    }

    // Mixed separator patterns
    static MIXED_SLASH: Lazy<Regex> = Lazy::new(|| Regex::new(r"/\\{2,}[^\s\\/]").unwrap());
    if MIXED_SLASH.is_match(path_or_command) {
        return true;
    }

    static REVERSE_MIXED: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\{2,}/[^\s\\/]").unwrap());
    if REVERSE_MIXED.is_match(path_or_command) {
        return true;
    }

    // WebDAV SSL/port patterns
    static SSL_PORT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)@SSL@\d+").unwrap());
    static PORT_SSL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)@\d+@SSL").unwrap());
    if SSL_PORT.is_match(path_or_command) || PORT_SSL.is_match(path_or_command) {
        return true;
    }

    // DavWWWRoot marker
    static DAV_ROOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)DavWWWRoot").unwrap());
    if DAV_ROOT.is_match(path_or_command) {
        return true;
    }

    // IPv4 UNC
    static IPV4_BACKSLASH: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^\\\\(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})[\\/]").unwrap());
    static IPV4_FORWARD: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^//(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})[\\/]").unwrap());
    if IPV4_BACKSLASH.is_match(path_or_command) || IPV4_FORWARD.is_match(path_or_command) {
        return true;
    }

    // IPv6 UNC
    static IPV6_BACKSLASH: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^\\\\(\[[\da-fA-F:]+\])[\\/]").unwrap());
    static IPV6_FORWARD: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^//(\[[\da-fA-F:]+\])[\\/]").unwrap());
    if IPV6_BACKSLASH.is_match(path_or_command) || IPV6_FORWARD.is_match(path_or_command) {
        return true;
    }

    false
}

// ─── Flag validation ─────────────────────────────────────────────────────────

pub fn validate_flag_argument(value: &str, arg_type: &FlagArgType) -> bool {
    match arg_type {
        FlagArgType::None => false,
        FlagArgType::Number => value.chars().all(|c| c.is_ascii_digit()) && !value.is_empty(),
        FlagArgType::StringArg => true,
        FlagArgType::Char => value.len() == 1,
        FlagArgType::Braces => value == "{}",
        FlagArgType::Eof => value == "EOF",
    }
}

pub fn validate_flags(
    tokens: &[String],
    start_index: usize,
    config: &ExternalCommandConfig,
    command_name: Option<&str>,
    _raw_command: Option<&str>,
    xargs_target_commands: Option<&[String]>,
) -> bool {
    let mut i = start_index;

    while i < tokens.len() {
        let token = &tokens[i];
        if token.is_empty() {
            i += 1;
            continue;
        }

        // xargs target command detection
        if let Some(targets) = xargs_target_commands {
            if command_name == Some("xargs") && (!token.starts_with('-') || token == "--") {
                let check_token = if token == "--" && i + 1 < tokens.len() {
                    i += 1;
                    &tokens[i]
                } else {
                    token
                };
                if targets.iter().any(|t| t == check_token) {
                    break;
                }
                return false;
            }
        }

        if token == "--" {
            if config.respects_double_dash {
                i += 1;
                break;
            }
            i += 1;
            continue;
        }

        if token.starts_with('-') && token.len() > 1 && FLAG_PATTERN.is_match(token) {
            let has_equals = token.contains('=');
            let (flag, inline_value) = if has_equals {
                let parts: Vec<&str> = token.splitn(2, '=').collect();
                (
                    parts[0].to_string(),
                    parts.get(1).unwrap_or(&"").to_string(),
                )
            } else {
                (token.clone(), String::new())
            };

            if flag.is_empty() {
                return false;
            }

            let flag_arg_type = config.safe_flags.get(&flag);

            match flag_arg_type {
                None => {
                    // Git numeric shorthand
                    if command_name == Some("git") {
                        static GIT_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-\d+$").unwrap());
                        if GIT_NUM.is_match(&flag) {
                            i += 1;
                            continue;
                        }
                    }

                    // grep/rg attached numeric
                    if (command_name == Some("grep") || command_name == Some("rg"))
                        && flag.starts_with('-')
                        && !flag.starts_with("--")
                        && flag.len() > 2
                    {
                        let potential_flag = &flag[..2];
                        let potential_value = &flag[2..];
                        if let Some(ft) = config.safe_flags.get(potential_flag) {
                            static DIGITS: Lazy<Regex> =
                                Lazy::new(|| Regex::new(r"^\d+$").unwrap());
                            if DIGITS.is_match(potential_value)
                                && (*ft == FlagArgType::Number || *ft == FlagArgType::StringArg)
                            {
                                if validate_flag_argument(potential_value, ft) {
                                    i += 1;
                                    continue;
                                } else {
                                    return false;
                                }
                            }
                        }
                    }

                    // Combined single-letter flags
                    if flag.starts_with('-') && !flag.starts_with("--") && flag.len() > 2 {
                        let mut all_valid = true;
                        for ch in flag[1..].chars() {
                            let single_flag = format!("-{}", ch);
                            match config.safe_flags.get(&single_flag) {
                                Some(ft) => {
                                    if *ft != FlagArgType::None {
                                        return false;
                                    }
                                }
                                None => {
                                    all_valid = false;
                                    break;
                                }
                            }
                        }
                        if all_valid {
                            i += 1;
                            continue;
                        }
                        return false;
                    } else {
                        return false;
                    }
                }
                Some(ft) => {
                    if *ft == FlagArgType::None {
                        if has_equals {
                            return false;
                        }
                        i += 1;
                    } else {
                        let arg_value = if has_equals {
                            i += 1;
                            inline_value.clone()
                        } else {
                            if i + 1 >= tokens.len() {
                                return false;
                            }
                            let next = &tokens[i + 1];
                            if next.starts_with('-')
                                && next.len() > 1
                                && FLAG_PATTERN.is_match(next)
                            {
                                return false;
                            }
                            let val = next.clone();
                            i += 2;
                            val
                        };

                        // String args: reject values starting with '-' (defense-in-depth)
                        if *ft == FlagArgType::StringArg && arg_value.starts_with('-') {
                            if flag == "--sort" && command_name == Some("git") {
                                static SORT_KEY: Lazy<Regex> =
                                    Lazy::new(|| Regex::new(r"^-[a-zA-Z]").unwrap());
                                if SORT_KEY.is_match(&arg_value) {
                                    // Allow reverse sort
                                } else {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }

                        if !validate_flag_argument(&arg_value, ft) {
                            return false;
                        }
                    }
                }
            }
        } else {
            // Non-flag argument
            i += 1;
        }
    }

    true
}

// ─── Git read-only command callbacks ─────────────────────────────────────────

pub fn git_reflog_is_dangerous(_raw_command: &str, args: &[String]) -> bool {
    static DANGEROUS_SUBCOMMANDS: Lazy<HashSet<&str>> = Lazy::new(|| {
        let mut s = HashSet::new();
        s.insert("expire");
        s.insert("delete");
        s.insert("exists");
        s
    });
    for token in args {
        if token.is_empty() || token.starts_with('-') {
            continue;
        }
        if DANGEROUS_SUBCOMMANDS.contains(token.as_str()) {
            return true;
        }
        return false;
    }
    false
}

pub fn git_remote_show_is_dangerous(_raw_command: &str, args: &[String]) -> bool {
    let positional: Vec<&String> = args.iter().filter(|a| *a != "-n").collect();
    if positional.len() != 1 {
        return true;
    }
    static REMOTE_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());
    !REMOTE_NAME.is_match(positional[0])
}

pub fn git_remote_is_dangerous(_raw_command: &str, args: &[String]) -> bool {
    args.iter().any(|a| a != "-v" && a != "--verbose")
}

pub fn git_tag_is_dangerous(_raw_command: &str, args: &[String]) -> bool {
    static FLAGS_WITH_ARGS: Lazy<HashSet<&str>> = Lazy::new(|| {
        let mut s = HashSet::new();
        s.insert("--contains");
        s.insert("--no-contains");
        s.insert("--merged");
        s.insert("--no-merged");
        s.insert("--points-at");
        s.insert("--sort");
        s.insert("--format");
        s.insert("-n");
        s
    });

    let mut i = 0;
    let mut seen_list_flag = false;
    let mut seen_dash_dash = false;

    while i < args.len() {
        let token = &args[i];
        if token.is_empty() {
            i += 1;
            continue;
        }
        if token == "--" && !seen_dash_dash {
            seen_dash_dash = true;
            i += 1;
            continue;
        }
        if !seen_dash_dash && token.starts_with('-') {
            if token == "--list" || token == "-l" {
                seen_list_flag = true;
            } else if token.starts_with('-')
                && !token.starts_with("--")
                && token.len() > 2
                && !token.contains('=')
                && token[1..].contains('l')
            {
                seen_list_flag = true;
            }
            if token.contains('=') {
                i += 1;
            } else if FLAGS_WITH_ARGS.contains(token.as_str()) {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            if !seen_list_flag {
                return true;
            }
            i += 1;
        }
    }
    false
}

pub fn git_branch_is_dangerous(_raw_command: &str, args: &[String]) -> bool {
    static FLAGS_WITH_ARGS: Lazy<HashSet<&str>> = Lazy::new(|| {
        let mut s = HashSet::new();
        s.insert("--contains");
        s.insert("--no-contains");
        s.insert("--points-at");
        s.insert("--sort");
        s
    });
    static FLAGS_WITH_OPTIONAL_ARGS: Lazy<HashSet<&str>> = Lazy::new(|| {
        let mut s = HashSet::new();
        s.insert("--merged");
        s.insert("--no-merged");
        s
    });

    let mut i = 0;
    let mut last_flag = String::new();
    let mut seen_list_flag = false;
    let mut seen_dash_dash = false;

    while i < args.len() {
        let token = &args[i];
        if token.is_empty() {
            i += 1;
            continue;
        }
        if token == "--" && !seen_dash_dash {
            seen_dash_dash = true;
            last_flag.clear();
            i += 1;
            continue;
        }
        if !seen_dash_dash && token.starts_with('-') {
            if token == "--list" || token == "-l" {
                seen_list_flag = true;
            } else if token.starts_with('-')
                && !token.starts_with("--")
                && token.len() > 2
                && !token.contains('=')
                && token[1..].contains('l')
            {
                seen_list_flag = true;
            }
            if token.contains('=') {
                last_flag = token.split('=').next().unwrap_or("").to_string();
                i += 1;
            } else if FLAGS_WITH_ARGS.contains(token.as_str()) {
                last_flag = token.clone();
                i += 2;
            } else {
                last_flag = token.clone();
                i += 1;
            }
        } else {
            let last_flag_has_optional_arg = FLAGS_WITH_OPTIONAL_ARGS.contains(last_flag.as_str());
            if !seen_list_flag && !last_flag_has_optional_arg {
                return true;
            }
            i += 1;
        }
    }
    false
}

pub fn gh_is_dangerous_callback(_raw_command: &str, args: &[String]) -> bool {
    for token in args {
        if token.is_empty() {
            continue;
        }
        let value = if token.starts_with('-') {
            match token.find('=') {
                Some(idx) => {
                    let v = &token[idx + 1..];
                    if v.is_empty() {
                        continue;
                    }
                    v.to_string()
                }
                None => continue,
            }
        } else {
            token.clone()
        };

        if !value.contains('/') && !value.contains("://") && !value.contains('@') {
            continue;
        }
        if value.contains("://") {
            return true;
        }
        if value.contains('@') {
            return true;
        }
        let slash_count = value.matches('/').count();
        if slash_count >= 2 {
            return true;
        }
    }
    false
}

// ─── Read-only command config builders ───────────────────────────────────────

fn make_git_flag_groups() -> (
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
    HashMap<String, FlagArgType>,
) {
    let ref_selection: HashMap<String, FlagArgType> = [
        ("--all", FlagArgType::None),
        ("--branches", FlagArgType::None),
        ("--tags", FlagArgType::None),
        ("--remotes", FlagArgType::None),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let date_filter: HashMap<String, FlagArgType> = [
        ("--since", FlagArgType::StringArg),
        ("--after", FlagArgType::StringArg),
        ("--until", FlagArgType::StringArg),
        ("--before", FlagArgType::StringArg),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let log_display: HashMap<String, FlagArgType> = [
        ("--oneline", FlagArgType::None),
        ("--graph", FlagArgType::None),
        ("--decorate", FlagArgType::None),
        ("--no-decorate", FlagArgType::None),
        ("--date", FlagArgType::StringArg),
        ("--relative-date", FlagArgType::None),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let count_flags: HashMap<String, FlagArgType> = [
        ("--max-count", FlagArgType::Number),
        ("-n", FlagArgType::Number),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let stat_flags: HashMap<String, FlagArgType> = [
        ("--stat", FlagArgType::None),
        ("--numstat", FlagArgType::None),
        ("--shortstat", FlagArgType::None),
        ("--name-only", FlagArgType::None),
        ("--name-status", FlagArgType::None),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let color_flags: HashMap<String, FlagArgType> = [
        ("--color", FlagArgType::None),
        ("--no-color", FlagArgType::None),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let patch_flags: HashMap<String, FlagArgType> = [
        ("--patch", FlagArgType::None),
        ("-p", FlagArgType::None),
        ("--no-patch", FlagArgType::None),
        ("--no-ext-diff", FlagArgType::None),
        ("-s", FlagArgType::None),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    let author_filter: HashMap<String, FlagArgType> = [
        ("--author", FlagArgType::StringArg),
        ("--committer", FlagArgType::StringArg),
        ("--grep", FlagArgType::StringArg),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.clone()))
    .collect();

    (
        ref_selection,
        date_filter,
        log_display,
        count_flags,
        stat_flags,
        color_flags,
        patch_flags,
        author_filter,
    )
}

pub fn build_git_read_only_commands() -> HashMap<String, ExternalCommandConfig> {
    let (
        ref_selection,
        date_filter,
        log_display,
        count_flags,
        stat_flags,
        color_flags,
        patch_flags,
        author_filter,
    ) = make_git_flag_groups();

    let mut commands: HashMap<String, ExternalCommandConfig> = HashMap::new();

    // git diff
    let mut diff_flags = stat_flags.clone();
    diff_flags.extend(color_flags.clone());
    for (k, v) in [
        ("--dirstat", FlagArgType::None),
        ("--summary", FlagArgType::None),
        ("--word-diff", FlagArgType::None),
        ("--word-diff-regex", FlagArgType::StringArg),
        ("--color-words", FlagArgType::None),
        ("--no-renames", FlagArgType::None),
        ("--no-ext-diff", FlagArgType::None),
        ("--check", FlagArgType::None),
        ("--full-index", FlagArgType::None),
        ("--binary", FlagArgType::None),
        ("--abbrev", FlagArgType::Number),
        ("--cached", FlagArgType::None),
        ("--staged", FlagArgType::None),
        ("--exit-code", FlagArgType::None),
        ("--quiet", FlagArgType::None),
        ("--relative", FlagArgType::StringArg),
        ("--diff-filter", FlagArgType::StringArg),
        ("--diff-algorithm", FlagArgType::StringArg),
        ("-p", FlagArgType::None),
        ("-u", FlagArgType::None),
        ("-s", FlagArgType::None),
        ("-M", FlagArgType::None),
        ("-C", FlagArgType::None),
        ("-S", FlagArgType::StringArg),
        ("-G", FlagArgType::StringArg),
        ("-O", FlagArgType::StringArg),
        ("-R", FlagArgType::None),
    ] {
        diff_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git diff".to_string(),
        ExternalCommandConfig {
            safe_flags: diff_flags,
            ..Default::default()
        },
    );

    // git log
    let mut log_flags = log_display.clone();
    log_flags.extend(ref_selection.clone());
    log_flags.extend(date_filter.clone());
    log_flags.extend(count_flags.clone());
    log_flags.extend(stat_flags.clone());
    log_flags.extend(color_flags.clone());
    log_flags.extend(patch_flags.clone());
    log_flags.extend(author_filter.clone());
    for (k, v) in [
        ("--abbrev-commit", FlagArgType::None),
        ("--first-parent", FlagArgType::None),
        ("--merges", FlagArgType::None),
        ("--no-merges", FlagArgType::None),
        ("--reverse", FlagArgType::None),
        ("--follow", FlagArgType::None),
        ("--pretty", FlagArgType::StringArg),
        ("--format", FlagArgType::StringArg),
        ("--diff-filter", FlagArgType::StringArg),
        ("-S", FlagArgType::StringArg),
        ("-G", FlagArgType::StringArg),
        ("--topo-order", FlagArgType::None),
        ("--date-order", FlagArgType::None),
        ("--skip", FlagArgType::Number),
    ] {
        log_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git log".to_string(),
        ExternalCommandConfig {
            safe_flags: log_flags,
            ..Default::default()
        },
    );

    // git status
    let mut status_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--short", FlagArgType::None),
        ("-s", FlagArgType::None),
        ("--branch", FlagArgType::None),
        ("-b", FlagArgType::None),
        ("--porcelain", FlagArgType::None),
        ("--long", FlagArgType::None),
        ("--verbose", FlagArgType::None),
        ("-v", FlagArgType::None),
        ("--untracked-files", FlagArgType::StringArg),
        ("-u", FlagArgType::StringArg),
        ("--ignored", FlagArgType::None),
    ] {
        status_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git status".to_string(),
        ExternalCommandConfig {
            safe_flags: status_flags,
            ..Default::default()
        },
    );

    // git branch
    let mut branch_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("-l", FlagArgType::None),
        ("--list", FlagArgType::None),
        ("-a", FlagArgType::None),
        ("--all", FlagArgType::None),
        ("-r", FlagArgType::None),
        ("--remotes", FlagArgType::None),
        ("-v", FlagArgType::None),
        ("-vv", FlagArgType::None),
        ("--verbose", FlagArgType::None),
        ("--color", FlagArgType::None),
        ("--no-color", FlagArgType::None),
        ("--abbrev", FlagArgType::Number),
        ("--contains", FlagArgType::StringArg),
        ("--no-contains", FlagArgType::StringArg),
        ("--merged", FlagArgType::None),
        ("--no-merged", FlagArgType::None),
        ("--points-at", FlagArgType::StringArg),
        ("--sort", FlagArgType::StringArg),
        ("--show-current", FlagArgType::None),
        ("-i", FlagArgType::None),
    ] {
        branch_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git branch".to_string(),
        ExternalCommandConfig {
            safe_flags: branch_flags,
            additional_command_is_dangerous_callback: Some(git_branch_is_dangerous),
            ..Default::default()
        },
    );

    // git tag
    let mut tag_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("-l", FlagArgType::None),
        ("--list", FlagArgType::None),
        ("-n", FlagArgType::Number),
        ("--contains", FlagArgType::StringArg),
        ("--no-contains", FlagArgType::StringArg),
        ("--merged", FlagArgType::StringArg),
        ("--no-merged", FlagArgType::StringArg),
        ("--sort", FlagArgType::StringArg),
        ("--format", FlagArgType::StringArg),
        ("--points-at", FlagArgType::StringArg),
    ] {
        tag_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git tag".to_string(),
        ExternalCommandConfig {
            safe_flags: tag_flags,
            additional_command_is_dangerous_callback: Some(git_tag_is_dangerous),
            ..Default::default()
        },
    );

    // git remote
    let mut remote_flags: HashMap<String, FlagArgType> = HashMap::new();
    remote_flags.insert("-v".to_string(), FlagArgType::None);
    remote_flags.insert("--verbose".to_string(), FlagArgType::None);
    commands.insert(
        "git remote".to_string(),
        ExternalCommandConfig {
            safe_flags: remote_flags,
            additional_command_is_dangerous_callback: Some(git_remote_is_dangerous),
            ..Default::default()
        },
    );

    // git remote show
    let mut remote_show_flags: HashMap<String, FlagArgType> = HashMap::new();
    remote_show_flags.insert("-n".to_string(), FlagArgType::None);
    commands.insert(
        "git remote show".to_string(),
        ExternalCommandConfig {
            safe_flags: remote_show_flags,
            additional_command_is_dangerous_callback: Some(git_remote_show_is_dangerous),
            ..Default::default()
        },
    );

    // git reflog
    let mut reflog_flags = log_display.clone();
    reflog_flags.extend(ref_selection.clone());
    reflog_flags.extend(date_filter.clone());
    reflog_flags.extend(count_flags.clone());
    reflog_flags.extend(author_filter.clone());
    commands.insert(
        "git reflog".to_string(),
        ExternalCommandConfig {
            safe_flags: reflog_flags,
            additional_command_is_dangerous_callback: Some(git_reflog_is_dangerous),
            ..Default::default()
        },
    );

    // git show
    let mut show_flags = log_display.clone();
    show_flags.extend(stat_flags.clone());
    show_flags.extend(color_flags.clone());
    show_flags.extend(patch_flags.clone());
    for (k, v) in [
        ("--abbrev-commit", FlagArgType::None),
        ("--word-diff", FlagArgType::None),
        ("--pretty", FlagArgType::StringArg),
        ("--format", FlagArgType::StringArg),
        ("--first-parent", FlagArgType::None),
        ("--raw", FlagArgType::None),
        ("--diff-filter", FlagArgType::StringArg),
        ("-m", FlagArgType::None),
        ("--quiet", FlagArgType::None),
    ] {
        show_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git show".to_string(),
        ExternalCommandConfig {
            safe_flags: show_flags,
            ..Default::default()
        },
    );

    // git blame
    let mut blame_flags = color_flags.clone();
    for (k, v) in [
        ("-L", FlagArgType::StringArg),
        ("--porcelain", FlagArgType::None),
        ("-p", FlagArgType::None),
        ("--date", FlagArgType::StringArg),
        ("-w", FlagArgType::None),
        ("--ignore-rev", FlagArgType::StringArg),
        ("--abbrev", FlagArgType::Number),
        ("-n", FlagArgType::None),
        ("-e", FlagArgType::None),
        ("-s", FlagArgType::None),
    ] {
        blame_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git blame".to_string(),
        ExternalCommandConfig {
            safe_flags: blame_flags,
            ..Default::default()
        },
    );

    // git rev-parse
    let mut rev_parse_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--verify", FlagArgType::None),
        ("--short", FlagArgType::StringArg),
        ("--abbrev-ref", FlagArgType::None),
        ("--symbolic", FlagArgType::None),
        ("--show-toplevel", FlagArgType::None),
        ("--git-dir", FlagArgType::None),
        ("--is-inside-work-tree", FlagArgType::None),
        ("--is-bare-repository", FlagArgType::None),
    ] {
        rev_parse_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git rev-parse".to_string(),
        ExternalCommandConfig {
            safe_flags: rev_parse_flags,
            ..Default::default()
        },
    );

    // git ls-files
    let mut ls_files_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--cached", FlagArgType::None),
        ("-c", FlagArgType::None),
        ("--deleted", FlagArgType::None),
        ("-d", FlagArgType::None),
        ("--modified", FlagArgType::None),
        ("-m", FlagArgType::None),
        ("--others", FlagArgType::None),
        ("-o", FlagArgType::None),
        ("--full-name", FlagArgType::None),
        ("--exclude", FlagArgType::StringArg),
        ("-x", FlagArgType::StringArg),
        ("--exclude-standard", FlagArgType::None),
        ("-z", FlagArgType::None),
    ] {
        ls_files_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git ls-files".to_string(),
        ExternalCommandConfig {
            safe_flags: ls_files_flags,
            ..Default::default()
        },
    );

    // git merge-base
    let mut merge_base_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--is-ancestor", FlagArgType::None),
        ("--fork-point", FlagArgType::None),
        (concat!("--octo", "pus"), FlagArgType::None),
        ("--independent", FlagArgType::None),
        ("--all", FlagArgType::None),
    ] {
        merge_base_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "git merge-base".to_string(),
        ExternalCommandConfig {
            safe_flags: merge_base_flags,
            ..Default::default()
        },
    );

    commands
}

pub fn build_gh_read_only_commands() -> HashMap<String, ExternalCommandConfig> {
    let mut commands: HashMap<String, ExternalCommandConfig> = HashMap::new();

    // gh pr view
    let mut pr_view_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--json", FlagArgType::StringArg),
        ("--comments", FlagArgType::None),
        ("--repo", FlagArgType::StringArg),
        ("-R", FlagArgType::StringArg),
    ] {
        pr_view_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "gh pr view".to_string(),
        ExternalCommandConfig {
            safe_flags: pr_view_flags,
            additional_command_is_dangerous_callback: Some(gh_is_dangerous_callback),
            ..Default::default()
        },
    );

    // gh pr list
    let mut pr_list_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--state", FlagArgType::StringArg),
        ("-s", FlagArgType::StringArg),
        ("--author", FlagArgType::StringArg),
        ("--label", FlagArgType::StringArg),
        ("--limit", FlagArgType::Number),
        ("-L", FlagArgType::Number),
        ("--json", FlagArgType::StringArg),
        ("--repo", FlagArgType::StringArg),
        ("-R", FlagArgType::StringArg),
    ] {
        pr_list_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "gh pr list".to_string(),
        ExternalCommandConfig {
            safe_flags: pr_list_flags,
            additional_command_is_dangerous_callback: Some(gh_is_dangerous_callback),
            ..Default::default()
        },
    );

    // gh issue view
    let mut issue_view_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--json", FlagArgType::StringArg),
        ("--comments", FlagArgType::None),
        ("--repo", FlagArgType::StringArg),
        ("-R", FlagArgType::StringArg),
    ] {
        issue_view_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "gh issue view".to_string(),
        ExternalCommandConfig {
            safe_flags: issue_view_flags,
            additional_command_is_dangerous_callback: Some(gh_is_dangerous_callback),
            ..Default::default()
        },
    );

    // gh issue list
    let mut issue_list_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--state", FlagArgType::StringArg),
        ("--author", FlagArgType::StringArg),
        ("--label", FlagArgType::StringArg),
        ("--limit", FlagArgType::Number),
        ("-L", FlagArgType::Number),
        ("--json", FlagArgType::StringArg),
        ("--repo", FlagArgType::StringArg),
        ("-R", FlagArgType::StringArg),
    ] {
        issue_list_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "gh issue list".to_string(),
        ExternalCommandConfig {
            safe_flags: issue_list_flags,
            additional_command_is_dangerous_callback: Some(gh_is_dangerous_callback),
            ..Default::default()
        },
    );

    // gh run list
    let mut run_list_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--branch", FlagArgType::StringArg),
        ("-b", FlagArgType::StringArg),
        ("--status", FlagArgType::StringArg),
        ("-s", FlagArgType::StringArg),
        ("--workflow", FlagArgType::StringArg),
        ("-w", FlagArgType::StringArg),
        ("--limit", FlagArgType::Number),
        ("-L", FlagArgType::Number),
        ("--json", FlagArgType::StringArg),
        ("--repo", FlagArgType::StringArg),
        ("-R", FlagArgType::StringArg),
    ] {
        run_list_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "gh run list".to_string(),
        ExternalCommandConfig {
            safe_flags: run_list_flags,
            additional_command_is_dangerous_callback: Some(gh_is_dangerous_callback),
            ..Default::default()
        },
    );

    // gh run view
    let mut run_view_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--log", FlagArgType::None),
        ("--log-failed", FlagArgType::None),
        ("--verbose", FlagArgType::None),
        ("-v", FlagArgType::None),
        ("--json", FlagArgType::StringArg),
        ("--repo", FlagArgType::StringArg),
        ("-R", FlagArgType::StringArg),
        ("--job", FlagArgType::StringArg),
        ("-j", FlagArgType::StringArg),
    ] {
        run_view_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "gh run view".to_string(),
        ExternalCommandConfig {
            safe_flags: run_view_flags,
            additional_command_is_dangerous_callback: Some(gh_is_dangerous_callback),
            ..Default::default()
        },
    );

    commands
}

pub fn build_docker_read_only_commands() -> HashMap<String, ExternalCommandConfig> {
    let mut commands: HashMap<String, ExternalCommandConfig> = HashMap::new();

    let mut logs_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--follow", FlagArgType::None),
        ("-f", FlagArgType::None),
        ("--tail", FlagArgType::StringArg),
        ("-n", FlagArgType::StringArg),
        ("--timestamps", FlagArgType::None),
        ("-t", FlagArgType::None),
        ("--since", FlagArgType::StringArg),
        ("--until", FlagArgType::StringArg),
    ] {
        logs_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "docker logs".to_string(),
        ExternalCommandConfig {
            safe_flags: logs_flags,
            ..Default::default()
        },
    );

    let mut inspect_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("--format", FlagArgType::StringArg),
        ("-f", FlagArgType::StringArg),
        ("--type", FlagArgType::StringArg),
        ("--size", FlagArgType::None),
        ("-s", FlagArgType::None),
    ] {
        inspect_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "docker inspect".to_string(),
        ExternalCommandConfig {
            safe_flags: inspect_flags,
            ..Default::default()
        },
    );

    commands
}

pub fn build_ripgrep_read_only_commands() -> HashMap<String, ExternalCommandConfig> {
    let mut commands: HashMap<String, ExternalCommandConfig> = HashMap::new();

    let mut rg_flags: HashMap<String, FlagArgType> = HashMap::new();
    for (k, v) in [
        ("-e", FlagArgType::StringArg),
        ("--regexp", FlagArgType::StringArg),
        ("-f", FlagArgType::StringArg),
        ("-i", FlagArgType::None),
        ("--ignore-case", FlagArgType::None),
        ("-S", FlagArgType::None),
        ("--smart-case", FlagArgType::None),
        ("-F", FlagArgType::None),
        ("--fixed-strings", FlagArgType::None),
        ("-w", FlagArgType::None),
        ("--word-regexp", FlagArgType::None),
        ("-v", FlagArgType::None),
        ("--invert-match", FlagArgType::None),
        ("-c", FlagArgType::None),
        ("--count", FlagArgType::None),
        ("-l", FlagArgType::None),
        ("--files-with-matches", FlagArgType::None),
        ("-n", FlagArgType::None),
        ("--line-number", FlagArgType::None),
        ("-o", FlagArgType::None),
        ("--only-matching", FlagArgType::None),
        ("-A", FlagArgType::Number),
        ("--after-context", FlagArgType::Number),
        ("-B", FlagArgType::Number),
        ("--before-context", FlagArgType::Number),
        ("-C", FlagArgType::Number),
        ("--context", FlagArgType::Number),
        ("-H", FlagArgType::None),
        ("-h", FlagArgType::None),
        ("--heading", FlagArgType::None),
        ("--no-heading", FlagArgType::None),
        ("-q", FlagArgType::None),
        ("--quiet", FlagArgType::None),
        ("--column", FlagArgType::None),
        ("-g", FlagArgType::StringArg),
        ("--glob", FlagArgType::StringArg),
        ("-t", FlagArgType::StringArg),
        ("--type", FlagArgType::StringArg),
        ("-T", FlagArgType::StringArg),
        ("--type-not", FlagArgType::StringArg),
        ("--hidden", FlagArgType::None),
        ("--no-ignore", FlagArgType::None),
        ("-u", FlagArgType::None),
        ("-m", FlagArgType::Number),
        ("--max-count", FlagArgType::Number),
        ("-d", FlagArgType::Number),
        ("--max-depth", FlagArgType::Number),
        ("-a", FlagArgType::None),
        ("--text", FlagArgType::None),
        ("-L", FlagArgType::None),
        ("--follow", FlagArgType::None),
        ("--color", FlagArgType::StringArg),
        ("--json", FlagArgType::None),
        ("--stats", FlagArgType::None),
        ("--help", FlagArgType::None),
        ("--version", FlagArgType::None),
    ] {
        rg_flags.insert(k.to_string(), v);
    }
    commands.insert(
        "rg".to_string(),
        ExternalCommandConfig {
            safe_flags: rg_flags,
            ..Default::default()
        },
    );

    commands
}

// ─── Spec prefix (simplified) ────────────────────────────────────────────────

pub fn build_prefix(command: &str, args: &[String], max_depth: usize) -> String {
    let mut parts = vec![command.to_string()];

    for arg in args {
        if parts.len() >= max_depth {
            break;
        }
        if arg.starts_with('-') {
            break;
        }
        // Stop at file paths or URLs
        if arg.contains('/') || arg.contains('.') {
            let dot_index = arg.rfind('.');
            let has_extension = dot_index
                .map(|idx| idx > 0 && idx < arg.len() - 1 && !arg[idx + 1..].contains(':'))
                .unwrap_or(false);
            if arg.contains('/') || has_extension {
                break;
            }
        }
        parts.push(arg.clone());
    }

    parts.join(" ")
}

pub fn calculate_depth(command: &str, args: &[String]) -> usize {
    let command_lower = command.to_lowercase();

    // Check compound key first
    let first_non_flag = args.iter().find(|a| !a.starts_with('-'));
    if let Some(sub) = first_non_flag {
        let key = format!("{} {}", command_lower, sub.to_lowercase());
        if let Some(&depth) = DEPTH_RULES.get(key.as_str()) {
            return depth;
        }
    }

    if let Some(&depth) = DEPTH_RULES.get(command_lower.as_str()) {
        return depth;
    }

    2
}

// =============================================================================
// 与 TS `readOnlyCommandValidation.ts` 对齐的常量入口 — TS 中是 `const` 表，
// Rust 端用 `once_cell::sync::Lazy` 包装 `build_*_read_only_commands()`。
// =============================================================================

/// 对应 TS `GIT_READ_ONLY_COMMANDS`。
pub static GIT_READ_ONLY_COMMANDS: once_cell::sync::Lazy<HashMap<String, ExternalCommandConfig>> =
    once_cell::sync::Lazy::new(build_git_read_only_commands);
/// 对应 TS `GH_READ_ONLY_COMMANDS`。
pub static GH_READ_ONLY_COMMANDS: once_cell::sync::Lazy<HashMap<String, ExternalCommandConfig>> =
    once_cell::sync::Lazy::new(build_gh_read_only_commands);
/// 对应 TS `DOCKER_READ_ONLY_COMMANDS`。
pub static DOCKER_READ_ONLY_COMMANDS: once_cell::sync::Lazy<
    HashMap<String, ExternalCommandConfig>,
> = once_cell::sync::Lazy::new(build_docker_read_only_commands);
/// 对应 TS `RIPGREP_READ_ONLY_COMMANDS`。
pub static RIPGREP_READ_ONLY_COMMANDS: once_cell::sync::Lazy<
    HashMap<String, ExternalCommandConfig>,
> = once_cell::sync::Lazy::new(build_ripgrep_read_only_commands);

/// pyright read-only command 表 — pyright 没有独立的 read-only 安全子集，
/// 因此返回空表（与 TS `PYRIGHT_READ_ONLY_COMMANDS` 等价）。
pub static PYRIGHT_READ_ONLY_COMMANDS: once_cell::sync::Lazy<
    HashMap<String, ExternalCommandConfig>,
> = once_cell::sync::Lazy::new(HashMap::new);
