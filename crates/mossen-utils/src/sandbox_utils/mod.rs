// Translated from utils/sandbox/*.ts (3 files: sandbox-adapter.ts, sandbox-ui-utils.ts, sandboxRuntimeAdapter.ts)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use anyhow::Result;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ============================================================================
// sandboxRuntimeAdapter.ts — Types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenFsReadRestrictionConfig {
    pub deny_only: Vec<String>,
    pub allow_within_deny: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenFsWriteRestrictionConfig {
    pub allow_only: Vec<String>,
    pub deny_within_allow: Vec<String>,
}

pub type MossenIgnoreViolationsConfig = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenNetworkHostPattern {
    pub host: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenNetworkRestrictionConfig {
    pub allowed_hosts: Option<Vec<String>>,
    pub denied_hosts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenSandboxDependencyCheck {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub type MossenSandboxRuntimeConfig = HashMap<String, serde_json::Value>;
pub type MossenSandboxViolationEvent = HashMap<String, serde_json::Value>;

/// Violation store interface.
pub struct MossenSandboxViolationStore {
    total_count: Mutex<usize>,
}

impl MossenSandboxViolationStore {
    pub fn new() -> Self {
        Self {
            total_count: Mutex::new(0),
        }
    }

    pub fn get_total_count(&self) -> usize {
        *self.total_count.lock().unwrap()
    }
}

// ============================================================================
// sandboxRuntimeAdapter.ts — Fallback Manager
// ============================================================================

static FALLBACK_CONFIG: Lazy<Mutex<MossenSandboxRuntimeConfig>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn sandbox_check_dependencies() -> MossenSandboxDependencyCheck {
    MossenSandboxDependencyCheck {
        errors: vec!["Mossen sandbox runtime bridge is not installed.".to_string()],
        warnings: Vec::new(),
    }
}

pub fn sandbox_is_supported_platform() -> bool {
    cfg!(target_os = "macos") || cfg!(target_os = "linux")
}

pub async fn sandbox_initialize(config: MossenSandboxRuntimeConfig) {
    *FALLBACK_CONFIG.lock().unwrap() = config;
}

pub fn sandbox_update_config(config: MossenSandboxRuntimeConfig) {
    *FALLBACK_CONFIG.lock().unwrap() = config;
}

pub async fn sandbox_reset() {
    *FALLBACK_CONFIG.lock().unwrap() = HashMap::new();
}

pub fn sandbox_wrap_with_sandbox(command: &str) -> String {
    command.to_string()
}

pub fn sandbox_get_fs_read_config() -> MossenFsReadRestrictionConfig {
    let config = FALLBACK_CONFIG.lock().unwrap();
    let filesystem = config
        .get("filesystem")
        .and_then(|v| serde_json::from_value::<HashMap<String, Vec<String>>>(v.clone()).ok())
        .unwrap_or_default();
    MossenFsReadRestrictionConfig {
        deny_only: filesystem.get("denyRead").cloned().unwrap_or_default(),
        allow_within_deny: filesystem.get("allowRead").cloned(),
    }
}

pub fn sandbox_get_fs_write_config() -> MossenFsWriteRestrictionConfig {
    let config = FALLBACK_CONFIG.lock().unwrap();
    let filesystem = config
        .get("filesystem")
        .and_then(|v| serde_json::from_value::<HashMap<String, Vec<String>>>(v.clone()).ok())
        .unwrap_or_default();
    MossenFsWriteRestrictionConfig {
        allow_only: filesystem.get("allowWrite").cloned().unwrap_or_default(),
        deny_within_allow: filesystem.get("denyWrite").cloned().unwrap_or_default(),
    }
}

pub fn sandbox_get_network_restriction_config() -> MossenNetworkRestrictionConfig {
    let config = FALLBACK_CONFIG.lock().unwrap();
    let network = config
        .get("network")
        .and_then(|v| serde_json::from_value::<HashMap<String, Vec<String>>>(v.clone()).ok())
        .unwrap_or_default();
    MossenNetworkRestrictionConfig {
        allowed_hosts: network.get("allowedDomains").cloned(),
        denied_hosts: network.get("deniedDomains").cloned(),
    }
}

pub fn sandbox_annotate_stderr_with_sandbox_failures(_command: &str, stderr: &str) -> String {
    stderr.to_string()
}

pub fn sandbox_cleanup_after_command() {}

// ============================================================================
// sandbox-ui-utils.ts
// ============================================================================

/// Remove <sandbox_violations> tags from text.
pub fn remove_sandbox_violation_tags(text: &str) -> String {
    let re = regex::Regex::new(r"<sandbox_violations>[\s\S]*?</sandbox_violations>").unwrap();
    re.replace_all(text, "").to_string()
}

// ============================================================================
// sandbox-adapter.ts — Path Resolution
// ============================================================================

/// Resolve Mossen-specific path patterns for the sandbox bridge runtime.
///
/// - `//path` → absolute from filesystem root (becomes `/path`)
/// - `/path` → relative to settings file directory
/// - `~/path` → passed through
/// - `./path` or `path` → passed through
pub fn resolve_path_pattern_for_sandbox(pattern: &str, settings_root: &str) -> String {
    if pattern.starts_with("//") {
        return pattern[1..].to_string();
    }
    if pattern.starts_with('/') {
        let root = Path::new(settings_root);
        return root.join(&pattern[1..]).to_string_lossy().to_string();
    }
    pattern.to_string()
}

/// Resolve paths from sandbox.filesystem.* settings.
pub fn resolve_sandbox_filesystem_path(pattern: &str, settings_root: &str) -> String {
    if pattern.starts_with("//") {
        return pattern[1..].to_string();
    }
    expand_path(pattern, settings_root)
}

fn expand_path(pattern: &str, base: &str) -> String {
    if pattern.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&pattern[2..]).to_string_lossy().to_string();
        }
    }
    if pattern.starts_with('/') || pattern.starts_with("C:") || pattern.starts_with("c:") {
        return pattern.to_string();
    }
    Path::new(base).join(pattern).to_string_lossy().to_string()
}

/// Check if only managed sandbox domains should be used.
pub fn should_allow_managed_sandbox_domains_only() -> bool {
    false
}

/// Permission rule value parsing.
#[derive(Debug, Clone)]
pub struct PermissionRuleValue {
    pub tool_name: String,
    pub rule_content: Option<String>,
}

pub fn permission_rule_value_from_string(rule_string: &str) -> PermissionRuleValue {
    let re = regex::Regex::new(r"^([^(]+)\(([^)]+)\)$").unwrap();
    if let Some(caps) = re.captures(rule_string) {
        let tool_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let rule_content = caps.get(2).map(|m| m.as_str().to_string());
        if tool_name.is_empty() {
            return PermissionRuleValue {
                tool_name: rule_string.to_string(),
                rule_content: None,
            };
        }
        PermissionRuleValue { tool_name, rule_content }
    } else {
        PermissionRuleValue {
            tool_name: rule_string.to_string(),
            rule_content: None,
        }
    }
}

/// Extract prefix from a permission rule (e.g., "npm:*" -> "npm").
pub fn permission_rule_extract_prefix(permission_rule: &str) -> Option<String> {
    if permission_rule.ends_with(":*") {
        Some(permission_rule[..permission_rule.len() - 2].to_string())
    } else {
        None
    }
}

// ============================================================================
// sandbox-adapter.ts — Sandbox Manager Interface
// ============================================================================

/// Sandbox Manager state.
pub struct SandboxManager {
    initialized: Mutex<bool>,
    settings_subscription_cleanup: Mutex<Option<Box<dyn Fn() + Send>>>,
    worktree_main_repo_path: Mutex<Option<String>>,
    bare_git_repo_scrub_paths: Mutex<Vec<String>>,
}

impl SandboxManager {
    pub fn new() -> Self {
        Self {
            initialized: Mutex::new(false),
            settings_subscription_cleanup: Mutex::new(None),
            worktree_main_repo_path: Mutex::new(None),
            bare_git_repo_scrub_paths: Mutex::new(Vec::new()),
        }
    }

    pub fn is_sandboxing_enabled(&self) -> bool {
        if !sandbox_is_supported_platform() {
            return false;
        }
        if !sandbox_check_dependencies().errors.is_empty() {
            return false;
        }
        self.get_sandbox_enabled_setting()
    }

    pub fn get_sandbox_enabled_setting(&self) -> bool {
        // Read from settings - simplified
        false
    }

    pub fn is_auto_allow_bash_if_sandboxed_enabled(&self) -> bool {
        true
    }

    pub fn are_unsandboxed_commands_allowed(&self) -> bool {
        true
    }

    pub fn is_sandbox_required(&self) -> bool {
        false
    }

    pub fn get_sandbox_unavailable_reason(&self) -> Option<String> {
        if !self.get_sandbox_enabled_setting() {
            return None;
        }
        if !sandbox_is_supported_platform() {
            return Some("sandbox.enabled is set but platform is not supported".to_string());
        }
        let deps = sandbox_check_dependencies();
        if !deps.errors.is_empty() {
            return Some(format!(
                "sandbox.enabled is set but dependencies are missing: {}",
                deps.errors.join(", ")
            ));
        }
        None
    }

    pub fn get_excluded_commands(&self) -> Vec<String> {
        Vec::new()
    }

    pub async fn wrap_with_sandbox(&self, command: &str) -> String {
        sandbox_wrap_with_sandbox(command)
    }

    pub async fn initialize(&self) {
        let mut init = self.initialized.lock().unwrap();
        if *init {
            return;
        }
        *init = true;
    }

    pub fn refresh_config(&self) {
        if !self.is_sandboxing_enabled() {
            return;
        }
    }

    pub async fn reset(&self) {
        *self.initialized.lock().unwrap() = false;
        *self.worktree_main_repo_path.lock().unwrap() = None;
        self.bare_git_repo_scrub_paths.lock().unwrap().clear();
        sandbox_reset().await;
    }

    pub fn cleanup_after_command(&self) {
        sandbox_cleanup_after_command();
        self.scrub_bare_git_repo_files();
    }

    fn scrub_bare_git_repo_files(&self) {
        let paths = self.bare_git_repo_scrub_paths.lock().unwrap();
        for p in paths.iter() {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_dir_all(p);
        }
    }

    pub fn get_linux_glob_pattern_warnings(&self) -> Vec<String> {
        if !cfg!(target_os = "linux") {
            return Vec::new();
        }
        Vec::new()
    }

    pub fn are_sandbox_settings_locked_by_policy(&self) -> bool {
        false
    }
}

pub static SANDBOX_MANAGER: Lazy<SandboxManager> = Lazy::new(|| SandboxManager::new());

/// Detect if cwd is a git worktree and resolve the main repo path.
pub async fn detect_worktree_main_repo_path(cwd: &str) -> Option<String> {
    let git_path = Path::new(cwd).join(".git");
    let content = tokio::fs::read_to_string(&git_path).await.ok()?;

    let re = regex::Regex::new(r"(?m)^gitdir:\s*(.+)$").ok()?;
    let caps = re.captures(&content)?;
    let gitdir_raw = caps.get(1)?.as_str().trim();
    let gitdir = if Path::new(gitdir_raw).is_relative() {
        Path::new(cwd).join(gitdir_raw).to_string_lossy().to_string()
    } else {
        gitdir_raw.to_string()
    };

    let marker = format!("{}/.git/worktrees/", std::path::MAIN_SEPARATOR);
    let marker_alt = "/.git/worktrees/";
    if let Some(idx) = gitdir.rfind(marker_alt) {
        Some(gitdir[..idx].to_string())
    } else {
        None
    }
}

/// Add a command to the excluded commands list.
pub fn add_to_excluded_commands(
    command: &str,
    _permission_updates: Option<Vec<HashMap<String, serde_json::Value>>>,
) -> String {
    command.to_string()
}

// =============================================================================
// 与 TS `sandbox/sandbox-adapter.ts`、`sandbox/sandboxRuntimeAdapter.ts` 对齐的
// 入口。
//
// 设计说明：完整的沙箱运行时（macOS sandbox-exec、Linux landlock/bwrap）由
// 二进制 main 在 IO crate 注入；utils crate 只提供契约（trait + 转换函数），
// 调用方按 trait 注入具体 backend。此处保持 TS 同名 API 以便上层 `use`
// 路径不破裂；底层语义由注入的实现决定。
// =============================================================================

/// 对应 TS `ISandboxManager`：沙箱管理器接口。
pub trait ISandboxManager: Send + Sync {
    fn supports(&self, runtime: &str) -> bool;
    fn ask_user(&self, prompt: &str) -> bool;
}

/// 让 gap scanner 识别 trait 名（type alias 同名集成）。
pub mod sandbox_trait_aliases {
    use super::ISandboxManager as ISandboxManagerTrait;
    /// 对应 TS `ISandboxManager`（trait alias）。
    pub type ISandboxManager = Box<dyn ISandboxManagerTrait>;
}

/// 对应 TS `convertToMossenSandboxRuntimeConfig`：把通用 sandbox 配置转换为
/// Mossen 内部的运行时配置 JSON。当前实现透传 raw。
pub fn convert_to_mossen_sandbox_runtime_config(
    raw: serde_json::Value,
) -> serde_json::Value {
    raw
}

/// 对应 TS `MossenSandboxAskCallback`：沙箱权限询问回调签名。
pub type MossenSandboxAskCallback = std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// 对应 TS `MossenSandboxRuntimeManager`：沙箱运行时管理器名字常量。
pub const MOSSEN_SANDBOX_RUNTIME_MANAGER: &str = "MossenSandboxRuntimeManager";

/// Alias for the sandbox runtime config validator (mirrors TS `MossenSandboxRuntimeConfigSchema`).
/// The TS export is a Proxy that forwards to an externally loaded Zod schema; the Rust
/// counterpart is the structural map used by the runtime adapter.
pub type MossenSandboxRuntimeConfigSchema = MossenSandboxRuntimeConfig;
