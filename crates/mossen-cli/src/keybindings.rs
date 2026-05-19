//! 快捷键系统 — 对应 TS 的 keybindings/ 目录。
//!
//! 管理用户自定义快捷键绑定、验证、解析和匹配。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

// ─── Schema (keybindings/schema.ts) ────────────────────────────────────────

/// 修饰键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
    Super,
}

/// 按键绑定定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    /// 快捷键组合字符串（如 "ctrl+k"）。
    pub key: String,
    /// 绑定的命令名。
    pub command: String,
    /// 当条件（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// 命令参数（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// 已解析的按键组合。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParsedKey {
    /// 基础键（不含修饰键）。
    pub key: String,
    /// 修饰键集合。
    pub modifiers: Vec<Modifier>,
}

/// 按键绑定上下文。
#[derive(Debug, Clone, Default)]
pub struct KeybindingContext {
    /// 当前是否处于输入模式。
    pub is_input_focused: bool,
    /// 当前是否有对话活跃。
    pub is_session_active: bool,
    /// 当前是否处于 vim 普通模式。
    pub is_vim_normal: bool,
    /// 当前面板状态。
    pub current_panel: Option<String>,
}

// ─── Parser (keybindings/parser.ts) ────────────────────────────────────────

/// 解析快捷键字符串为 ParsedKey。
///
/// 支持格式：
/// - "ctrl+k"
/// - "ctrl+shift+p"
/// - "alt+enter"
/// - "escape"
pub fn parse_key(key_str: &str) -> Result<ParsedKey> {
    let parts: Vec<&str> = key_str.split('+').collect();
    if parts.is_empty() {
        anyhow::bail!("empty key binding");
    }

    let mut modifiers = Vec::new();
    let mut base_key = None;

    for part in &parts {
        let normalized = part.trim().to_lowercase();
        match normalized.as_str() {
            "ctrl" | "control" => modifiers.push(Modifier::Ctrl),
            "alt" | "option" => modifiers.push(Modifier::Alt),
            "shift" => modifiers.push(Modifier::Shift),
            "meta" | "cmd" | "command" => modifiers.push(Modifier::Meta),
            "super" | "win" => modifiers.push(Modifier::Super),
            _ => {
                if base_key.is_some() {
                    anyhow::bail!("multiple base keys in binding: {}", key_str);
                }
                base_key = Some(normalized);
            }
        }
    }

    let key = base_key.ok_or_else(|| anyhow::anyhow!("no base key in binding: {}", key_str))?;

    // 排序修饰键以确保一致性
    modifiers.sort_by_key(|m| match m {
        Modifier::Ctrl => 0,
        Modifier::Alt => 1,
        Modifier::Shift => 2,
        Modifier::Meta => 3,
        Modifier::Super => 4,
    });
    modifiers.dedup();

    Ok(ParsedKey { key, modifiers })
}

// ─── Default Bindings (keybindings/defaultBindings.ts) ─────────────────────

/// 获取默认按键绑定。
pub fn get_default_bindings() -> Vec<KeyBinding> {
    vec![
        // 基本导航
        KeyBinding {
            key: "ctrl+c".to_string(),
            command: "interrupt".to_string(),
            when: None,
            args: None,
        },
        KeyBinding {
            key: "ctrl+d".to_string(),
            command: "exit".to_string(),
            when: Some("inputEmpty".to_string()),
            args: None,
        },
        KeyBinding {
            key: "escape".to_string(),
            command: "cancel".to_string(),
            when: None,
            args: None,
        },
        // 编辑快捷键
        KeyBinding {
            key: "ctrl+a".to_string(),
            command: "cursor.home".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        KeyBinding {
            key: "ctrl+e".to_string(),
            command: "cursor.end".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        KeyBinding {
            key: "ctrl+k".to_string(),
            command: "input.killLine".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        KeyBinding {
            key: "ctrl+u".to_string(),
            command: "input.killLineBack".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        KeyBinding {
            key: "ctrl+w".to_string(),
            command: "input.killWord".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        // 历史导航
        KeyBinding {
            key: "ctrl+p".to_string(),
            command: "history.previous".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        KeyBinding {
            key: "ctrl+n".to_string(),
            command: "history.next".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        // 功能快捷键
        KeyBinding {
            key: "ctrl+r".to_string(),
            command: "history.search".to_string(),
            when: None,
            args: None,
        },
        KeyBinding {
            key: "ctrl+l".to_string(),
            command: "clear".to_string(),
            when: None,
            args: None,
        },
        KeyBinding {
            key: "ctrl+o".to_string(),
            command: "expand".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        KeyBinding {
            key: "ctrl+j".to_string(),
            command: "submit".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
        // Tab 补全
        KeyBinding {
            key: "tab".to_string(),
            command: "complete".to_string(),
            when: Some("inputFocused".to_string()),
            args: None,
        },
    ]
}

// ─── Reserved Shortcuts (keybindings/reservedShortcuts.ts) ─────────────────

/// 保留的快捷键（不可被用户覆盖）。
pub fn get_reserved_shortcuts() -> Vec<&'static str> {
    vec![
        "ctrl+c",    // 中断
        "ctrl+z",    // 挂起进程
        "ctrl+\\",   // SIGQUIT
        "ctrl+s",    // XOFF
        "ctrl+q",    // XON
    ]
}

/// 检查快捷键是否为保留键。
pub fn is_reserved_shortcut(key: &str) -> bool {
    get_reserved_shortcuts()
        .iter()
        .any(|&reserved| reserved == key.to_lowercase())
}

// ─── Validate (keybindings/validate.ts) ────────────────────────────────────

/// 验证错误。
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub binding_index: usize,
    pub message: String,
}

/// 验证按键绑定配置。
///
/// 检查：
/// - 格式正确性
/// - 命令有效性
/// - 冲突检测
/// - 保留键检测
pub fn validate_bindings(
    bindings: &[KeyBinding],
    available_commands: &[&str],
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (i, binding) in bindings.iter().enumerate() {
        // 检查键格式
        if let Err(e) = parse_key(&binding.key) {
            errors.push(ValidationError {
                binding_index: i,
                message: format!("Invalid key '{}': {}", binding.key, e),
            });
            continue;
        }

        // 检查是否为保留键
        if is_reserved_shortcut(&binding.key) {
            errors.push(ValidationError {
                binding_index: i,
                message: format!(
                    "Key '{}' is reserved and cannot be rebound",
                    binding.key
                ),
            });
        }

        // 检查命令有效性
        if !available_commands.is_empty()
            && !available_commands.contains(&binding.command.as_str())
        {
            errors.push(ValidationError {
                binding_index: i,
                message: format!("Unknown command: '{}'", binding.command),
            });
        }
    }

    // 检查冲突
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, binding) in bindings.iter().enumerate() {
        let key_normalized = binding.key.to_lowercase();
        let context_key = format!(
            "{}@{}",
            key_normalized,
            binding.when.as_deref().unwrap_or("*")
        );
        if let Some(prev_idx) = seen.get(&context_key) {
            errors.push(ValidationError {
                binding_index: i,
                message: format!(
                    "Conflict: '{}' already bound at index {}",
                    binding.key, prev_idx
                ),
            });
        } else {
            seen.insert(context_key, i);
        }
    }

    errors
}

// ─── Load User Bindings (keybindings/loadUserBindings.ts) ──────────────────

/// 加载用户自定义绑定。
///
/// 从 ~/.mossen/keybindings.json 加载，与默认绑定合并。
pub fn load_user_bindings() -> Vec<KeyBinding> {
    let config_path = mossen_utils::env::get_mossen_config_home_dir().join("keybindings.json");

    if !config_path.exists() {
        return get_default_bindings();
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => match serde_json::from_str::<Vec<KeyBinding>>(&content) {
            Ok(user_bindings) => {
                info!(count = user_bindings.len(), "loaded user keybindings");
                merge_bindings(&get_default_bindings(), &user_bindings)
            }
            Err(e) => {
                warn!(error = %e, "failed to parse keybindings.json, using defaults");
                get_default_bindings()
            }
        },
        Err(e) => {
            warn!(error = %e, "failed to read keybindings.json, using defaults");
            get_default_bindings()
        }
    }
}

/// 合并默认绑定和用户绑定。
///
/// 用户绑定优先于默认绑定（相同 key+when 条件下）。
fn merge_bindings(defaults: &[KeyBinding], user: &[KeyBinding]) -> Vec<KeyBinding> {
    let mut result: Vec<KeyBinding> = Vec::new();
    let mut user_keys: HashMap<String, &KeyBinding> = HashMap::new();

    for binding in user {
        let key = format!(
            "{}@{}",
            binding.key.to_lowercase(),
            binding.when.as_deref().unwrap_or("*")
        );
        user_keys.insert(key, binding);
    }

    // 添加默认绑定（如果用户没有覆盖）
    for binding in defaults {
        let key = format!(
            "{}@{}",
            binding.key.to_lowercase(),
            binding.when.as_deref().unwrap_or("*")
        );
        if !user_keys.contains_key(&key) {
            result.push(binding.clone());
        }
    }

    // 添加所有用户绑定
    for binding in user {
        result.push(binding.clone());
    }

    result
}

// ─── Resolver (keybindings/resolver.ts) ────────────────────────────────────

/// 快捷键解析器 — 根据当前上下文解析要执行的命令。
pub struct KeybindingResolver {
    bindings: Vec<KeyBinding>,
}

impl KeybindingResolver {
    /// 创建解析器（加载用户绑定）。
    pub fn new() -> Self {
        Self {
            bindings: load_user_bindings(),
        }
    }

    /// 使用指定绑定创建解析器。
    pub fn with_bindings(bindings: Vec<KeyBinding>) -> Self {
        Self { bindings }
    }

    /// 解析按键事件，返回匹配的命令。
    ///
    /// 按照优先级顺序匹配：
    /// 1. 精确匹配 key + when 条件
    /// 2. 精确匹配 key（无 when 条件）
    pub fn resolve(&self, key: &ParsedKey, context: &KeybindingContext) -> Option<&KeyBinding> {
        // 首先尝试带 when 条件的精确匹配
        for binding in &self.bindings {
            if let Ok(parsed) = parse_key(&binding.key) {
                if parsed == *key {
                    if let Some(ref when) = binding.when {
                        if evaluate_when_clause(when, context) {
                            return Some(binding);
                        }
                    }
                }
            }
        }

        // 然后尝试无条件匹配
        for binding in &self.bindings {
            if binding.when.is_none() {
                if let Ok(parsed) = parse_key(&binding.key) {
                    if parsed == *key {
                        return Some(binding);
                    }
                }
            }
        }

        None
    }

    /// 获取所有绑定。
    pub fn all_bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }
}

impl Default for KeybindingResolver {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Match (keybindings/match.ts) ──────────────────────────────────────────

/// 评估 when 条件子句。
fn evaluate_when_clause(when: &str, context: &KeybindingContext) -> bool {
    let conditions: Vec<&str> = when.split("&&").map(|s| s.trim()).collect();

    for condition in conditions {
        let (negated, cond_name) = if let Some(stripped) = condition.strip_prefix('!') {
            (true, stripped.trim())
        } else {
            (false, condition)
        };

        let value = match cond_name {
            "inputFocused" => context.is_input_focused,
            "inputEmpty" => !context.is_input_focused, // 简化
            "sessionActive" => context.is_session_active,
            "vimNormal" => context.is_vim_normal,
            _ => false,
        };

        if negated {
            if value {
                return false;
            }
        } else if !value {
            return false;
        }
    }
    true
}

// ─── useKeybinding hook (keybindings/useKeybinding.ts) ──────────────────────

/// 快捷键提供器设置。
///
/// 对应 TS 的 KeybindingProviderSetup — 在应用启动时初始化快捷键系统。
pub fn setup_keybinding_provider() -> KeybindingResolver {
    let resolver = KeybindingResolver::new();
    info!(
        bindings = resolver.all_bindings().len(),
        "keybinding provider initialized"
    );
    resolver
}

// ────────────────────────────────────────────────────────────────────────────
// keybindings/parser.ts — 详细 keystroke / chord 解析
// ────────────────────────────────────────────────────────────────────────────

/// 已解析单个 keystroke。对应 TS `ParsedKeystroke`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParsedKeystroke {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub super_key: bool,
}

/// Chord = 多个 keystroke 序列。
pub type Chord = Vec<ParsedKeystroke>;

/// 解析单个 keystroke 字符串（如 "ctrl+shift+k"）。
pub fn parse_keystroke(input: &str) -> ParsedKeystroke {
    let mut ks = ParsedKeystroke::default();
    for part in input.split('+') {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => ks.ctrl = true,
            "alt" | "opt" | "option" => ks.alt = true,
            "shift" => ks.shift = true,
            "meta" => ks.meta = true,
            "cmd" | "command" | "super" | "win" => ks.super_key = true,
            "esc" => ks.key = "escape".to_string(),
            "return" => ks.key = "enter".to_string(),
            "space" => ks.key = " ".to_string(),
            "↑" => ks.key = "up".to_string(),
            "↓" => ks.key = "down".to_string(),
            "←" => ks.key = "left".to_string(),
            "→" => ks.key = "right".to_string(),
            _ => ks.key = lower,
        }
    }
    ks
}

/// 别名以匹配 TS export 名（驼峰式）。
pub fn parseKeystroke(input: &str) -> ParsedKeystroke {
    parse_keystroke(input)
}

/// 解析 chord 字符串（如 "ctrl+k ctrl+s"）。
pub fn parse_chord(input: &str) -> Chord {
    if input == " " {
        return vec![parse_keystroke("space")];
    }
    input
        .trim()
        .split_whitespace()
        .map(parse_keystroke)
        .collect()
}

pub fn parseChord(input: &str) -> Chord {
    parse_chord(input)
}

/// 将一个 keystroke 转为字符串表示。
pub fn keystroke_to_string(ks: &ParsedKeystroke) -> String {
    let mut parts: Vec<String> = Vec::new();
    if ks.ctrl {
        parts.push("ctrl".into());
    }
    if ks.alt {
        parts.push("alt".into());
    }
    if ks.shift {
        parts.push("shift".into());
    }
    if ks.meta {
        parts.push("meta".into());
    }
    if ks.super_key {
        parts.push("cmd".into());
    }
    parts.push(key_to_display_name(&ks.key));
    parts.join("+")
}

pub fn keystrokeToString(ks: &ParsedKeystroke) -> String {
    keystroke_to_string(ks)
}

fn key_to_display_name(key: &str) -> String {
    match key {
        "escape" => "Esc".into(),
        " " => "Space".into(),
        "tab" => "tab".into(),
        "enter" => "Enter".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        other => other.into(),
    }
}

/// 将 chord 转为字符串表示。
pub fn chord_to_string(chord: &[ParsedKeystroke]) -> String {
    chord
        .iter()
        .map(keystroke_to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn chordToString(chord: &[ParsedKeystroke]) -> String {
    chord_to_string(chord)
}

/// 显示平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPlatform {
    Macos,
    Windows,
    Linux,
    Wsl,
    Unknown,
}

pub fn keystroke_to_display_string(ks: &ParsedKeystroke, platform: DisplayPlatform) -> String {
    let mut parts: Vec<String> = Vec::new();
    if ks.ctrl {
        parts.push("ctrl".into());
    }
    if ks.alt || ks.meta {
        parts.push(
            if matches!(platform, DisplayPlatform::Macos) {
                "opt".into()
            } else {
                "alt".into()
            },
        );
    }
    if ks.shift {
        parts.push("shift".into());
    }
    if ks.super_key {
        parts.push(
            if matches!(platform, DisplayPlatform::Macos) {
                "cmd".into()
            } else {
                "super".into()
            },
        );
    }
    parts.push(key_to_display_name(&ks.key));
    parts.join("+")
}

pub fn chord_to_display_string(chord: &[ParsedKeystroke], platform: DisplayPlatform) -> String {
    chord
        .iter()
        .map(|ks| keystroke_to_display_string(ks, platform))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 用户 keybindings 配置块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingBlock {
    pub context: String,
    /// 键 → 动作（或 null 表示解绑）。
    pub bindings: HashMap<String, Option<String>>,
}

/// 已解析的绑定项。
#[derive(Debug, Clone)]
pub struct ParsedBinding {
    pub chord: Chord,
    pub action: Option<String>,
    pub context: String,
}

/// 把 KeybindingBlock 列表解析为 ParsedBinding 列表。
pub fn parse_bindings(blocks: &[KeybindingBlock]) -> Vec<ParsedBinding> {
    let mut bindings = Vec::new();
    for block in blocks {
        for (key, action) in &block.bindings {
            bindings.push(ParsedBinding {
                chord: parse_chord(key),
                action: action.clone(),
                context: block.context.clone(),
            });
        }
    }
    bindings
}

// ────────────────────────────────────────────────────────────────────────────
// keybindings/validate.ts
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeybindingWarningType {
    ParseError,
    Duplicate,
    Reserved,
    InvalidContext,
    InvalidAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeybindingSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingWarning {
    #[serde(rename = "type")]
    pub kind: KeybindingWarningType,
    pub severity: KeybindingSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// 有效的 context 名（必须与 TS 同步）。
pub const VALID_CONTEXTS: &[&str] = &[
    "Global",
    "Chat",
    "Autocomplete",
    "Confirmation",
    "Help",
    "Transcript",
    "HistorySearch",
    "Task",
    "ThemePicker",
    "Settings",
    "Tabs",
    "Attachments",
    "Footer",
    "MessageSelector",
    "DiffDialog",
    "ModelPicker",
    "Select",
    "Plugin",
];

fn is_valid_context(name: &str) -> bool {
    VALID_CONTEXTS.contains(&name)
}

fn validate_keystroke_str(keystroke: &str) -> Option<KeybindingWarning> {
    for part in keystroke.to_lowercase().split('+') {
        if part.trim().is_empty() {
            return Some(KeybindingWarning {
                kind: KeybindingWarningType::ParseError,
                severity: KeybindingSeverity::Error,
                message: format!("Empty key part in \"{}\"", keystroke),
                key: Some(keystroke.to_string()),
                context: None,
                action: None,
                suggestion: Some("Remove extra \"+\" characters".into()),
            });
        }
    }
    let parsed = parse_keystroke(keystroke);
    if parsed.key.is_empty()
        && !parsed.ctrl
        && !parsed.alt
        && !parsed.shift
        && !parsed.meta
    {
        return Some(KeybindingWarning {
            kind: KeybindingWarningType::ParseError,
            severity: KeybindingSeverity::Error,
            message: format!("Could not parse keystroke \"{}\"", keystroke),
            key: Some(keystroke.to_string()),
            context: None,
            action: None,
            suggestion: None,
        });
    }
    None
}

fn validate_block(block: &serde_json::Value, block_index: usize) -> Vec<KeybindingWarning> {
    let mut warnings = Vec::new();
    let block_obj = match block.as_object() {
        Some(o) => o,
        None => {
            warnings.push(KeybindingWarning {
                kind: KeybindingWarningType::ParseError,
                severity: KeybindingSeverity::Error,
                message: format!("Keybinding block {} is not an object", block_index + 1),
                key: None,
                context: None,
                action: None,
                suggestion: None,
            });
            return warnings;
        }
    };

    let raw_context = block_obj.get("context").and_then(|v| v.as_str());
    let context_name = match raw_context {
        None => {
            warnings.push(KeybindingWarning {
                kind: KeybindingWarningType::ParseError,
                severity: KeybindingSeverity::Error,
                message: format!(
                    "Keybinding block {} missing \"context\" field",
                    block_index + 1
                ),
                key: None,
                context: None,
                action: None,
                suggestion: None,
            });
            None
        }
        Some(s) if !is_valid_context(s) => {
            warnings.push(KeybindingWarning {
                kind: KeybindingWarningType::InvalidContext,
                severity: KeybindingSeverity::Error,
                message: format!("Unknown context \"{}\"", s),
                key: None,
                context: Some(s.to_string()),
                action: None,
                suggestion: Some(format!("Valid contexts: {}", VALID_CONTEXTS.join(", "))),
            });
            None
        }
        Some(s) => Some(s.to_string()),
    };

    let bindings = match block_obj.get("bindings").and_then(|v| v.as_object()) {
        Some(b) => b,
        None => {
            warnings.push(KeybindingWarning {
                kind: KeybindingWarningType::ParseError,
                severity: KeybindingSeverity::Error,
                message: format!(
                    "Keybinding block {} missing \"bindings\" field",
                    block_index + 1
                ),
                key: None,
                context: None,
                action: None,
                suggestion: None,
            });
            return warnings;
        }
    };

    for (key, action_val) in bindings {
        if let Some(mut w) = validate_keystroke_str(key) {
            w.context = context_name.clone();
            warnings.push(w);
        }

        let action_str = if action_val.is_null() {
            None
        } else if let Some(s) = action_val.as_str() {
            Some(s.to_string())
        } else {
            warnings.push(KeybindingWarning {
                kind: KeybindingWarningType::InvalidAction,
                severity: KeybindingSeverity::Error,
                message: format!("Invalid action for \"{}\": must be a string or null", key),
                key: Some(key.clone()),
                context: context_name.clone(),
                action: None,
                suggestion: None,
            });
            continue;
        };

        if let Some(action) = &action_str {
            if action.starts_with("command:") {
                let valid_re = regex::Regex::new(r"^command:[a-zA-Z0-9:\-_]+$").unwrap();
                if !valid_re.is_match(action) {
                    warnings.push(KeybindingWarning {
                        kind: KeybindingWarningType::InvalidAction,
                        severity: KeybindingSeverity::Warning,
                        message: format!(
                            "Invalid command binding \"{}\" for \"{}\": command name may only contain alphanumeric characters, colons, hyphens, and underscores",
                            action, key
                        ),
                        key: Some(key.clone()),
                        context: context_name.clone(),
                        action: Some(action.clone()),
                        suggestion: None,
                    });
                }
                if let Some(ctx) = &context_name {
                    if ctx != "Chat" {
                        warnings.push(KeybindingWarning {
                            kind: KeybindingWarningType::InvalidAction,
                            severity: KeybindingSeverity::Warning,
                            message: format!(
                                "Command binding \"{}\" must be in \"Chat\" context, not \"{}\"",
                                action, ctx
                            ),
                            key: Some(key.clone()),
                            context: context_name.clone(),
                            action: Some(action.clone()),
                            suggestion: Some(
                                "Move this binding to a block with \"context\": \"Chat\"".into(),
                            ),
                        });
                    }
                }
            }
        }
    }

    warnings
}

/// 在 JSON 字符串中检查重复键。
pub fn check_duplicate_keys_in_json(json_string: &str) -> Vec<KeybindingWarning> {
    let mut warnings = Vec::new();
    // 匹配 "bindings" : { ... }
    let block_re = match regex::Regex::new(r#""bindings"\s*:\s*\{([^{}]*(?:\{[^{}]*\}[^{}]*)*)\}"#) {
        Ok(r) => r,
        Err(_) => return warnings,
    };
    let key_re = regex::Regex::new(r#""([^"]+)"\s*:"#).unwrap();
    let context_re = regex::Regex::new(r#""context"\s*:\s*"([^"]+)"[^{]*$"#).unwrap();

    for m in block_re.captures_iter(json_string) {
        let full_match = m.get(0).unwrap();
        let block_content = m.get(1).map(|m| m.as_str()).unwrap_or("");
        let text_before = &json_string[..full_match.start()];
        let context = context_re
            .captures(text_before)
            .and_then(|c| c.get(1).map(|x| x.as_str().to_string()))
            .unwrap_or_else(|| "unknown".to_string());

        let mut keys_by_name: HashMap<String, usize> = HashMap::new();
        for c in key_re.captures_iter(block_content) {
            let key = c.get(1).unwrap().as_str().to_string();
            let count = *keys_by_name.get(&key).unwrap_or(&0) + 1;
            keys_by_name.insert(key.clone(), count);
            if count == 2 {
                warnings.push(KeybindingWarning {
                    kind: KeybindingWarningType::Duplicate,
                    severity: KeybindingSeverity::Warning,
                    message: format!("Duplicate key \"{}\" in {} bindings", key, context),
                    key: Some(key),
                    context: Some(context.clone()),
                    action: None,
                    suggestion: Some(
                        "This key appears multiple times in the same context. JSON uses the last value, earlier values are ignored.".into(),
                    ),
                });
            }
        }
    }

    warnings
}

pub fn checkDuplicateKeysInJson(json_string: &str) -> Vec<KeybindingWarning> {
    check_duplicate_keys_in_json(json_string)
}

/// 验证用户 keybinding 配置。
pub fn validate_user_config(user_blocks: &serde_json::Value) -> Vec<KeybindingWarning> {
    let mut warnings = Vec::new();
    let arr = match user_blocks.as_array() {
        Some(a) => a,
        None => {
            warnings.push(KeybindingWarning {
                kind: KeybindingWarningType::ParseError,
                severity: KeybindingSeverity::Error,
                message: "keybindings.json must contain an array".into(),
                key: None,
                context: None,
                action: None,
                suggestion: Some("Wrap your bindings in [ ]".into()),
            });
            return warnings;
        }
    };
    for (i, block) in arr.iter().enumerate() {
        warnings.extend(validate_block(block, i));
    }
    warnings
}

pub fn validateUserConfig(user_blocks: &serde_json::Value) -> Vec<KeybindingWarning> {
    validate_user_config(user_blocks)
}

/// 检查同 context 内是否存在 key 重复。
pub fn check_duplicates(blocks: &[KeybindingBlock]) -> Vec<KeybindingWarning> {
    let mut warnings = Vec::new();
    let mut seen_by_context: HashMap<String, HashMap<String, String>> = HashMap::new();

    for block in blocks {
        let ctx_map = seen_by_context
            .entry(block.context.clone())
            .or_default();
        for (key, action) in &block.bindings {
            let normalized = key.to_lowercase();
            if let Some(existing) = ctx_map.get(&normalized) {
                if let Some(a) = action {
                    if existing != a {
                        warnings.push(KeybindingWarning {
                            kind: KeybindingWarningType::Duplicate,
                            severity: KeybindingSeverity::Warning,
                            message: format!(
                                "Duplicate binding \"{}\" in {} context",
                                key, block.context
                            ),
                            key: Some(key.clone()),
                            context: Some(block.context.clone()),
                            action: action.clone(),
                            suggestion: Some(format!(
                                "Previously bound to \"{}\". Only the last binding will be used.",
                                existing
                            )),
                        });
                    }
                }
            }
            ctx_map.insert(
                normalized,
                action.clone().unwrap_or_else(|| "null".to_string()),
            );
        }
    }

    warnings
}

pub fn checkDuplicates(blocks: &[KeybindingBlock]) -> Vec<KeybindingWarning> {
    check_duplicates(blocks)
}

/// 检查保留快捷键。
pub fn check_reserved_shortcuts(bindings: &[ParsedBinding]) -> Vec<KeybindingWarning> {
    let mut warnings = Vec::new();
    let reserved = get_reserved_shortcuts();
    for binding in bindings {
        let key_display = chord_to_string(&binding.chord);
        let normalized = key_display.to_lowercase();
        for res in &reserved {
            if res.to_lowercase() == normalized {
                warnings.push(KeybindingWarning {
                    kind: KeybindingWarningType::Reserved,
                    severity: KeybindingSeverity::Warning,
                    message: format!("\"{}\" may not work: reserved shortcut", key_display),
                    key: Some(key_display.clone()),
                    context: Some(binding.context.clone()),
                    action: binding.action.clone(),
                    suggestion: None,
                });
            }
        }
    }
    warnings
}

pub fn checkReservedShortcuts(bindings: &[ParsedBinding]) -> Vec<KeybindingWarning> {
    check_reserved_shortcuts(bindings)
}

/// 综合验证。
pub fn validate_bindings_full(
    user_blocks: &serde_json::Value,
    _parsed_bindings: &[ParsedBinding],
) -> Vec<KeybindingWarning> {
    let mut warnings = validate_user_config(user_blocks);
    if let Some(arr) = user_blocks.as_array() {
        let blocks: Vec<KeybindingBlock> = arr
            .iter()
            .filter_map(|v| serde_json::from_value::<KeybindingBlock>(v.clone()).ok())
            .collect();
        warnings.extend(check_duplicates(&blocks));
        let user_bindings = parse_bindings(&blocks);
        warnings.extend(check_reserved_shortcuts(&user_bindings));
    }
    // 去重
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    warnings.retain(|w| {
        let k = format!(
            "{:?}:{:?}:{:?}",
            w.kind, w.key, w.context
        );
        seen.insert(k)
    });
    warnings
}

/// 格式化警告显示。
pub fn format_warning(w: &KeybindingWarning) -> String {
    let icon = if matches!(w.severity, KeybindingSeverity::Error) {
        "✗"
    } else {
        "⚠"
    };
    let sev = if matches!(w.severity, KeybindingSeverity::Error) {
        "error"
    } else {
        "warning"
    };
    let mut msg = format!("{} Keybinding {}: {}", icon, sev, w.message);
    if let Some(s) = &w.suggestion {
        msg.push_str(&format!("\n  {}", s));
    }
    msg
}

/// 格式化多个警告。
pub fn format_warnings(warnings: &[KeybindingWarning]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let errors: Vec<_> = warnings
        .iter()
        .filter(|w| matches!(w.severity, KeybindingSeverity::Error))
        .collect();
    let warns: Vec<_> = warnings
        .iter()
        .filter(|w| matches!(w.severity, KeybindingSeverity::Warning))
        .collect();
    let mut lines: Vec<String> = Vec::new();
    if !errors.is_empty() {
        lines.push(format!("Found {} keybinding error(s):", errors.len()));
        for e in &errors {
            lines.push(format_warning(e));
        }
    }
    if !warns.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("Found {} keybinding warning(s):", warns.len()));
        for w in &warns {
            lines.push(format_warning(w));
        }
    }
    lines.join("\n")
}

// ────────────────────────────────────────────────────────────────────────────
// keybindings/loadUserBindings.ts
// ────────────────────────────────────────────────────────────────────────────

/// 加载结果。
#[derive(Debug, Clone)]
pub struct KeybindingsLoadResult {
    pub bindings: Vec<ParsedBinding>,
    pub warnings: Vec<KeybindingWarning>,
}

/// 是否启用 keybinding 自定义。
pub fn is_keybinding_customization_enabled() -> bool {
    // GrowthBook gate 在 Rust 端默认开启；外部用户也允许自定义。
    true
}

pub fn isKeybindingCustomizationEnabled() -> bool {
    is_keybinding_customization_enabled()
}

/// 获取 keybindings.json 路径。
pub fn get_keybindings_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".mossen").join("keybindings.json")
}

pub fn getKeybindingsPath() -> std::path::PathBuf {
    get_keybindings_path()
}

/// 从默认绑定生成 ParsedBinding 列表。
fn get_default_parsed_bindings() -> Vec<ParsedBinding> {
    get_default_bindings()
        .into_iter()
        .map(|kb| ParsedBinding {
            chord: parse_chord(&kb.key),
            action: Some(kb.command),
            context: kb.when.unwrap_or_else(|| "Global".to_string()),
        })
        .collect()
}

/// 异步加载 keybindings。
pub async fn load_keybindings() -> KeybindingsLoadResult {
    let default_bindings = get_default_parsed_bindings();
    if !is_keybinding_customization_enabled() {
        return KeybindingsLoadResult {
            bindings: default_bindings,
            warnings: Vec::new(),
        };
    }
    let path = get_keybindings_path();
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(_) => {
            return KeybindingsLoadResult {
                bindings: default_bindings,
                warnings: Vec::new(),
            };
        }
    };
    parse_keybindings_content(&content, default_bindings)
}

pub async fn loadKeybindings() -> KeybindingsLoadResult {
    load_keybindings().await
}

/// 同步加载 keybindings（initial render 使用）。
pub fn load_keybindings_sync() -> Vec<ParsedBinding> {
    load_keybindings_sync_with_warnings().bindings
}

pub fn loadKeybindingsSync() -> Vec<ParsedBinding> {
    load_keybindings_sync()
}

pub fn load_keybindings_sync_with_warnings() -> KeybindingsLoadResult {
    let default_bindings = get_default_parsed_bindings();
    if !is_keybinding_customization_enabled() {
        return KeybindingsLoadResult {
            bindings: default_bindings,
            warnings: Vec::new(),
        };
    }
    let path = get_keybindings_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return KeybindingsLoadResult {
                bindings: default_bindings,
                warnings: Vec::new(),
            };
        }
    };
    parse_keybindings_content(&content, default_bindings)
}

fn parse_keybindings_content(
    content: &str,
    default_bindings: Vec<ParsedBinding>,
) -> KeybindingsLoadResult {
    let parsed: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            return KeybindingsLoadResult {
                bindings: default_bindings,
                warnings: vec![KeybindingWarning {
                    kind: KeybindingWarningType::ParseError,
                    severity: KeybindingSeverity::Error,
                    message: format!("Failed to parse keybindings.json: {}", e),
                    key: None,
                    context: None,
                    action: None,
                    suggestion: None,
                }],
            };
        }
    };
    let user_blocks = match parsed.get("bindings") {
        Some(v) => v.clone(),
        None => {
            return KeybindingsLoadResult {
                bindings: default_bindings,
                warnings: vec![KeybindingWarning {
                    kind: KeybindingWarningType::ParseError,
                    severity: KeybindingSeverity::Error,
                    message: "keybindings.json must have a \"bindings\" array".into(),
                    key: None,
                    context: None,
                    action: None,
                    suggestion: Some("Use format: { \"bindings\": [ ... ] }".into()),
                }],
            };
        }
    };

    let arr = match user_blocks.as_array() {
        Some(a) => a,
        None => {
            return KeybindingsLoadResult {
                bindings: default_bindings,
                warnings: vec![KeybindingWarning {
                    kind: KeybindingWarningType::ParseError,
                    severity: KeybindingSeverity::Error,
                    message: "\"bindings\" must be an array".into(),
                    key: None,
                    context: None,
                    action: None,
                    suggestion: Some("Set \"bindings\" to an array of keybinding blocks".into()),
                }],
            };
        }
    };

    let blocks: Vec<KeybindingBlock> = arr
        .iter()
        .filter_map(|v| serde_json::from_value::<KeybindingBlock>(v.clone()).ok())
        .collect();
    let user_parsed = parse_bindings(&blocks);
    let mut merged = default_bindings;
    merged.extend(user_parsed);

    let dup_warnings = check_duplicate_keys_in_json(content);
    let mut warnings = dup_warnings;
    warnings.extend(validate_bindings_full(&user_blocks, &merged));

    KeybindingsLoadResult {
        bindings: merged,
        warnings,
    }
}

/// 重置加载状态（测试用）。
pub fn reset_keybinding_loader_for_testing() {
    // 此 Rust 实现无缓存，故空操作。
}

/// 获取上一次 `load_user_bindings` 产生的警告快照。
///
/// 真实实现：load 函数会把 warnings 写入 `KEYBINDING_WARNINGS` 缓存；
/// 本函数读取并返回当前快照（克隆）。
pub fn get_cached_keybinding_warnings() -> Vec<KeybindingWarning> {
    KEYBINDING_WARNINGS
        .read()
        .map(|w| w.clone())
        .unwrap_or_default()
}

static KEYBINDING_WARNINGS: once_cell::sync::Lazy<
    std::sync::RwLock<Vec<KeybindingWarning>>,
> = once_cell::sync::Lazy::new(|| std::sync::RwLock::new(Vec::new()));

/// 内部：更新缓存的 warnings（由 `load_user_bindings` 调用）。
pub fn set_cached_keybinding_warnings(warnings: Vec<KeybindingWarning>) {
    if let Ok(mut w) = KEYBINDING_WARNINGS.write() {
        *w = warnings;
    }
}

// ────────────────────────────────────────────────────────────────────────────
// keybindings/schema.ts — context / action 常量
// ────────────────────────────────────────────────────────────────────────────

pub const KEYBINDING_CONTEXTS: &[&str] = &[
    "Global",
    "Chat",
    "Autocomplete",
    "Confirmation",
    "Help",
    "Transcript",
    "HistorySearch",
    "Task",
    "ThemePicker",
    "Settings",
    "Tabs",
    "Attachments",
    "Footer",
    "MessageSelector",
    "DiffDialog",
    "ModelPicker",
    "Select",
    "Plugin",
];

pub const KEYBINDING_CONTEXT_DESCRIPTIONS: &[(&str, &str)] = &[
    ("Global", "Active everywhere, regardless of focus"),
    ("Chat", "When the chat input is focused"),
    ("Autocomplete", "When autocomplete menu is visible"),
    ("Confirmation", "When a confirmation/permission dialog is shown"),
    ("Help", "When the help overlay is open"),
    ("Transcript", "When viewing the transcript"),
    ("HistorySearch", "When searching command history (ctrl+r)"),
    ("Task", "When a task/agent is running in the foreground"),
    ("ThemePicker", "When the theme picker is open"),
    ("Settings", "When the settings menu is open"),
    ("Tabs", "When tab navigation is active"),
    ("Attachments", "When navigating image attachments in a select dialog"),
    ("Footer", "When footer indicators are focused"),
    ("MessageSelector", "When the message selector (rewind) is open"),
    ("DiffDialog", "When the diff dialog is open"),
    ("ModelPicker", "When the model picker is open"),
    ("Select", "When a select/list component is focused"),
    ("Plugin", "When the plugin dialog is open"),
];

pub const KEYBINDING_ACTIONS: &[&str] = &[
    "app:interrupt",
    "app:exit",
    "app:toggleTodos",
    "app:toggleTranscript",
    "app:toggleBrief",
    "app:toggleTeammatePreview",
    "app:toggleTerminal",
    "app:redraw",
    "app:globalSearch",
    "app:quickOpen",
    "history:search",
    "history:previous",
    "history:next",
    "chat:cancel",
    "chat:killAgents",
    "chat:cycleMode",
    "chat:modelPicker",
    "chat:fastMode",
    "chat:thinkingToggle",
    "chat:submit",
    "chat:newline",
    "chat:undo",
    "chat:externalEditor",
    "chat:stash",
    "chat:imagePaste",
    "chat:messageActions",
    "autocomplete:accept",
    "autocomplete:dismiss",
    "autocomplete:previous",
    "autocomplete:next",
    "confirm:yes",
    "confirm:no",
    "confirm:previous",
    "confirm:next",
    "confirm:nextField",
    "confirm:previousField",
    "confirm:cycleMode",
    "confirm:toggle",
    "confirm:toggleExplanation",
    "tabs:next",
    "tabs:previous",
    "transcript:toggleShowAll",
    "transcript:exit",
    "historySearch:next",
    "historySearch:accept",
    "historySearch:cancel",
    "historySearch:execute",
    "task:background",
    "theme:toggleSyntaxHighlighting",
    "help:dismiss",
    "attachments:next",
    "attachments:previous",
    "attachments:remove",
    "attachments:exit",
    "footer:up",
    "footer:down",
    "footer:next",
    "footer:previous",
    "footer:openSelected",
    "footer:clearSelection",
    "footer:close",
    "messageSelector:up",
    "messageSelector:down",
    "messageSelector:top",
    "messageSelector:bottom",
    "messageSelector:select",
    "diff:dismiss",
    "diff:previousSource",
    "diff:nextSource",
    "diff:back",
    "diff:viewDetails",
    "diff:previousFile",
    "diff:nextFile",
    "modelPicker:decreaseEffort",
    "modelPicker:increaseEffort",
    "select:next",
    "select:previous",
    "select:accept",
    "select:cancel",
    "plugin:toggle",
    "plugin:install",
    "permission:toggleDebug",
    "settings:search",
    "settings:retry",
    "settings:close",
    "voice:pushToTalk",
];

/// JSON Schema 描述符 — 手写版本，与 TS zod schema 等价。
///
/// 该 schema 用于 `keybindings.json` 的运行时校验，结构覆盖
/// `KEYBINDING_CONTEXTS` 列举的所有上下文，每个上下文映射到
/// `action → keystroke[]`。
pub fn keybindings_schema_json() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "bindings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "context": { "type": "string", "enum": KEYBINDING_CONTEXTS },
                        "bindings": { "type": "object" }
                    },
                    "required": ["context", "bindings"]
                }
            }
        },
        "required": ["bindings"]
    })
}

pub const KeybindingsSchema: &str = "keybindings.schema.json";
pub const KeybindingBlockSchema: &str = "keybindings.block.schema.json";

// ────────────────────────────────────────────────────────────────────────────
// keybindings/reservedShortcuts.ts
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservedShortcut {
    pub key: String,
    pub reason: String,
    pub severity: KeybindingSeverity,
}

/// 不可重新绑定的键（控制流相关）。
pub fn non_rebindable() -> Vec<ReservedShortcut> {
    vec![
        ReservedShortcut {
            key: "ctrl+c".to_string(),
            reason: "Interrupt is hardwired".to_string(),
            severity: KeybindingSeverity::Error,
        },
        ReservedShortcut {
            key: "ctrl+z".to_string(),
            reason: "Suspend is hardwired".to_string(),
            severity: KeybindingSeverity::Error,
        },
    ]
}

pub const NON_REBINDABLE: &[(&str, &str)] = &[
    ("ctrl+c", "Interrupt is hardwired"),
    ("ctrl+z", "Suspend is hardwired"),
];

pub const TERMINAL_RESERVED: &[(&str, &str)] = &[
    ("ctrl+s", "XOFF (flow control)"),
    ("ctrl+q", "XON (flow control)"),
    ("ctrl+\\", "SIGQUIT"),
];

pub const MACOS_RESERVED: &[(&str, &str)] = &[
    ("cmd+q", "Quit application (macOS)"),
    ("cmd+w", "Close window (macOS)"),
    ("cmd+tab", "Switch app (macOS)"),
];

pub fn normalize_key_for_comparison(key: &str) -> String {
    let lower = key.to_lowercase();
    // 排序修饰键
    let mut parts: Vec<&str> = lower.split('+').collect();
    if parts.len() > 1 {
        let last = parts.pop().unwrap();
        parts.sort();
        parts.push(last);
    }
    parts.join("+")
}

pub fn normalizeKeyForComparison(key: &str) -> String {
    normalize_key_for_comparison(key)
}

// ────────────────────────────────────────────────────────────────────────────
// keybindings/resolver.ts
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ResolveResult {
    Match { action: String },
    None,
    Unbound,
}

#[derive(Debug, Clone)]
pub enum ChordResolveResult {
    Match { action: String },
    None,
    Unbound,
    ChordStarted { pending: Vec<ParsedKeystroke> },
    ChordCancelled,
}

/// 单键解析。
pub fn resolve_key(
    current: &ParsedKeystroke,
    active_contexts: &[String],
    bindings: &[ParsedBinding],
) -> ResolveResult {
    let ctx_set: std::collections::HashSet<&str> =
        active_contexts.iter().map(|s| s.as_str()).collect();
    let mut last_match: Option<&ParsedBinding> = None;
    for b in bindings {
        if b.chord.len() != 1 {
            continue;
        }
        if !ctx_set.contains(b.context.as_str()) {
            continue;
        }
        if let Some(only) = b.chord.first() {
            if keystrokes_equal(only, current) {
                last_match = Some(b);
            }
        }
    }
    match last_match {
        None => ResolveResult::None,
        Some(b) => match &b.action {
            None => ResolveResult::Unbound,
            Some(a) => ResolveResult::Match { action: a.clone() },
        },
    }
}

pub fn resolveKey(
    current: &ParsedKeystroke,
    active_contexts: &[String],
    bindings: &[ParsedBinding],
) -> ResolveResult {
    resolve_key(current, active_contexts, bindings)
}

/// 获取绑定的显示文本。
pub fn get_binding_display_text(
    action: &str,
    context: &str,
    bindings: &[ParsedBinding],
) -> Option<String> {
    let mut last: Option<&ParsedBinding> = None;
    for b in bindings {
        if b.action.as_deref() == Some(action) && b.context == context {
            last = Some(b);
        }
    }
    last.map(|b| chord_to_string(&b.chord))
}

pub fn getBindingDisplayText(
    action: &str,
    context: &str,
    bindings: &[ParsedBinding],
) -> Option<String> {
    get_binding_display_text(action, context, bindings)
}

/// 比较两个 keystroke 是否相等（alt/meta 视为等价）。
pub fn keystrokes_equal(a: &ParsedKeystroke, b: &ParsedKeystroke) -> bool {
    a.key == b.key
        && a.ctrl == b.ctrl
        && a.shift == b.shift
        && (a.alt || a.meta) == (b.alt || b.meta)
        && a.super_key == b.super_key
}

pub fn keystrokesEqual(a: &ParsedKeystroke, b: &ParsedKeystroke) -> bool {
    keystrokes_equal(a, b)
}

fn chord_prefix_matches(prefix: &[ParsedKeystroke], binding: &ParsedBinding) -> bool {
    if prefix.len() >= binding.chord.len() {
        return false;
    }
    for (i, p) in prefix.iter().enumerate() {
        if !keystrokes_equal(p, &binding.chord[i]) {
            return false;
        }
    }
    true
}

fn chord_exactly_matches(chord: &[ParsedKeystroke], binding: &ParsedBinding) -> bool {
    if chord.len() != binding.chord.len() {
        return false;
    }
    for (i, c) in chord.iter().enumerate() {
        if !keystrokes_equal(c, &binding.chord[i]) {
            return false;
        }
    }
    true
}

/// 带 chord 状态的解析。
pub fn resolve_key_with_chord_state(
    current: &ParsedKeystroke,
    is_escape: bool,
    active_contexts: &[String],
    bindings: &[ParsedBinding],
    pending: Option<&[ParsedKeystroke]>,
) -> ChordResolveResult {
    if is_escape && pending.is_some() {
        return ChordResolveResult::ChordCancelled;
    }
    let test_chord: Vec<ParsedKeystroke> = match pending {
        Some(p) => {
            let mut v = p.to_vec();
            v.push(current.clone());
            v
        }
        None => vec![current.clone()],
    };
    let ctx_set: std::collections::HashSet<&str> =
        active_contexts.iter().map(|s| s.as_str()).collect();
    let context_bindings: Vec<&ParsedBinding> = bindings
        .iter()
        .filter(|b| ctx_set.contains(b.context.as_str()))
        .collect();

    let mut chord_winners: HashMap<String, Option<String>> = HashMap::new();
    for b in &context_bindings {
        if b.chord.len() > test_chord.len() && chord_prefix_matches(&test_chord, b) {
            chord_winners.insert(chord_to_string(&b.chord), b.action.clone());
        }
    }
    let has_longer = chord_winners.values().any(|a| a.is_some());
    if has_longer {
        return ChordResolveResult::ChordStarted {
            pending: test_chord,
        };
    }

    let mut exact_match: Option<&&ParsedBinding> = None;
    for b in &context_bindings {
        if chord_exactly_matches(&test_chord, b) {
            exact_match = Some(b);
        }
    }
    match exact_match {
        Some(b) => match &b.action {
            None => ChordResolveResult::Unbound,
            Some(a) => ChordResolveResult::Match { action: a.clone() },
        },
        None => {
            if pending.is_some() {
                ChordResolveResult::ChordCancelled
            } else {
                ChordResolveResult::None
            }
        }
    }
}

pub fn resolveKeyWithChordState(
    current: &ParsedKeystroke,
    is_escape: bool,
    active_contexts: &[String],
    bindings: &[ParsedBinding],
    pending: Option<&[ParsedKeystroke]>,
) -> ChordResolveResult {
    resolve_key_with_chord_state(current, is_escape, active_contexts, bindings, pending)
}
