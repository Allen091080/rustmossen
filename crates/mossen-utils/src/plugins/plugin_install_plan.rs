use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use rand::Rng;
use tracing::debug;

use super::schemas::{PluginMarketplaceEntry, PluginSource};

/// TTL for install plan tokens (10 minutes).
pub const PLUGIN_INSTALL_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

const GITHUB_DIRECT_MARKETPLACE: &str = "github-direct";

/// A plugin install plan with all resolution details.
#[derive(Debug, Clone)]
pub struct PluginInstallPlan {
    pub token: String,
    pub created_at: u64,
    pub plugin_id: String,
    pub plugin_name: String,
    pub marketplace_name: String,
    pub scope: String,
    pub entry: PluginMarketplaceEntry,
    pub marketplace_install_location: Option<String>,
    pub dependency_closure: Vec<String>,
    pub dep_note: String,
    pub source_description: Option<String>,
}

/// Errors that can occur during install plan creation/execution.
#[derive(Debug, Clone)]
pub enum PluginInstallPlanError {
    MissingPlugin,
    PluginNotFound { plugin: String },
    MarketplaceRequired { plugin: String },
    InvalidGithubTarget { reason: String },
    InvalidScope { scope: Option<String> },
    BlockedByPolicy { plugin_id: String },
    ResolutionFailed { message: String },
    UnknownToken { token: String },
    ExpiredToken { token: String },
    InstallFailed { message: String },
}

pub type PluginInstallPlanResult = Result<PluginInstallPlan, PluginInstallPlanError>;

static PLAN_STORE: Lazy<Mutex<HashMap<String, PluginInstallPlan>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
struct GitHubPluginTarget {
    owner: String,
    repo: String,
    ref_name: Option<String>,
    path: String,
    original: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GitHubContentsItem {
    #[serde(rename = "type")]
    item_type: String,
    name: String,
    path: String,
    download_url: Option<String>,
}

fn prune_expired_plans(now: u64) {
    let mut store = PLAN_STORE.lock().unwrap();
    store.retain(|_, plan| now - plan.created_at <= PLUGIN_INSTALL_PLAN_TOKEN_TTL_MS);
}

fn create_token() -> String {
    let mut rng = rand::thread_rng();
    let store = PLAN_STORE.lock().unwrap();
    loop {
        let token = format!("{:08x}", rng.gen::<u32>());
        if !store.contains_key(&token) {
            return token;
        }
    }
}

fn normalize_scope(scope: Option<&str>) -> Option<String> {
    match scope {
        None => Some("user".to_string()),
        Some("user") | Some("project") | Some("local") => Some(scope.unwrap().to_string()),
        _ => None,
    }
}

/// Trait for plugin resolution dependencies.
#[async_trait::async_trait]
pub trait PluginInstallResolver: Send + Sync {
    async fn get_plugin_by_id(
        &self,
        id: &str,
    ) -> Option<(PluginMarketplaceEntry, String)>;
    async fn get_marketplace_cache_only(
        &self,
        marketplace: &str,
    ) -> Option<Vec<String>>;
    fn get_enabled_plugin_ids_for_scope(&self, scope: &str) -> HashSet<String>;
    fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool;
    fn parse_plugin_identifier(&self, id: &str) -> (String, Option<String>);
    fn scope_to_setting_source(&self, scope: &str) -> String;
    async fn resolve_dependency_closure(
        &self,
        plugin_id: &str,
        enabled_ids: &HashSet<String>,
        allowed_cross: &HashSet<String>,
    ) -> DependencyResolution;
    fn format_resolution_error(&self, resolution: &DependencyResolution) -> String;
    fn format_dependency_count_suffix(&self, deps: &[String]) -> String;
}

#[derive(Debug, Clone)]
pub struct DependencyResolution {
    pub ok: bool,
    pub closure: Vec<String>,
    pub error_reason: Option<String>,
}

/// Get a plugin install plan for the given options.
pub async fn get_plugin_install_plan(
    plugin: Option<&str>,
    scope: Option<&str>,
    resolver: &dyn PluginInstallResolver,
) -> PluginInstallPlanResult {
    let now = current_time_ms();
    prune_expired_plans(now);

    let requested_plugin = match plugin.map(|s| s.trim()) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return Err(PluginInstallPlanError::MissingPlugin),
    };

    let install_scope = match normalize_scope(scope) {
        Some(s) => s,
        None => {
            return Err(PluginInstallPlanError::InvalidScope {
                scope: scope.map(|s| s.to_string()),
            });
        }
    };

    // Check if it's a GitHub target
    if let Some(github_target) = parse_github_plugin_target(&requested_plugin) {
        return get_github_direct_plugin_install_plan(github_target, &install_scope, resolver).await;
    }

    let (name, marketplace) = resolver.parse_plugin_identifier(&requested_plugin);
    if marketplace.is_none() {
        return Err(PluginInstallPlanError::MarketplaceRequired {
            plugin: requested_plugin,
        });
    }
    let marketplace_name = marketplace.unwrap();

    let info = match resolver.get_plugin_by_id(&requested_plugin).await {
        Some(info) => info,
        None => {
            return Err(PluginInstallPlanError::PluginNotFound {
                plugin: requested_plugin,
            });
        }
    };

    let plugin_id = format!("{}@{}", info.0.name, marketplace_name);
    if resolver.is_plugin_blocked_by_policy(&plugin_id) {
        return Err(PluginInstallPlanError::BlockedByPolicy { plugin_id });
    }

    let allowed_cross = resolver
        .get_marketplace_cache_only(&marketplace_name)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<String>>();

    let setting_source = resolver.scope_to_setting_source(&install_scope);
    let enabled_ids = resolver.get_enabled_plugin_ids_for_scope(&setting_source);

    let resolution = resolver
        .resolve_dependency_closure(&plugin_id, &enabled_ids, &allowed_cross)
        .await;

    if !resolution.ok {
        return Err(PluginInstallPlanError::ResolutionFailed {
            message: resolver.format_resolution_error(&resolution),
        });
    }

    // Check policy for all dependencies
    for id in &resolution.closure {
        if resolver.is_plugin_blocked_by_policy(id) {
            return Err(PluginInstallPlanError::BlockedByPolicy {
                plugin_id: id.clone(),
            });
        }
    }

    let dep_note = resolver.format_dependency_count_suffix(
        &resolution
            .closure
            .iter()
            .filter(|id| *id != &plugin_id)
            .cloned()
            .collect::<Vec<_>>(),
    );

    let token = create_token();
    let plan = PluginInstallPlan {
        token: token.clone(),
        created_at: now,
        plugin_id: plugin_id.clone(),
        plugin_name: info.0.name.clone(),
        marketplace_name,
        scope: install_scope,
        entry: info.0,
        marketplace_install_location: Some(info.1),
        dependency_closure: resolution.closure,
        dep_note,
        source_description: None,
    };

    PLAN_STORE.lock().unwrap().insert(token, plan.clone());
    Ok(plan)
}

async fn get_github_direct_plugin_install_plan(
    target: GitHubPluginTarget,
    scope: &str,
    resolver: &dyn PluginInstallResolver,
) -> PluginInstallPlanResult {
    // For GitHub direct, we'd need HTTP calls to resolve ref and fetch manifest.
    // This is a simplified version that constructs the plan structure.
    let target = resolve_default_ref(target).await;

    let manifest_result = load_github_plugin_manifest(&target).await;
    let manifest = match manifest_result {
        Ok(m) => m,
        Err(reason) => {
            return Err(PluginInstallPlanError::InvalidGithubTarget { reason });
        }
    };

    let plugin_name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let source = build_github_plugin_source(&target);
    let entry = PluginMarketplaceEntry {
        name: plugin_name.clone(),
        description: manifest
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        source,
        version: manifest
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ..Default::default()
    };

    let plugin_id = format!("{}@{}", entry.name, GITHUB_DIRECT_MARKETPLACE);
    if resolver.is_plugin_blocked_by_policy(&plugin_id) {
        return Err(PluginInstallPlanError::BlockedByPolicy { plugin_id });
    }

    let setting_source = resolver.scope_to_setting_source(scope);
    let enabled_ids = resolver.get_enabled_plugin_ids_for_scope(&setting_source);
    let resolution = resolver
        .resolve_dependency_closure(&plugin_id, &enabled_ids, &HashSet::new())
        .await;

    if !resolution.ok {
        return Err(PluginInstallPlanError::ResolutionFailed {
            message: resolver.format_resolution_error(&resolution),
        });
    }

    for id in &resolution.closure {
        if resolver.is_plugin_blocked_by_policy(id) {
            return Err(PluginInstallPlanError::BlockedByPolicy {
                plugin_id: id.clone(),
            });
        }
    }

    let dep_note = resolver.format_dependency_count_suffix(
        &resolution
            .closure
            .iter()
            .filter(|id| *id != &plugin_id)
            .cloned()
            .collect::<Vec<_>>(),
    );

    let token = create_token();
    let source_desc = format!(
        "{}/{}@{}{}",
        target.owner,
        target.repo,
        target.ref_name.as_deref().unwrap_or("main"),
        if target.path.is_empty() {
            String::new()
        } else {
            format!("/{}", target.path)
        }
    );

    let plan = PluginInstallPlan {
        token: token.clone(),
        created_at: current_time_ms(),
        plugin_id,
        plugin_name,
        marketplace_name: GITHUB_DIRECT_MARKETPLACE.to_string(),
        scope: scope.to_string(),
        entry,
        marketplace_install_location: None,
        dependency_closure: resolution.closure,
        dep_note,
        source_description: Some(source_desc),
    };

    PLAN_STORE.lock().unwrap().insert(token, plan.clone());
    Ok(plan)
}

/// Execute a previously-created install plan by token.
pub async fn execute_plugin_install_plan(
    token: &str,
    installer: &dyn PluginInstaller,
) -> PluginInstallPlanResult {
    let now = current_time_ms();
    prune_expired_plans(now);

    let plan = {
        let mut store = PLAN_STORE.lock().unwrap();
        match store.remove(token) {
            Some(p) => p,
            None => {
                return Err(PluginInstallPlanError::UnknownToken {
                    token: token.to_string(),
                });
            }
        }
    };

    if now - plan.created_at > PLUGIN_INSTALL_PLAN_TOKEN_TTL_MS {
        return Err(PluginInstallPlanError::ExpiredToken {
            token: token.to_string(),
        });
    }

    match installer.install_resolved_plugin(&plan).await {
        Ok(result) => Ok(PluginInstallPlan {
            dependency_closure: result.closure,
            dep_note: result.dep_note,
            ..plan
        }),
        Err(e) => Err(PluginInstallPlanError::InstallFailed { message: e }),
    }
}

/// Trait for actual plugin installation.
#[async_trait::async_trait]
pub trait PluginInstaller: Send + Sync {
    async fn install_resolved_plugin(
        &self,
        plan: &PluginInstallPlan,
    ) -> Result<InstallResult, String>;
}

pub struct InstallResult {
    pub closure: Vec<String>,
    pub dep_note: String,
}

/// Reset plan store for testing.
pub fn reset_plugin_install_plan_store_for_testing() {
    PLAN_STORE.lock().unwrap().clear();
}

fn parse_github_plugin_target(input: &str) -> Option<GitHubPluginTarget> {
    let trimmed = input.trim();

    // Shorthand: owner/repo
    let re_shorthand = regex::Regex::new(r"^([A-Za-z0-9_.\-]+)/([A-Za-z0-9_.\-]+)$").unwrap();
    if let Some(caps) = re_shorthand.captures(trimmed) {
        return Some(GitHubPluginTarget {
            owner: caps[1].to_string(),
            repo: strip_git_suffix(&caps[2]),
            ref_name: None,
            path: String::new(),
            original: trimmed.to_string(),
        });
    }

    // URL form
    let url = url::Url::parse(trimmed).ok()?;
    let host = url.host_str()?;
    if host != "github.com" && host != "www.github.com" {
        return None;
    }

    let parts: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    let owner = parts.first()?.to_string();
    let repo = strip_git_suffix(parts.get(1)?);
    if parts.len() <= 2 {
        return Some(GitHubPluginTarget {
            owner,
            repo,
            ref_name: None,
            path: String::new(),
            original: trimmed.to_string(),
        });
    }

    let kind = parts.get(2)?;
    if *kind != "tree" && *kind != "blob" {
        return None;
    }

    let ref_name = parts.get(3)?.to_string();
    let raw_path = parts[4..].join("/");

    Some(GitHubPluginTarget {
        owner,
        repo,
        ref_name: Some(ref_name),
        path: plugin_root_from_github_path(&raw_path),
        original: trimmed.to_string(),
    })
}

fn plugin_root_from_github_path(path: &str) -> String {
    let normalized = normalize_github_path(path);
    if normalized.ends_with("/.mossen-plugin/plugin.json") {
        let parent = &normalized[..normalized.len() - "/.mossen-plugin/plugin.json".len()];
        return normalize_github_path(parent);
    }
    if normalized == ".mossen-plugin/plugin.json" {
        return String::new();
    }
    if normalized.ends_with("/plugin.json") {
        let parent = &normalized[..normalized.len() - "/plugin.json".len()];
        return normalize_github_path(parent);
    }
    if normalized == "plugin.json" {
        return String::new();
    }
    normalized
}

fn strip_git_suffix(value: &str) -> String {
    if value.ends_with(".git") {
        value[..value.len() - 4].to_string()
    } else {
        value.to_string()
    }
}

async fn resolve_default_ref(target: GitHubPluginTarget) -> GitHubPluginTarget {
    if target.ref_name.is_some() {
        return target;
    }
    // In production, this would call GitHub API to get default branch.
    // For now, default to "main".
    GitHubPluginTarget {
        ref_name: Some("main".to_string()),
        ..target
    }
}

async fn load_github_plugin_manifest(
    _target: &GitHubPluginTarget,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    // In production, this would fetch from GitHub API.
    // Placeholder that returns error since we can't actually fetch.
    Err("No plugin manifest found. Expected .mossen-plugin/plugin.json or plugin.json at the GitHub target.".to_string())
}

fn build_github_plugin_source(target: &GitHubPluginTarget) -> PluginSource {
    let url = format!("https://github.com/{}/{}.git", target.owner, target.repo);
    if !target.path.is_empty() {
        PluginSource::Structured(super::schemas::StructuredPluginSource::GitSubdir {
            url,
            path: target.path.clone(),
            git_ref: target.ref_name.clone(),
            sha: None,
        })
    } else {
        PluginSource::Structured(super::schemas::StructuredPluginSource::Url {
            url,
            git_ref: target.ref_name.clone(),
            sha: None,
        })
    }
}

fn normalize_github_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    if normalized == "." {
        String::new()
    } else {
        normalized
    }
}

fn join_github_path(root: &str, child: &str) -> String {
    let combined = [root, child]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("/");
    normalize_github_path(&combined)
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
