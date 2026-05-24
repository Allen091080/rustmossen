//! Command prefix detection for permission system.
//!
//! Translated from `prefix.ts` (205 lines).

use regex::Regex;
use std::collections::HashSet;

use crate::bash::commands::split_command_deprecated;
use crate::bash::parser_interface::{extract_command_arguments, parse_command};
use crate::bash::registry::get_command_spec;

lazy_static::lazy_static! {
    static ref NUMERIC_RE: Regex = Regex::new(r"^\d+$").unwrap();
    static ref ENV_VAR_RE: Regex = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*=").unwrap();
    static ref WRAPPER_COMMANDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("nice");
        s
    };
}

fn to_vec<'a>(
    args: &'a [crate::bash::registry::Argument],
) -> &'a [crate::bash::registry::Argument] {
    args
}

/// Check if args[0] matches a known subcommand.
fn is_known_subcommand(arg: &str, spec: Option<&crate::bash::registry::CommandSpec>) -> bool {
    match spec {
        Some(spec) if !spec.subcommands.is_empty() => {
            spec.subcommands.iter().any(|sub| sub.name == arg)
        }
        _ => false,
    }
}

/// Get the command prefix for a parsed command (static, non-LLM).
pub fn get_command_prefix_static(
    command: &str,
    recursion_depth: usize,
    wrapper_count: usize,
) -> Option<Option<String>> {
    if wrapper_count > 2 || recursion_depth > 10 {
        return None;
    }

    let parsed = parse_command(command)?;
    let command_node = match &parsed.command_node {
        Some(node) => node,
        None => return Some(None),
    };

    let cmd_args = extract_command_arguments(command_node);
    if cmd_args.is_empty() {
        return Some(None);
    }

    let cmd = &cmd_args[0];
    let args = &cmd_args[1..];

    let spec = get_command_spec(cmd);
    let mut is_wrapper = WRAPPER_COMMANDS.contains(cmd.as_str())
        || spec
            .as_ref()
            .map_or(false, |s| s.args.iter().any(|arg| arg.is_command));

    // Special case: if the command has subcommands and first arg matches
    if is_wrapper && !args.is_empty() && is_known_subcommand(&args[0], spec.as_ref()) {
        is_wrapper = false;
    }

    let prefix = if is_wrapper {
        handle_wrapper(cmd, args, recursion_depth, wrapper_count)
    } else {
        build_prefix(cmd, args, spec.as_ref())
    };

    if prefix.is_none() && recursion_depth == 0 && is_wrapper {
        return None;
    }

    let env_prefix = if !parsed.env_vars.is_empty() {
        format!("{} ", parsed.env_vars.join(" "))
    } else {
        String::new()
    };

    Some(prefix.map(|p| format!("{}{}", env_prefix, p)))
}

/// Build a prefix from command name, arguments, and optional spec.
fn build_prefix(
    cmd: &str,
    args: &[String],
    spec: Option<&crate::bash::registry::CommandSpec>,
) -> Option<String> {
    // If spec has args with isCommand, the command is a wrapper
    if let Some(spec) = spec {
        // Check if spec defines subcommands
        if !spec.subcommands.is_empty() && !args.is_empty() {
            // Find matching subcommand
            for sub in &spec.subcommands {
                if sub.name == args[0] {
                    return Some(format!("{} {}", cmd, args[0]));
                }
            }
        }
    }

    // For commands with no subcommands spec, the prefix is just the command name
    if args.is_empty() {
        return Some(cmd.to_string());
    }

    // Check if first arg looks like a subcommand (no dash prefix, not numeric, not env var)
    let first_arg = &args[0];
    if !first_arg.starts_with('-')
        && !NUMERIC_RE.is_match(first_arg)
        && !ENV_VAR_RE.is_match(first_arg)
    {
        return Some(format!("{} {}", cmd, first_arg));
    }

    Some(cmd.to_string())
}

fn handle_wrapper(
    command: &str,
    args: &[String],
    recursion_depth: usize,
    wrapper_count: usize,
) -> Option<String> {
    let spec = get_command_spec(command);

    if let Some(ref spec) = spec {
        let command_arg_index = spec.args.iter().position(|arg| arg.is_command);

        if let Some(cmd_idx) = command_arg_index {
            let mut parts = vec![command.to_string()];

            for i in 0..=std::cmp::min(args.len().saturating_sub(1), cmd_idx) {
                if i == cmd_idx {
                    let remaining = args[i..].join(" ");
                    let result = get_command_prefix_static(
                        &remaining,
                        recursion_depth + 1,
                        wrapper_count + 1,
                    );
                    if let Some(Some(prefix)) = result {
                        parts.extend(prefix.split(' ').map(|s| s.to_string()));
                        return Some(parts.join(" "));
                    }
                    break;
                } else if let Some(arg) = args.get(i) {
                    if !arg.starts_with('-') && !ENV_VAR_RE.is_match(arg) {
                        parts.push(arg.clone());
                    }
                }
            }
        }
    }

    // Fallback: find first non-flag, non-numeric, non-env-var argument
    let wrapped = args.iter().find(|arg| {
        !arg.starts_with('-') && !NUMERIC_RE.is_match(arg) && !ENV_VAR_RE.is_match(arg)
    });

    let wrapped = match wrapped {
        Some(w) => w,
        None => return Some(command.to_string()),
    };

    let idx = args.iter().position(|a| a == wrapped).unwrap();
    let remaining = args[idx..].join(" ");
    let result = get_command_prefix_static(&remaining, recursion_depth + 1, wrapper_count + 1);

    match result {
        Some(Some(prefix)) => Some(format!("{} {}", command, prefix)),
        _ => None,
    }
}

/// Computes prefixes for a compound command.
pub fn get_compound_command_prefixes_static(
    command: &str,
    exclude_subcommand: Option<&dyn Fn(&str) -> bool>,
) -> Vec<String> {
    let subcommands = split_command_deprecated(command);
    if subcommands.len() <= 1 {
        let result = get_command_prefix_static(command, 0, 0);
        return match result {
            Some(Some(prefix)) => vec![prefix],
            _ => Vec::new(),
        };
    }

    let mut prefixes: Vec<String> = Vec::new();
    for subcmd in &subcommands {
        let trimmed = subcmd.trim();
        if let Some(exclude_fn) = exclude_subcommand {
            if exclude_fn(trimmed) {
                continue;
            }
        }
        let result = get_command_prefix_static(trimmed, 0, 0);
        if let Some(Some(prefix)) = result {
            prefixes.push(prefix);
        }
    }

    if prefixes.is_empty() {
        return Vec::new();
    }

    // Group prefixes by their first word (root command)
    let mut groups: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for prefix in &prefixes {
        let root = prefix.split(' ').next().unwrap_or("").to_string();
        groups.entry(root).or_default().push(prefix.clone());
    }

    // Collapse each group via word-aligned LCP
    let mut collapsed: Vec<String> = Vec::new();
    for (_, group) in &groups {
        collapsed.push(longest_common_prefix(group));
    }
    collapsed
}

/// Compute the longest common prefix of strings, aligned to word boundaries.
fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    if strings.len() == 1 {
        return strings[0].clone();
    }

    let first = &strings[0];
    let words: Vec<&str> = first.split(' ').collect();
    let mut common_words = words.len();

    for s in &strings[1..] {
        let other_words: Vec<&str> = s.split(' ').collect();
        let mut shared = 0;
        while shared < common_words
            && shared < other_words.len()
            && words[shared] == other_words[shared]
        {
            shared += 1;
        }
        common_words = shared;
    }

    words[..std::cmp::max(1, common_words)].join(" ")
}

// ─── Prefix Extractor Factory ────────────────────────────────────────────────

use std::sync::Arc;
use std::sync::Mutex;

/// 对应 TS `PrefixExtractorConfig`：构造前缀提取器时使用的可选配置。
#[derive(Clone, Default)]
pub struct PrefixExtractorConfig {
    /// 可选的命令归一化函数（用于 alias 展开等）。
    pub normalize_command: Option<Arc<dyn Fn(&str) -> String + Send + Sync>>,
    /// 是否启用结果缓存（TS 默认为 true）。
    pub enable_cache: bool,
}

/// 对应 TS `createCommandPrefixExtractor`：创建带两层缓存的命令前缀提取器。
///
/// 返回的闭包接受单条命令，返回该命令的最短安全前缀（与
/// [`get_command_prefix_static`] 等价）。
pub fn create_command_prefix_extractor(
    config: PrefixExtractorConfig,
) -> Arc<dyn Fn(&str) -> Option<String> + Send + Sync> {
    let cache: Arc<Mutex<std::collections::HashMap<String, Option<String>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
    let enable_cache = config.enable_cache;
    let normalize = config.normalize_command;

    Arc::new(move |command: &str| -> Option<String> {
        let normalized = match &normalize {
            Some(f) => f(command),
            None => command.to_string(),
        };
        if enable_cache {
            if let Some(hit) = cache.lock().unwrap().get(&normalized).cloned() {
                return hit;
            }
        }
        let computed = match get_command_prefix_static(&normalized, 0, 0) {
            Some(Some(p)) => Some(p),
            _ => None,
        };
        if enable_cache {
            cache.lock().unwrap().insert(normalized, computed.clone());
        }
        computed
    })
}

/// 对应 TS `createSubcommandPrefixExtractor`：基于单命令前缀提取器构造一个
/// 处理 compound 命令（含 `&&`/`||`/`;`/`|`）的提取器。
pub fn create_subcommand_prefix_extractor(
    get_prefix: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
) -> Arc<dyn Fn(&str) -> Vec<String> + Send + Sync> {
    let cache: Arc<Mutex<std::collections::HashMap<String, Vec<String>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    Arc::new(move |command: &str| -> Vec<String> {
        if let Some(hit) = cache.lock().unwrap().get(command).cloned() {
            return hit;
        }
        let subcommands = split_command_deprecated(command);
        let prefixes: Vec<String> = if subcommands.len() <= 1 {
            match get_prefix(command) {
                Some(p) => vec![p],
                None => Vec::new(),
            }
        } else {
            subcommands
                .iter()
                .filter_map(|sc| get_prefix(sc.trim()))
                .collect()
        };
        cache
            .lock()
            .unwrap()
            .insert(command.to_string(), prefixes.clone());
        prefixes
    })
}
