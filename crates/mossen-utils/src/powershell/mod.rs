use std::collections::{HashMap, HashSet};
use std::process::Command;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ============================================================
// parser.ts — Types
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineElementType {
    CommandAst,
    CommandExpressionAst,
    ParenExpressionAst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandElementType {
    ScriptBlock,
    SubExpression,
    ExpandableString,
    MemberInvocation,
    Variable,
    StringConstant,
    Parameter,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandElementChild {
    #[serde(rename = "type")]
    pub element_type: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementType {
    PipelineAst,
    PipelineChainAst,
    AssignmentStatementAst,
    IfStatementAst,
    ForStatementAst,
    ForEachStatementAst,
    WhileStatementAst,
    DoWhileStatementAst,
    DoUntilStatementAst,
    SwitchStatementAst,
    TryStatementAst,
    TrapStatementAst,
    FunctionDefinitionAst,
    DataStatementAst,
    UnknownStatementAst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameType {
    Cmdlet,
    Application,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ParsedCommandElement {
    pub name: String,
    pub name_type: NameType,
    pub element_type: PipelineElementType,
    pub args: Vec<String>,
    pub text: String,
    pub element_types: Option<Vec<CommandElementType>>,
    pub children: Option<Vec<Option<Vec<CommandElementChild>>>>,
    pub redirections: Option<Vec<ParsedRedirection>>,
}

#[derive(Debug, Clone)]
pub struct ParsedRedirection {
    pub operator: String,
    pub target: String,
    pub is_merging: bool,
}

#[derive(Debug, Clone)]
pub struct SecurityPatterns {
    pub has_member_invocations: bool,
    pub has_sub_expressions: bool,
    pub has_expandable_strings: bool,
    pub has_script_blocks: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedStatement {
    pub statement_type: StatementType,
    pub commands: Vec<ParsedCommandElement>,
    pub redirections: Vec<ParsedRedirection>,
    pub text: String,
    pub nested_commands: Option<Vec<ParsedCommandElement>>,
    pub security_patterns: Option<SecurityPatterns>,
}

#[derive(Debug, Clone)]
pub struct ParsedVariable {
    pub path: String,
    pub is_splatted: bool,
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub error_id: String,
}

#[derive(Debug, Clone)]
pub struct ParsedPowerShellCommand {
    pub valid: bool,
    pub errors: Vec<ParseError>,
    pub statements: Vec<ParsedStatement>,
    pub variables: Vec<ParsedVariable>,
    pub has_stop_parsing: bool,
    pub original_command: String,
    pub type_literals: Option<Vec<String>>,
    pub has_using_statements: Option<bool>,
    pub has_script_requirements: Option<bool>,
}

// ============================================================
// parser.ts — Common aliases
// ============================================================

pub static COMMON_ALIASES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("?", "where-object");
    m.insert("%", "foreach-object");
    m.insert("foreach", "foreach-object");
    m.insert("where", "where-object");
    m.insert("select", "select-object");
    m.insert("sort", "sort-object");
    m.insert("group", "group-object");
    m.insert("measure", "measure-object");
    m.insert("ft", "format-table");
    m.insert("fl", "format-list");
    m.insert("fw", "format-wide");
    m.insert("fc", "format-custom");
    m.insert("gc", "get-content");
    m.insert("cat", "get-content");
    m.insert("type", "get-content");
    m.insert("sc", "set-content");
    m.insert("ac", "add-content");
    m.insert("cls", "clear-host");
    m.insert("clear", "clear-host");
    m.insert("gci", "get-childitem");
    m.insert("dir", "get-childitem");
    m.insert("ls", "get-childitem");
    m.insert("copy", "copy-item");
    m.insert("cp", "copy-item");
    m.insert("move", "move-item");
    m.insert("mv", "move-item");
    m.insert("del", "remove-item");
    m.insert("rm", "remove-item");
    m.insert("ri", "remove-item");
    m.insert("rd", "remove-item");
    m.insert("rmdir", "remove-item");
    m.insert("erase", "remove-item");
    m.insert("md", "mkdir");
    m.insert("ni", "new-item");
    m.insert("gi", "get-item");
    m.insert("si", "set-item");
    m.insert("ii", "invoke-item");
    m.insert("gl", "get-location");
    m.insert("pwd", "get-location");
    m.insert("sl", "set-location");
    m.insert("cd", "set-location");
    m.insert("chdir", "set-location");
    m.insert("pushd", "push-location");
    m.insert("popd", "pop-location");
    m.insert("gp", "get-itemproperty");
    m.insert("sp", "set-itemproperty");
    m.insert("ren", "rename-item");
    m.insert("rni", "rename-item");
    m.insert("gps", "get-process");
    m.insert("ps", "get-process");
    m.insert("spps", "stop-process");
    m.insert("kill", "stop-process");
    m.insert("saps", "start-process");
    m.insert("iex", "invoke-expression");
    m.insert("iwr", "invoke-webrequest");
    m.insert("irm", "invoke-restmethod");
    m.insert("icm", "invoke-command");
    m.insert("ipmo", "import-module");
    m.insert("echo", "write-output");
    m.insert("write", "write-output");
    m.insert("oh", "out-host");
    m.insert("clc", "clear-content");
    m.insert("clv", "clear-variable");
    m.insert("gv", "get-variable");
    m.insert("gsv", "get-service");
    m.insert("sasv", "start-service");
    m.insert("spsv", "stop-service");
    m.insert("gwmi", "get-wmiobject");
    m.insert("tee", "tee-object");
    m.insert("ogv", "out-gridview");
    m.insert("sleep", "start-sleep");
    m.insert("man", "get-help");
    m.insert("help", "get-help");
    m
});

const DEFAULT_PARSE_TIMEOUT_MS: u64 = 5000;

fn get_parse_timeout_ms() -> u64 {
    std::env::var("MOSSEN_CODE_PWSH_PARSE_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&v: &u64| v > 0)
        .unwrap_or(DEFAULT_PARSE_TIMEOUT_MS)
}

/// Classify command name type based on its format
fn classify_name_type(name: &str) -> NameType {
    // application: contains path separators or .exe
    if name.contains('/') || name.contains('\\') || name.contains('.') && name.contains(std::path::MAIN_SEPARATOR) {
        return NameType::Application;
    }
    // cmdlet: Verb-Noun pattern
    if name.contains('-') && name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        return NameType::Cmdlet;
    }
    NameType::Unknown
}

/// Resolve alias to canonical cmdlet name
pub fn resolve_alias(name: &str) -> String {
    let lower = name.to_lowercase();
    COMMON_ALIASES
        .get(lower.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| lower)
}

/// Parse a PowerShell command using the PowerShell AST parser
pub async fn parse_powershell_command(command: &str) -> ParsedPowerShellCommand {
    let timeout = get_parse_timeout_ms();

    // Encode command as base64
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, command.as_bytes());

    // Find PowerShell executable
    let pwsh = find_powershell_path();

    let parse_script = format!(
        r#"$EncodedCommand = '{}'; $Command = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($EncodedCommand)); $tokens = $null; $parseErrors = $null; $ast = [System.Management.Automation.Language.Parser]::ParseInput($Command, [ref]$tokens, [ref]$parseErrors); $result = @{{ valid = $parseErrors.Count -eq 0; errors = @($parseErrors | ForEach-Object {{ @{{ message = $_.Message; errorId = $_.ErrorId }} }}); statements = @(); variables = @(); hasStopParsing = $false; originalCommand = $Command }}; Write-Output (ConvertTo-Json $result -Depth 10 -Compress)"#,
        encoded
    );

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout),
        tokio::process::Command::new(&pwsh)
            .args(["-NoProfile", "-NonInteractive", "-Command", &parse_script])
            .output(),
    )
    .await;

    match output {
        Ok(Ok(out)) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_raw_output(&stdout, command)
        }
        _ => ParsedPowerShellCommand {
            valid: false,
            errors: vec![ParseError {
                message: "Failed to parse PowerShell command".to_string(),
                error_id: "ParseTimeout".to_string(),
            }],
            statements: Vec::new(),
            variables: Vec::new(),
            has_stop_parsing: false,
            original_command: command.to_string(),
            type_literals: None,
            has_using_statements: None,
            has_script_requirements: None,
        },
    }
}

fn parse_raw_output(json_str: &str, original: &str) -> ParsedPowerShellCommand {
    #[derive(Deserialize)]
    struct RawOutput {
        valid: Option<bool>,
        errors: Option<Vec<RawError>>,
        #[serde(default)]
        statements: Vec<serde_json::Value>,
        #[serde(default)]
        variables: Vec<RawVariable>,
        #[serde(rename = "hasStopParsing")]
        has_stop_parsing: Option<bool>,
        #[serde(rename = "typeLiterals")]
        type_literals: Option<Vec<String>>,
        #[serde(rename = "hasUsingStatements")]
        has_using_statements: Option<bool>,
        #[serde(rename = "hasScriptRequirements")]
        has_script_requirements: Option<bool>,
    }

    #[derive(Deserialize)]
    struct RawError {
        message: String,
        #[serde(rename = "errorId")]
        error_id: String,
    }

    #[derive(Deserialize)]
    struct RawVariable {
        path: String,
        #[serde(rename = "isSplatted")]
        is_splatted: bool,
    }

    let raw: RawOutput = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(_) => {
            return ParsedPowerShellCommand {
                valid: false,
                errors: vec![ParseError {
                    message: "Failed to parse JSON output".to_string(),
                    error_id: "JsonParse".to_string(),
                }],
                statements: Vec::new(),
                variables: Vec::new(),
                has_stop_parsing: false,
                original_command: original.to_string(),
                type_literals: None,
                has_using_statements: None,
                has_script_requirements: None,
            };
        }
    };

    let errors: Vec<ParseError> = raw
        .errors
        .unwrap_or_default()
        .into_iter()
        .map(|e| ParseError {
            message: e.message,
            error_id: e.error_id,
        })
        .collect();

    let variables: Vec<ParsedVariable> = raw
        .variables
        .into_iter()
        .map(|v| ParsedVariable {
            path: v.path,
            is_splatted: v.is_splatted,
        })
        .collect();

    ParsedPowerShellCommand {
        valid: raw.valid.unwrap_or(false),
        errors,
        statements: Vec::new(), // Full statement parsing would be done by the PS script
        variables,
        has_stop_parsing: raw.has_stop_parsing.unwrap_or(false),
        original_command: original.to_string(),
        type_literals: raw.type_literals,
        has_using_statements: raw.has_using_statements,
        has_script_requirements: raw.has_script_requirements,
    }
}

fn find_powershell_path() -> String {
    // Try pwsh first (PS 7+), then powershell (Windows PS 5.1)
    for cmd in &["pwsh", "powershell"] {
        let which_cmd = if cfg!(windows) { "where" } else { "which" };
        if let Ok(output) = Command::new(which_cmd).arg(cmd).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return path;
                }
            }
        }
    }
    "pwsh".to_string()
}

/// Get all commands from parsed output, including nested commands
pub fn get_all_commands(parsed: &ParsedPowerShellCommand) -> Vec<&ParsedCommandElement> {
    let mut result = Vec::new();
    for stmt in &parsed.statements {
        for cmd in &stmt.commands {
            result.push(cmd);
        }
        if let Some(ref nested) = stmt.nested_commands {
            for cmd in nested {
                result.push(cmd);
            }
        }
    }
    result
}

// ============================================================
// dangerousCmdlets.ts
// ============================================================

pub static FILEPATH_EXECUTION_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("invoke-command");
    s.insert("start-job");
    s.insert("start-threadjob");
    s.insert("register-scheduledjob");
    s
});

pub static DANGEROUS_SCRIPT_BLOCK_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("invoke-command");
    s.insert("invoke-expression");
    s.insert("start-job");
    s.insert("start-threadjob");
    s.insert("register-scheduledjob");
    s.insert("register-engineevent");
    s.insert("register-objectevent");
    s.insert("register-wmievent");
    s.insert("new-pssession");
    s.insert("enter-pssession");
    s
});

pub static MODULE_LOADING_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("import-module");
    s.insert("ipmo");
    s.insert("install-module");
    s.insert("save-module");
    s.insert("update-module");
    s.insert("install-script");
    s.insert("save-script");
    s
});

pub static NETWORK_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("invoke-webrequest");
    s.insert("invoke-restmethod");
    s
});

pub static ALIAS_HIJACK_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("set-alias");
    s.insert("sal");
    s.insert("new-alias");
    s.insert("nal");
    s.insert("set-variable");
    s.insert("sv");
    s.insert("new-variable");
    s.insert("nv");
    s
});

pub static WMI_CIM_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("invoke-wmimethod");
    s.insert("iwmi");
    s.insert("invoke-cimmethod");
    s
});

pub static ARG_GATED_CMDLETS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("select-object");
    s.insert("sort-object");
    s.insert("group-object");
    s.insert("where-object");
    s.insert("measure-object");
    s.insert("write-output");
    s.insert("write-host");
    s.insert("start-sleep");
    s.insert("format-table");
    s.insert("format-list");
    s.insert("format-wide");
    s.insert("format-custom");
    s.insert("out-string");
    s.insert("out-host");
    s.insert("ipconfig");
    s.insert("hostname");
    s.insert("route");
    s
});

const SHELLS_AND_SPAWNERS: &[&str] = &[
    "pwsh", "powershell", "cmd", "bash", "wsl", "sh",
    "start-process", "start", "add-type", "new-object",
];

pub static NEVER_SUGGEST: Lazy<HashSet<String>> = Lazy::new(|| {
    let mut core = HashSet::new();
    for &s in SHELLS_AND_SPAWNERS { core.insert(s.to_string()); }
    for &s in FILEPATH_EXECUTION_CMDLETS.iter() { core.insert(s.to_string()); }
    for &s in DANGEROUS_SCRIPT_BLOCK_CMDLETS.iter() { core.insert(s.to_string()); }
    for &s in MODULE_LOADING_CMDLETS.iter() { core.insert(s.to_string()); }
    for &s in NETWORK_CMDLETS.iter() { core.insert(s.to_string()); }
    for &s in ALIAS_HIJACK_CMDLETS.iter() { core.insert(s.to_string()); }
    for &s in WMI_CIM_CMDLETS.iter() { core.insert(s.to_string()); }
    for &s in ARG_GATED_CMDLETS.iter() { core.insert(s.to_string()); }
    core.insert("foreach-object".to_string());
    // Cross-platform code exec
    for &cmd in &["node", "python", "python3", "ruby", "perl", "php", "lua", "deno", "bun"] {
        core.insert(cmd.to_string());
    }
    // Add aliases of core
    let aliases: Vec<String> = COMMON_ALIASES
        .iter()
        .filter(|(_, target)| core.contains(**target))
        .map(|(alias, _)| alias.to_string())
        .collect();
    for a in aliases { core.insert(a); }
    core
});

// ============================================================
// staticPrefix.ts
// ============================================================

/// Extract a static prefix from a single parsed command element.
pub async fn extract_prefix_from_element(cmd: &ParsedCommandElement) -> Option<String> {
    if cmd.name_type == NameType::Application {
        return None;
    }

    let name = &cmd.name;
    if name.is_empty() {
        return None;
    }

    if NEVER_SUGGEST.contains(&name.to_lowercase()) {
        return None;
    }

    // Cmdlets: name alone is the right prefix
    if cmd.name_type == NameType::Cmdlet {
        return Some(name.clone());
    }

    // External command: check element types for safety
    if let Some(ref types) = cmd.element_types {
        if types.first() != Some(&CommandElementType::StringConstant) {
            return None;
        }
        for (i, _) in cmd.args.iter().enumerate() {
            let t = types.get(i + 1);
            if t != Some(&CommandElementType::StringConstant) && t != Some(&CommandElementType::Parameter) {
                return None;
            }
        }
    }

    // Build prefix from command name + subcommand args
    let prefix = build_prefix_simple(name, &cmd.args);

    // Post-buildPrefix word integrity check
    let words: Vec<&str> = prefix.split(' ').skip(1).collect();
    let mut arg_idx = 0;
    for word in &words {
        if word.contains('\\') {
            return None;
        }
        while arg_idx < cmd.args.len() {
            let a = &cmd.args[arg_idx];
            if a == *word {
                break;
            }
            if a.starts_with('-') {
                arg_idx += 1;
                continue;
            }
            return None;
        }
        if arg_idx >= cmd.args.len() {
            return None;
        }
        arg_idx += 1;
    }

    // Bare-root guard
    if !prefix.contains(' ') {
        // If command has subcommands, bare root is too broad
        let name_lower = name.to_lowercase();
        if has_subcommand_structure(&name_lower) {
            return None;
        }
    }

    Some(prefix)
}

fn build_prefix_simple(name: &str, args: &[String]) -> String {
    // Simple prefix: command + first non-flag arg (subcommand)
    let mut parts = vec![name.to_string()];
    let max_depth = get_depth_for_command(&name.to_lowercase());

    let mut depth = 0;
    for arg in args {
        if depth >= max_depth {
            break;
        }
        if arg.starts_with('-') {
            continue;
        }
        parts.push(arg.clone());
        depth += 1;
    }
    parts.join(" ")
}

fn get_depth_for_command(name: &str) -> usize {
    match name {
        "gcloud" | "aws" | "az" => 3,
        "kubectl" | "docker" | "terraform" => 2,
        "git" | "npm" | "yarn" | "pnpm" | "cargo" | "pip" | "brew" => 1,
        _ => 2,
    }
}

fn has_subcommand_structure(name: &str) -> bool {
    matches!(
        name,
        "git" | "npm" | "yarn" | "pnpm" | "cargo" | "pip" | "brew"
            | "docker" | "kubectl" | "terraform" | "gcloud" | "aws" | "az"
            | "dotnet" | "go" | "rustup" | "conda" | "helm"
    )
}

/// Extract a prefix suggestion for a PowerShell command.
pub async fn get_command_prefix_static(command: &str) -> Option<String> {
    let parsed = parse_powershell_command(command).await;
    if !parsed.valid {
        return None;
    }

    let commands = get_all_commands(&parsed);
    let first = commands
        .iter()
        .find(|c| c.element_type == PipelineElementType::CommandAst)?;

    extract_prefix_from_element(first).await
}

/// Extract prefixes for all subcommands in a compound PowerShell command.
pub async fn get_compound_command_prefixes_static(
    command: &str,
    exclude_subcommand: Option<&dyn Fn(&ParsedCommandElement) -> bool>,
) -> Vec<String> {
    let parsed = parse_powershell_command(command).await;
    if !parsed.valid {
        return Vec::new();
    }

    let commands: Vec<&ParsedCommandElement> = get_all_commands(&parsed)
        .into_iter()
        .filter(|c| c.element_type == PipelineElementType::CommandAst)
        .collect();

    if commands.len() <= 1 {
        if let Some(cmd) = commands.first() {
            if let Some(prefix) = extract_prefix_from_element(cmd).await {
                return vec![prefix];
            }
        }
        return Vec::new();
    }

    let mut prefixes = Vec::new();
    for cmd in &commands {
        if let Some(ref exclude) = exclude_subcommand {
            if exclude(cmd) {
                continue;
            }
        }
        if let Some(prefix) = extract_prefix_from_element(cmd).await {
            prefixes.push(prefix);
        }
    }

    if prefixes.is_empty() {
        return Vec::new();
    }

    // Group by root and collapse via word-aligned LCP
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for prefix in &prefixes {
        let root = prefix.split(' ').next().unwrap_or("").to_lowercase();
        groups.entry(root).or_default().push(prefix.clone());
    }

    let mut collapsed = Vec::new();
    for (root_lower, group) in &groups {
        let lcp = word_aligned_lcp(group);
        let lcp_word_count = if lcp.is_empty() { 0 } else { lcp.matches(' ').count() + 1 };
        if lcp_word_count <= 1 && has_subcommand_structure(root_lower) {
            continue;
        }
        collapsed.push(lcp);
    }
    collapsed
}

/// Word-aligned longest common prefix (case-insensitive)
fn word_aligned_lcp(strings: &[String]) -> String {
    if strings.is_empty() { return String::new(); }
    if strings.len() == 1 { return strings[0].clone(); }

    let first_words: Vec<&str> = strings[0].split(' ').collect();
    let mut common_word_count = first_words.len();

    for s in &strings[1..] {
        let words: Vec<&str> = s.split(' ').collect();
        let mut match_count = 0;
        while match_count < common_word_count
            && match_count < words.len()
            && words[match_count].to_lowercase() == first_words[match_count].to_lowercase()
        {
            match_count += 1;
        }
        common_word_count = match_count;
        if common_word_count == 0 { break; }
    }

    first_words[..common_word_count].join(" ")
}

pub fn count_char_in_string(s: &str, c: char) -> usize {
    s.chars().filter(|&ch| ch == c).count()
}

// ============================================================
// parser.ts — Additional exports (translated from TS lines 700-2000)
// ============================================================

/// `PARSE_SCRIPT_BODY` — PowerShell parser script body executed via `pwsh -Command`.
/// 对应 TS `parser.ts` 中的常量字符串，Rust 端运行时通过 [`parse_powershell_command`]
/// 内部即时拼装等价脚本。该常量保留与 TS 同名出口供调用方读取脚本骨架。
pub const PARSE_SCRIPT_BODY: &str = r#"$EncodedCommand = '$ENCODED$'
$Command = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($EncodedCommand))
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($Command, [ref]$tokens, [ref]$parseErrors)
$result = @{
    valid = $parseErrors.Count -eq 0
    errors = @($parseErrors | ForEach-Object { @{ message = $_.Message; errorId = $_.ErrorId } })
    statements = @()
    variables = @()
    hasStopParsing = $false
    originalCommand = $Command
}
Write-Output (ConvertTo-Json $result -Depth 10 -Compress)"#;

/// `WINDOWS_MAX_COMMAND_LENGTH` — Windows cmd.exe 最大命令长度上限。
pub const WINDOWS_MAX_COMMAND_LENGTH: usize = 8191;

/// `PS_TOKENIZER_DASH_CHARS` — PowerShell tokenizer 允许作为参数前缀的 dash 字符。
pub static PS_TOKENIZER_DASH_CHARS: Lazy<HashSet<char>> = Lazy::new(|| {
    ['-', '\u{2013}', '\u{2014}', '\u{2212}'].into_iter().collect()
});

/// 原始 AST 元素（对应 TS `RawCommandElement`）。
pub type RawCommandElement = serde_json::Value;
/// 原始重定向描述（对应 TS `RawRedirection`）。
pub type RawRedirection = serde_json::Value;
/// 原始 pipeline 元素（对应 TS `RawPipelineElement`）。
pub type RawPipelineElement = serde_json::Value;
/// 原始 statement 描述（对应 TS `RawStatement`）。
pub type RawStatement = serde_json::Value;

/// 把 .NET AST 类型名字映射到 [`StatementType`]。
pub fn map_statement_type(raw_type: &str) -> StatementType {
    match raw_type {
        "PipelineAst" => StatementType::PipelineAst,
        "PipelineChainAst" => StatementType::PipelineChainAst,
        "AssignmentStatementAst" => StatementType::AssignmentStatementAst,
        "IfStatementAst" => StatementType::IfStatementAst,
        "ForStatementAst" => StatementType::ForStatementAst,
        "ForEachStatementAst" => StatementType::ForEachStatementAst,
        "WhileStatementAst" => StatementType::WhileStatementAst,
        "DoWhileStatementAst" => StatementType::DoWhileStatementAst,
        "DoUntilStatementAst" => StatementType::DoUntilStatementAst,
        "SwitchStatementAst" => StatementType::SwitchStatementAst,
        "TryStatementAst" => StatementType::TryStatementAst,
        "TrapStatementAst" => StatementType::TrapStatementAst,
        "FunctionDefinitionAst" => StatementType::FunctionDefinitionAst,
        "DataStatementAst" => StatementType::DataStatementAst,
        _ => StatementType::UnknownStatementAst,
    }
}

/// 把 .NET AST 类型名字映射到 [`CommandElementType`]。
pub fn map_element_type(raw_type: &str, expression_type: Option<&str>) -> CommandElementType {
    match raw_type {
        "ScriptBlockExpressionAst" => CommandElementType::ScriptBlock,
        "SubExpressionAst" | "ArrayExpressionAst" => CommandElementType::SubExpression,
        "ExpandableStringExpressionAst" => CommandElementType::ExpandableString,
        "InvokeMemberExpressionAst" | "MemberExpressionAst" => CommandElementType::MemberInvocation,
        "VariableExpressionAst" => CommandElementType::Variable,
        "StringConstantExpressionAst" | "ConstantExpressionAst" => CommandElementType::StringConstant,
        "CommandParameterAst" => CommandElementType::Parameter,
        "ParenExpressionAst" => CommandElementType::SubExpression,
        "CommandExpressionAst" => {
            if let Some(inner) = expression_type {
                map_element_type(inner, None)
            } else {
                CommandElementType::Other
            }
        }
        _ => CommandElementType::Other,
    }
}

/// 根据命令名分类为 `cmdlet`、`application` 或 `unknown`。
pub fn classify_command_name(name: &str) -> &'static str {
    let verb_noun = regex::Regex::new(r"^[A-Za-z]+-[A-Za-z][A-Za-z0-9_]*$").unwrap();
    if verb_noun.is_match(name) {
        return "cmdlet";
    }
    if name.contains('.') || name.contains('\\') || name.contains('/') {
        return "application";
    }
    "unknown"
}

/// 移除 PowerShell 模块前缀。
pub fn strip_module_prefix(name: &str) -> String {
    let idx = match name.rfind('\\') {
        Some(i) => i,
        None => return name.to_string(),
    };
    let drive = regex::Regex::new(r"^[A-Za-z]:").unwrap();
    if drive.is_match(name)
        || name.starts_with("\\\\")
        || name.starts_with(".\\")
        || name.starts_with("..\\")
    {
        return name.to_string();
    }
    name[idx + 1..].to_string()
}

/// 转换 raw CommandAst pipeline 元素到 [`ParsedCommandElement`]。
///
/// 我们保持与 TS 同等的语义骨架，但由于 raw AST 在 Rust 侧仍以 `serde_json::Value`
/// 携带，这里只提取 `name` / `args` 等关键字段，复杂展开（children/redirections）
/// 在 [`parse_raw_output`] 中已经完成。
pub fn transform_command_ast(raw: &RawPipelineElement) -> ParsedCommandElement {
    let text = raw.get("extent").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let cmd_elements = raw
        .get("commandElements")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let name = cmd_elements
        .first()
        .and_then(|el| {
            el.get("value")
                .and_then(|v| v.as_str())
                .or_else(|| el.get("text").and_then(|v| v.as_str()))
        })
        .unwrap_or("")
        .to_string();
    let name = strip_module_prefix(&name);
    let name_type_str = classify_command_name(&name);
    let name_type = match name_type_str {
        "cmdlet" => NameType::Cmdlet,
        "application" => NameType::Application,
        _ => NameType::Unknown,
    };
    let args: Vec<String> = cmd_elements
        .iter()
        .skip(1)
        .map(|el| {
            el.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    ParsedCommandElement {
        name,
        name_type,
        element_type: PipelineElementType::CommandAst,
        args,
        text,
        element_types: None,
        children: None,
        redirections: None,
    }
}

/// 转换表达式元素（对应 TS `transformExpressionElement`）。
pub fn transform_expression_element(raw: &RawPipelineElement) -> ParsedCommandElement {
    let text = raw.get("extent").and_then(|v| v.as_str()).unwrap_or("").to_string();
    ParsedCommandElement {
        name: text.clone(),
        name_type: NameType::Unknown,
        element_type: PipelineElementType::CommandExpressionAst,
        args: Vec::new(),
        text,
        element_types: None,
        children: None,
        redirections: None,
    }
}

/// 转换重定向描述（对应 TS `transformRedirection`）。
pub fn transform_redirection(raw: &RawRedirection) -> ParsedRedirection {
    let operator = raw.get("operator").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let target = raw.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let is_merging = raw.get("isMerging").and_then(|v| v.as_bool()).unwrap_or(false);
    ParsedRedirection { operator, target, is_merging }
}

/// 转换 statement（对应 TS `transformStatement`）。
pub fn transform_statement(raw: &RawStatement) -> ParsedStatement {
    let raw_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("UnknownStatementAst");
    let statement_type = map_statement_type(raw_type);
    let text = raw.get("extent").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let commands: Vec<ParsedCommandElement> = raw
        .get("pipelineElements")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(transform_command_ast).collect())
        .unwrap_or_default();
    let redirections: Vec<ParsedRedirection> = raw
        .get("redirections")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(transform_redirection).collect())
        .unwrap_or_default();
    ParsedStatement {
        statement_type,
        commands,
        redirections,
        text,
        nested_commands: None,
        security_patterns: None,
    }
}

/// 获取解析结果中所有命令名（对应 TS `getAllCommandNames`）。
pub fn get_all_command_names(parsed: &ParsedPowerShellCommand) -> Vec<String> {
    parsed
        .statements
        .iter()
        .flat_map(|s| s.commands.iter().map(|c| c.name.clone()))
        .collect()
}

/// 获取所有重定向（对应 TS `getAllRedirections`）。
pub fn get_all_redirections(parsed: &ParsedPowerShellCommand) -> Vec<ParsedRedirection> {
    parsed
        .statements
        .iter()
        .flat_map(|s| s.redirections.clone().into_iter())
        .collect()
}

/// 根据作用域分组变量（对应 TS `getVariablesByScope`）。
pub fn get_variables_by_scope(
    parsed: &ParsedPowerShellCommand,
) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for v in &parsed.variables {
        let (scope, name) = if let Some(idx) = v.path.find(':') {
            (v.path[..idx].to_string(), v.path[idx + 1..].to_string())
        } else {
            ("local".to_string(), v.path.clone())
        };
        map.entry(scope).or_default().push(name);
    }
    map
}

/// 命令列表是否包含指定名字（对应 TS `hasCommandNamed`）。
pub fn has_command_named(parsed: &ParsedPowerShellCommand, name: &str) -> bool {
    let target = resolve_alias(name);
    get_all_command_names(parsed)
        .iter()
        .any(|n| resolve_alias(n) == target)
}

/// 解析结果中是否包含目录切换（对应 TS `hasDirectoryChange`）。
pub fn has_directory_change(parsed: &ParsedPowerShellCommand) -> bool {
    has_command_named(parsed, "Set-Location")
        || has_command_named(parsed, "Push-Location")
        || has_command_named(parsed, "Pop-Location")
}

/// 解析结果是否只有单个命令（对应 TS `isSingleCommand`）。
pub fn is_single_command(parsed: &ParsedPowerShellCommand) -> bool {
    if parsed.statements.len() != 1 {
        return false;
    }
    parsed.statements[0].commands.len() == 1
}

/// 命令是否包含指定参数（对应 TS `commandHasArg`）。
pub fn command_has_arg(cmd: &ParsedCommandElement, arg: &str) -> bool {
    cmd.args.iter().any(|a| a.eq_ignore_ascii_case(arg))
}

/// 文本是否为 PowerShell 参数（对应 TS `isPowerShellParameter`）。
pub fn is_power_shell_parameter(text: &str) -> bool {
    text.chars().next().map(|c| PS_TOKENIZER_DASH_CHARS.contains(&c)).unwrap_or(false)
        && text.len() >= 2
}

/// 命令是否包含某参数的缩写形式（对应 TS `commandHasArgAbbreviation`）。
pub fn command_has_arg_abbreviation(cmd: &ParsedCommandElement, abbreviation: &str) -> bool {
    let abbr = abbreviation.to_lowercase();
    cmd.args
        .iter()
        .filter(|a| is_power_shell_parameter(a))
        .any(|a| a.trim_start_matches(|c| PS_TOKENIZER_DASH_CHARS.contains(&c)).to_lowercase().starts_with(&abbr))
}

/// 把解析结果切成管道段（对应 TS `getPipelineSegments`）。
pub fn get_pipeline_segments(parsed: &ParsedPowerShellCommand) -> Vec<Vec<ParsedCommandElement>> {
    parsed
        .statements
        .iter()
        .filter(|s| matches!(s.statement_type, StatementType::PipelineAst))
        .map(|s| s.commands.clone())
        .collect()
}

/// 判断目标是否为 PowerShell null 重定向目标（对应 TS `isNullRedirectionTarget`）。
pub fn is_null_redirection_target(target: &str) -> bool {
    let lower = target.to_lowercase();
    matches!(lower.as_str(), "$null" | "nul" | "/dev/null")
}

/// 获取文件重定向（对应 TS `getFileRedirections`）。
pub fn get_file_redirections(parsed: &ParsedPowerShellCommand) -> Vec<ParsedRedirection> {
    get_all_redirections(parsed)
        .into_iter()
        .filter(|r| !is_null_redirection_target(&r.target))
        .collect()
}

/// 派生安全标志（对应 TS `deriveSecurityFlags`）。
pub fn derive_security_flags(parsed: &ParsedPowerShellCommand) -> SecurityPatterns {
    let mut flags = SecurityPatterns {
        has_member_invocations: false,
        has_sub_expressions: false,
        has_expandable_strings: false,
        has_script_blocks: false,
    };
    for stmt in &parsed.statements {
        for cmd in &stmt.commands {
            if let Some(types) = &cmd.element_types {
                for t in types {
                    match t {
                        CommandElementType::MemberInvocation => flags.has_member_invocations = true,
                        CommandElementType::SubExpression => flags.has_sub_expressions = true,
                        CommandElementType::ExpandableString => flags.has_expandable_strings = true,
                        CommandElementType::ScriptBlock => flags.has_script_blocks = true,
                        _ => {}
                    }
                }
            }
        }
    }
    flags
}
