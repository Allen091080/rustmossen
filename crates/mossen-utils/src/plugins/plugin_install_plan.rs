use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use rand::Rng;

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
    async fn get_plugin_by_id(&self, id: &str) -> Option<(PluginMarketplaceEntry, String)>;
    async fn get_marketplace_cache_only(&self, marketplace: &str) -> Option<Vec<String>>;
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
        return get_github_direct_plugin_install_plan(github_target, &install_scope, resolver)
            .await;
    }

    let (_name, marketplace) = resolver.parse_plugin_identifier(&requested_plugin);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    #[derive(Clone)]
    struct MockResolver {
        entries: HashMap<String, (PluginMarketplaceEntry, String)>,
        marketplace_plugins: Vec<String>,
        enabled_ids: HashSet<String>,
        blocked: HashSet<String>,
        resolution: DependencyResolution,
    }

    impl Default for MockResolver {
        fn default() -> Self {
            let mut entries = HashMap::new();
            entries.insert(
                "demo@market".to_string(),
                (
                    PluginMarketplaceEntry {
                        name: "demo".to_string(),
                        source: PluginSource::RelativePath("./demo".to_string()),
                        version: Some("1.0.0".to_string()),
                        description: Some("demo plugin".to_string()),
                        ..Default::default()
                    },
                    "/tmp/market".to_string(),
                ),
            );
            Self {
                entries,
                marketplace_plugins: vec!["market".to_string()],
                enabled_ids: HashSet::new(),
                blocked: HashSet::new(),
                resolution: DependencyResolution {
                    ok: true,
                    closure: vec!["demo@market".to_string()],
                    error_reason: None,
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl PluginInstallResolver for MockResolver {
        async fn get_plugin_by_id(&self, id: &str) -> Option<(PluginMarketplaceEntry, String)> {
            self.entries.get(id).cloned()
        }

        async fn get_marketplace_cache_only(&self, _marketplace: &str) -> Option<Vec<String>> {
            Some(self.marketplace_plugins.clone())
        }

        fn get_enabled_plugin_ids_for_scope(&self, _scope: &str) -> HashSet<String> {
            self.enabled_ids.clone()
        }

        fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool {
            self.blocked.contains(plugin_id)
        }

        fn parse_plugin_identifier(&self, id: &str) -> (String, Option<String>) {
            if let Some((name, marketplace)) = id.split_once('@') {
                (name.to_string(), Some(marketplace.to_string()))
            } else {
                (id.to_string(), None)
            }
        }

        fn scope_to_setting_source(&self, scope: &str) -> String {
            match scope {
                "project" => "projectSettings".to_string(),
                "local" => "localSettings".to_string(),
                _ => "userSettings".to_string(),
            }
        }

        async fn resolve_dependency_closure(
            &self,
            _plugin_id: &str,
            _enabled_ids: &HashSet<String>,
            _allowed_cross: &HashSet<String>,
        ) -> DependencyResolution {
            self.resolution.clone()
        }

        fn format_resolution_error(&self, resolution: &DependencyResolution) -> String {
            resolution
                .error_reason
                .clone()
                .unwrap_or_else(|| "dependency resolution failed".to_string())
        }

        fn format_dependency_count_suffix(&self, deps: &[String]) -> String {
            if deps.is_empty() {
                String::new()
            } else {
                format!(" (+{} dependencies)", deps.len())
            }
        }
    }

    #[derive(Default)]
    struct MockInstaller {
        installed: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl PluginInstaller for MockInstaller {
        async fn install_resolved_plugin(
            &self,
            plan: &PluginInstallPlan,
        ) -> Result<InstallResult, String> {
            self.installed.lock().unwrap().push(plan.plugin_id.clone());
            Ok(InstallResult {
                closure: plan.dependency_closure.clone(),
                dep_note: "installed from plan".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn plugin_install_plan_resolves_dependencies_and_confirm_is_one_shot() {
        reset_plugin_install_plan_store_for_testing();
        let resolver = MockResolver {
            resolution: DependencyResolution {
                ok: true,
                closure: vec!["helper@market".to_string(), "demo@market".to_string()],
                error_reason: None,
            },
            ..Default::default()
        };

        let plan = get_plugin_install_plan(Some("demo@market"), Some("project"), &resolver)
            .await
            .expect("create install plan");
        assert!(!plan.token.is_empty());
        assert_eq!(plan.plugin_id, "demo@market");
        assert_eq!(plan.plugin_name, "demo");
        assert_eq!(plan.marketplace_name, "market");
        assert_eq!(plan.scope, "project");
        assert_eq!(
            plan.marketplace_install_location.as_deref(),
            Some("/tmp/market")
        );
        assert_eq!(
            plan.dependency_closure,
            vec!["helper@market".to_string(), "demo@market".to_string()]
        );
        assert_eq!(plan.dep_note, " (+1 dependencies)");

        let installer = MockInstaller::default();
        let confirmed = execute_plugin_install_plan(&plan.token, &installer)
            .await
            .expect("confirm install plan");
        assert_eq!(confirmed.plugin_id, "demo@market");
        assert_eq!(confirmed.dep_note, "installed from plan");
        assert_eq!(
            installer.installed.lock().unwrap().as_slice(),
            &["demo@market".to_string()]
        );

        let second = execute_plugin_install_plan(&plan.token, &installer).await;
        assert!(matches!(
            second,
            Err(PluginInstallPlanError::UnknownToken { .. })
        ));
    }

    #[tokio::test]
    async fn plugin_install_plan_rejects_missing_scope_and_blocked_dependency() {
        reset_plugin_install_plan_store_for_testing();
        let resolver = MockResolver::default();

        let missing_plugin = get_plugin_install_plan(None, Some("user"), &resolver).await;
        assert!(matches!(
            missing_plugin,
            Err(PluginInstallPlanError::MissingPlugin)
        ));

        let invalid_scope =
            get_plugin_install_plan(Some("demo@market"), Some("team"), &resolver).await;
        assert!(matches!(
            invalid_scope,
            Err(PluginInstallPlanError::InvalidScope { .. })
        ));

        let marketplace_required =
            get_plugin_install_plan(Some("demo"), Some("user"), &resolver).await;
        assert!(matches!(
            marketplace_required,
            Err(PluginInstallPlanError::MarketplaceRequired { .. })
        ));

        let blocked_resolver = MockResolver {
            blocked: HashSet::from(["blocked@market".to_string()]),
            resolution: DependencyResolution {
                ok: true,
                closure: vec!["demo@market".to_string(), "blocked@market".to_string()],
                error_reason: None,
            },
            ..Default::default()
        };
        let blocked =
            get_plugin_install_plan(Some("demo@market"), Some("user"), &blocked_resolver).await;
        assert!(matches!(
            blocked,
            Err(PluginInstallPlanError::BlockedByPolicy { plugin_id })
                if plugin_id == "blocked@market"
        ));
    }
}
