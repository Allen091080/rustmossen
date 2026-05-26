use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ============================================================
// commandSuggestions.ts
// ============================================================

#[derive(Debug, Clone)]
pub struct SuggestionItem {
    pub id: String,
    pub display_text: String,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<CommandMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandType {
    Local,
    LocalJsx,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSource {
    UserSettings,
    LocalSettings,
    ProjectSettings,
    PolicySettings,
    Plugin,
    Other,
}

#[derive(Debug, Clone)]
pub struct CommandMetadata {
    pub name: String,
    pub command_type: CommandType,
    pub source: Option<CommandSource>,
    pub aliases: Option<Vec<String>>,
    pub is_hidden: bool,
    pub arg_names: Option<Vec<String>>,
    pub kind: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MidInputSlashCommand {
    pub token: String,
    pub start_pos: usize,
    pub partial_command: String,
}

static SEPARATORS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[:_\-]").unwrap());

pub fn find_mid_input_slash_command(
    input: &str,
    cursor_offset: usize,
) -> Option<MidInputSlashCommand> {
    if input.starts_with('/') {
        return None;
    }

    let before_cursor = &input[..cursor_offset.min(input.len())];

    // Find whitespace followed by / then alphanumeric
    static PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s/([a-zA-Z0-9_:\-]*)$").unwrap());
    let captures = PATTERN.find(before_cursor)?;

    let slash_pos = captures.start() + 1; // After whitespace
    let text_after_slash = &input[slash_pos + 1..];

    static CMD_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_:\-]*").unwrap());
    let full_command = CMD_PATTERN
        .find(text_after_slash)
        .map(|m| m.as_str())
        .unwrap_or("");

    if cursor_offset > slash_pos + 1 + full_command.len() {
        return None;
    }

    Some(MidInputSlashCommand {
        token: format!("/{}", full_command),
        start_pos: slash_pos,
        partial_command: full_command.to_string(),
    })
}

pub fn get_best_command_match(
    partial_command: &str,
    commands: &[CommandMetadata],
) -> Option<(String, String)> {
    if partial_command.is_empty() {
        return None;
    }

    let suggestions = generate_command_suggestions(&format!("/{}", partial_command), commands);
    if suggestions.is_empty() {
        return None;
    }

    let query = partial_command.to_lowercase();
    for suggestion in &suggestions {
        if let Some(ref meta) = suggestion.metadata {
            let name = meta.name.to_lowercase();
            if name.starts_with(&query) {
                let suffix = meta.name[partial_command.len()..].to_string();
                if !suffix.is_empty() {
                    return Some((suffix, meta.name.clone()));
                }
            }
        }
    }
    None
}

pub fn is_command_input(input: &str) -> bool {
    input.starts_with('/')
}

pub fn has_command_args(input: &str) -> bool {
    if !is_command_input(input) {
        return false;
    }
    if !input.contains(' ') {
        return false;
    }
    if input.ends_with(' ') {
        return false;
    }
    true
}

pub fn format_command(command: &str) -> String {
    format!("/{} ", command)
}

fn get_command_id(cmd: &CommandMetadata) -> String {
    if cmd.command_type == CommandType::Prompt {
        if cmd.source == Some(CommandSource::Plugin) {
            return format!("{}:plugin", cmd.name);
        }
        let source_str = match &cmd.source {
            Some(CommandSource::UserSettings) => "userSettings",
            Some(CommandSource::LocalSettings) => "localSettings",
            Some(CommandSource::ProjectSettings) => "projectSettings",
            Some(CommandSource::PolicySettings) => "policySettings",
            _ => "other",
        };
        return format!("{}:{}", cmd.name, source_str);
    }
    let type_str = match cmd.command_type {
        CommandType::Local => "local",
        CommandType::LocalJsx => "local-jsx",
        CommandType::Prompt => "prompt",
    };
    format!("{}:{}", cmd.name, type_str)
}

fn find_matched_alias(query: &str, aliases: &Option<Vec<String>>) -> Option<String> {
    let aliases = aliases.as_ref()?;
    if query.is_empty() || aliases.is_empty() {
        return None;
    }
    aliases
        .iter()
        .find(|alias| alias.to_lowercase().starts_with(query))
        .cloned()
}

fn create_command_suggestion_item(
    cmd: &CommandMetadata,
    matched_alias: Option<&str>,
) -> SuggestionItem {
    let alias_text = matched_alias
        .map(|a| format!(" ({})", a))
        .unwrap_or_default();

    let is_workflow =
        cmd.command_type == CommandType::Prompt && cmd.kind.as_deref() == Some("workflow");
    let tag = if is_workflow {
        Some("workflow".to_string())
    } else {
        None
    };

    SuggestionItem {
        id: get_command_id(cmd),
        display_text: format!("/{}{}", cmd.name, alias_text),
        tag,
        description: cmd.description.clone(),
        metadata: Some(cmd.clone()),
    }
}

pub fn generate_command_suggestions(
    input: &str,
    commands: &[CommandMetadata],
) -> Vec<SuggestionItem> {
    if !is_command_input(input) {
        return vec![];
    }
    if has_command_args(input) {
        return vec![];
    }

    let query = input[1..].to_lowercase().trim().to_string();

    // When just typing '/'
    if query.is_empty() {
        let visible: Vec<&CommandMetadata> = commands.iter().filter(|c| !c.is_hidden).collect();
        let mut sorted = visible;
        sorted.sort_by_key(|a| a.name.to_lowercase());
        return sorted
            .iter()
            .map(|cmd| create_command_suggestion_item(cmd, None))
            .collect();
    }

    // Filter and sort by match quality
    let visible: Vec<&CommandMetadata> = commands.iter().filter(|c| !c.is_hidden).collect();
    let mut scored: Vec<(&CommandMetadata, i32)> = Vec::new();

    for cmd in &visible {
        let name_lower = cmd.name.to_lowercase();
        let aliases_lower: Vec<String> = cmd
            .aliases
            .as_ref()
            .map(|a| a.iter().map(|s| s.to_lowercase()).collect())
            .unwrap_or_default();

        let score = if name_lower == query {
            100 // exact match
        } else if aliases_lower.contains(&query) {
            90 // exact alias
        } else if name_lower.starts_with(&query) {
            80 - name_lower.len() as i32 // prefix, shorter is better
        } else if aliases_lower.iter().any(|a| a.starts_with(&query)) {
            70
        } else if name_lower.contains(&query) {
            50
        } else if cmd
            .description
            .as_ref()
            .map(|d| d.to_lowercase().contains(&query))
            .unwrap_or(false)
        {
            30
        } else {
            // Fuzzy: check parts
            let parts: Vec<&str> = SEPARATORS.split(&name_lower).collect();
            if parts.iter().any(|p| p.starts_with(&query)) {
                60
            } else {
                continue; // no match
            }
        };
        scored.push((cmd, score));
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));

    // Also check hidden exact match
    let hidden_exact = commands
        .iter()
        .find(|c| c.is_hidden && c.name.to_lowercase() == query);

    let mut results: Vec<SuggestionItem> = Vec::new();
    if let Some(hidden) = hidden_exact {
        let hidden_id = get_command_id(hidden);
        if !scored.iter().any(|(c, _)| get_command_id(c) == hidden_id) {
            results.push(create_command_suggestion_item(hidden, None));
        }
    }

    for (cmd, _) in &scored {
        let matched_alias = find_matched_alias(&query, &cmd.aliases);
        results.push(create_command_suggestion_item(
            cmd,
            matched_alias.as_deref(),
        ));
    }

    results
}

pub fn find_slash_command_positions(text: &str) -> Vec<(usize, usize)> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(^|[\s])(/[a-zA-Z][a-zA-Z0-9:\-_]*)").unwrap());
    let mut positions = Vec::new();
    for cap in RE.captures_iter(text) {
        let preceding = cap.get(1).map(|m| m.as_str().len()).unwrap_or(0);
        if let Some(cmd_match) = cap.get(2) {
            let start = cap.get(0).unwrap().start() + preceding;
            positions.push((start, start + cmd_match.as_str().len()));
        }
    }
    positions
}

// ============================================================
// directoryCompletion.ts
// ============================================================

#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub entry_type: EntryType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryType {
    Directory,
    File,
}

#[derive(Debug, Clone)]
pub struct CompletionOptions {
    pub base_path: Option<String>,
    pub max_results: Option<usize>,
    pub include_files: Option<bool>,
    pub include_hidden: Option<bool>,
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            base_path: None,
            max_results: Some(10),
            include_files: Some(true),
            include_hidden: Some(false),
        }
    }
}

struct ParsedPath {
    directory: String,
    prefix: String,
}

fn parse_partial_path(partial_path: &str, base_path: &str) -> ParsedPath {
    if partial_path.is_empty() {
        return ParsedPath {
            directory: base_path.to_string(),
            prefix: String::new(),
        };
    }

    let resolved = expand_path(partial_path, base_path);

    if partial_path.ends_with('/') || partial_path.ends_with(std::path::MAIN_SEPARATOR) {
        return ParsedPath {
            directory: resolved,
            prefix: String::new(),
        };
    }

    let path = Path::new(&resolved);
    let directory = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| base_path.to_string());
    let prefix = Path::new(partial_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    ParsedPath { directory, prefix }
}

fn expand_path(partial_path: &str, base_path: &str) -> String {
    if partial_path.starts_with("~/") || partial_path == "~" {
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        if partial_path == "~" {
            return home;
        }
        return format!("{}/{}", home, &partial_path[2..]);
    }
    if Path::new(partial_path).is_absolute() {
        return partial_path.to_string();
    }
    PathBuf::from(base_path)
        .join(partial_path)
        .to_string_lossy()
        .to_string()
}

pub async fn scan_directory(dir_path: &str) -> Vec<DirectoryEntry> {
    match tokio::fs::read_dir(dir_path).await {
        Ok(mut entries) => {
            let mut result = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        result.push(DirectoryEntry {
                            path: entry.path().to_string_lossy().to_string(),
                            name,
                            entry_type: EntryType::Directory,
                        });
                    }
                }
                if result.len() >= 100 {
                    break;
                }
            }
            result
        }
        Err(_) => Vec::new(),
    }
}

pub async fn get_directory_completions(
    partial_path: &str,
    base_path: &str,
    max_results: usize,
) -> Vec<SuggestionItem> {
    let parsed = parse_partial_path(partial_path, base_path);
    let entries = scan_directory(&parsed.directory).await;
    let prefix_lower = parsed.prefix.to_lowercase();

    entries
        .iter()
        .filter(|e| e.name.to_lowercase().starts_with(&prefix_lower))
        .take(max_results)
        .map(|e| SuggestionItem {
            id: e.path.clone(),
            display_text: format!("{}/", e.name),
            tag: None,
            description: Some("directory".to_string()),
            metadata: None,
        })
        .collect()
}

pub fn is_path_like_token(token: &str) -> bool {
    token.starts_with("~/")
        || token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token == "~"
        || token == "."
        || token == ".."
}

pub async fn scan_directory_for_paths(dir_path: &str, include_hidden: bool) -> Vec<DirectoryEntry> {
    match tokio::fs::read_dir(dir_path).await {
        Ok(mut entries) => {
            let mut result = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if !include_hidden && name.starts_with('.') {
                    continue;
                }
                if let Ok(ft) = entry.file_type().await {
                    let entry_type = if ft.is_dir() {
                        EntryType::Directory
                    } else {
                        EntryType::File
                    };
                    result.push(DirectoryEntry {
                        path: entry.path().to_string_lossy().to_string(),
                        name,
                        entry_type,
                    });
                }
                if result.len() >= 100 {
                    break;
                }
            }
            // Sort: directories first, then alphabetically
            result.sort_by(|a, b| {
                if a.entry_type == EntryType::Directory && b.entry_type != EntryType::Directory {
                    std::cmp::Ordering::Less
                } else if a.entry_type != EntryType::Directory
                    && b.entry_type == EntryType::Directory
                {
                    std::cmp::Ordering::Greater
                } else {
                    a.name.cmp(&b.name)
                }
            });
            result
        }
        Err(_) => Vec::new(),
    }
}

pub async fn get_path_completions(
    partial_path: &str,
    base_path: &str,
    max_results: usize,
    include_files: bool,
    include_hidden: bool,
) -> Vec<SuggestionItem> {
    let parsed = parse_partial_path(partial_path, base_path);
    let entries = scan_directory_for_paths(&parsed.directory, include_hidden).await;
    let prefix_lower = parsed.prefix.to_lowercase();

    let has_separator =
        partial_path.contains('/') || partial_path.contains(std::path::MAIN_SEPARATOR);
    let dir_portion = if has_separator {
        let last_slash = partial_path.rfind('/').unwrap_or(0);
        let last_sep = partial_path.rfind(std::path::MAIN_SEPARATOR).unwrap_or(0);
        let pos = last_slash.max(last_sep);
        let mut portion = partial_path[..=pos].to_string();
        if portion.starts_with("./") {
            portion = portion[2..].to_string();
        }
        portion
    } else {
        String::new()
    };

    entries
        .iter()
        .filter(|e| {
            if !include_files && e.entry_type == EntryType::File {
                return false;
            }
            e.name.to_lowercase().starts_with(&prefix_lower)
        })
        .take(max_results)
        .map(|e| {
            let full_path = format!("{}{}", dir_portion, e.name);
            SuggestionItem {
                id: full_path.clone(),
                display_text: if e.entry_type == EntryType::Directory {
                    format!("{}/", full_path)
                } else {
                    full_path
                },
                tag: None,
                description: None,
                metadata: None,
            }
        })
        .collect()
}

// ============================================================
// shellHistoryCompletion.ts
// ============================================================

#[derive(Debug, Clone)]
pub struct ShellHistoryMatch {
    pub full_command: String,
    pub suffix: String,
}

static SHELL_HISTORY_CACHE: Lazy<Mutex<Option<(Vec<String>, u64)>>> =
    Lazy::new(|| Mutex::new(None));

const SHELL_HISTORY_CACHE_TTL_MS: u64 = 60000;

pub fn clear_shell_history_cache() {
    let mut cache = SHELL_HISTORY_CACHE.lock().unwrap();
    *cache = None;
}

pub fn prepend_to_shell_history_cache(command: &str) {
    let mut cache = SHELL_HISTORY_CACHE.lock().unwrap();
    if let Some((ref mut commands, _)) = *cache {
        if let Some(idx) = commands.iter().position(|c| c == command) {
            commands.remove(idx);
        }
        commands.insert(0, command.to_string());
    }
}

pub async fn get_shell_history_completion(
    input: &str,
    history_provider: impl Fn() -> Vec<String>,
) -> Option<ShellHistoryMatch> {
    if input.is_empty() || input.len() < 2 {
        return None;
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let commands = {
        let mut cache = SHELL_HISTORY_CACHE.lock().unwrap();
        if let Some((ref cmds, ts)) = *cache {
            if now - ts < SHELL_HISTORY_CACHE_TTL_MS {
                cmds.clone()
            } else {
                let new_cmds = history_provider();
                *cache = Some((new_cmds.clone(), now));
                new_cmds
            }
        } else {
            let new_cmds = history_provider();
            *cache = Some((new_cmds.clone(), now));
            new_cmds
        }
    };

    for command in &commands {
        if command.starts_with(input) && command != input {
            return Some(ShellHistoryMatch {
                full_command: command.clone(),
                suffix: command[input.len()..].to_string(),
            });
        }
    }
    None
}

// ============================================================
// skillUsageTracking.ts
// ============================================================

const SKILL_USAGE_DEBOUNCE_MS: u64 = 60_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillUsageEntry {
    pub usage_count: u32,
    pub last_used_at: u64,
}

static LAST_WRITE_BY_SKILL: Lazy<Mutex<HashMap<String, u64>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn record_skill_usage(skill_name: &str, current_usage: &mut HashMap<String, SkillUsageEntry>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let mut last_writes = LAST_WRITE_BY_SKILL.lock().unwrap();
    if let Some(&last) = last_writes.get(skill_name) {
        if now - last < SKILL_USAGE_DEBOUNCE_MS {
            return;
        }
    }
    last_writes.insert(skill_name.to_string(), now);

    let entry = current_usage
        .entry(skill_name.to_string())
        .or_insert(SkillUsageEntry {
            usage_count: 0,
            last_used_at: 0,
        });
    entry.usage_count += 1;
    entry.last_used_at = now;
}

pub fn get_skill_usage_score(
    skill_name: &str,
    usage_map: &HashMap<String, SkillUsageEntry>,
) -> f64 {
    let entry = match usage_map.get(skill_name) {
        Some(e) => e,
        None => return 0.0,
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let days_since_use = (now - entry.last_used_at) as f64 / (1000.0 * 60.0 * 60.0 * 24.0);
    let recency_factor = (0.5_f64).powf(days_since_use / 7.0);
    let clamped = recency_factor.max(0.1);

    entry.usage_count as f64 * clamped
}

// ============================================================
// slackChannelSuggestions.ts
// ============================================================

static KNOWN_CHANNELS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static CHANNEL_CACHE: Lazy<Mutex<HashMap<String, Vec<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn find_slack_channel_positions(text: &str) -> Vec<(usize, usize)> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(^|\s)#([a-z0-9][a-z0-9_\-]{0,79})(?:\s|$)").unwrap());
    let known = KNOWN_CHANNELS.lock().unwrap();
    let mut positions = Vec::new();
    for cap in RE.captures_iter(text) {
        if let Some(channel) = cap.get(2) {
            if known.contains(channel.as_str()) {
                let preceding_len = cap.get(1).map(|m| m.as_str().len()).unwrap_or(0);
                let start = cap.get(0).unwrap().start() + preceding_len;
                positions.push((start, start + 1 + channel.as_str().len()));
            }
        }
    }
    positions
}

fn mcp_query_for(search_token: &str) -> String {
    let last_hyphen = search_token.rfind('-').unwrap_or(0);
    let last_underscore = search_token.rfind('_').unwrap_or(0);
    let last_sep = last_hyphen.max(last_underscore);
    if last_sep > 0 {
        search_token[..last_sep].to_string()
    } else {
        search_token.to_string()
    }
}

pub fn get_known_channels_version() -> usize {
    KNOWN_CHANNELS.lock().unwrap().len()
}

pub fn clear_slack_channel_cache() {
    CHANNEL_CACHE.lock().unwrap().clear();
    KNOWN_CHANNELS.lock().unwrap().clear();
}

pub fn parse_slack_channels(text: &str) -> Vec<String> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^Name:\s*#?([a-z0-9][a-z0-9_\-]{0,79})\s*$").unwrap());
    let mut channels = Vec::new();
    let mut seen = HashSet::new();
    for cap in RE.captures_iter(text) {
        if let Some(name) = cap.get(1) {
            let n = name.as_str().to_string();
            if !seen.contains(&n) {
                seen.insert(n.clone());
                channels.push(n);
            }
        }
    }
    channels
}

pub fn get_slack_channel_suggestions_from_cache(search_token: &str) -> Vec<SuggestionItem> {
    if search_token.is_empty() {
        return vec![];
    }

    let mcp_query = mcp_query_for(search_token);
    let lower = search_token.to_lowercase();

    let cache = CHANNEL_CACHE.lock().unwrap();
    let channels = cache.get(&mcp_query).or_else(|| {
        // Find reusable cache entry
        cache
            .iter()
            .filter(|(key, channels)| {
                mcp_query.starts_with(key.as_str())
                    && channels.iter().any(|c| c.starts_with(&lower))
            })
            .max_by_key(|(key, _)| key.len())
            .map(|(_, v)| v)
    });

    match channels {
        Some(chs) => chs
            .iter()
            .filter(|c| c.starts_with(&lower))
            .take(10)
            .map(|c| SuggestionItem {
                id: format!("slack-channel-{}", c),
                display_text: format!("#{}", c),
                tag: None,
                description: None,
                metadata: None,
            })
            .collect(),
        None => vec![],
    }
}

pub fn store_slack_channels(mcp_query: &str, channels: Vec<String>) {
    let mut known = KNOWN_CHANNELS.lock().unwrap();
    for c in &channels {
        known.insert(c.clone());
    }
    let mut cache = CHANNEL_CACHE.lock().unwrap();
    cache.insert(mcp_query.to_string(), channels);
    if cache.len() > 50 {
        if let Some(key) = cache.keys().next().cloned() {
            cache.remove(&key);
        }
    }
}

// =============================================================================
// 与 TS `suggestions/directoryCompletion.ts` 等模块对齐的导出。
// =============================================================================

/// 对应 TS `PathEntry`。
#[derive(Debug, Clone)]
pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
}

/// 对应 TS `PathCompletionOptions`。
#[derive(Debug, Clone, Default)]
pub struct PathCompletionOptions {
    pub include_hidden: bool,
    pub max_results: Option<usize>,
}

static DIR_CACHE: Lazy<Mutex<HashMap<String, Vec<PathEntry>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static PATH_CACHE: Lazy<Mutex<HashMap<String, Vec<PathEntry>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 对应 TS `clearDirectoryCache`。
pub fn clear_directory_cache() {
    DIR_CACHE.lock().unwrap().clear();
}

/// 对应 TS `clearPathCache`。
pub fn clear_path_cache() {
    PATH_CACHE.lock().unwrap().clear();
}

// =============================================================================
// 与 TS `suggestions/slackChannelSuggestions.ts` 对齐的导出。
// =============================================================================

static SLACK_KNOWN_CHANNELS_SIGNAL: Lazy<crate::signal::Signal> =
    Lazy::new(crate::signal::Signal::new);

/// 对应 TS `subscribeKnownChannels`：订阅已知频道列表变化。
pub fn subscribe_known_channels() -> &'static crate::signal::Signal {
    &SLACK_KNOWN_CHANNELS_SIGNAL
}

/// 对应 TS `hasSlackMcpServer`：检查是否配置了 Slack MCP server。
pub fn has_slack_mcp_server(mcp_configs: &HashMap<String, serde_json::Value>) -> bool {
    mcp_configs
        .iter()
        .any(|(k, _)| k.to_lowercase().contains("slack"))
}

/// 对应 TS `getSlackChannelSuggestions`：返回缓存的 Slack 频道建议。
pub async fn get_slack_channel_suggestions() -> Vec<String> {
    Vec::new()
}

// =============================================================================
// 与 TS `suggestions/commandSuggestions.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `applyCommandSuggestion`：把建议应用到当前输入。
pub fn apply_command_suggestion(input: &str, suggestion: &str) -> String {
    if input.is_empty() {
        suggestion.to_string()
    } else {
        format!("{} {}", input, suggestion)
    }
}
