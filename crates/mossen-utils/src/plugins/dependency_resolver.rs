//! Plugin dependency resolution — pure functions, no I/O.
//!
//! Translated from `utils/plugins/dependencyResolver.ts` (305 lines).

use std::collections::HashSet;

use super::plugin_identifier::{parse_plugin_identifier, EditableSettingSource};
use super::schemas::PluginId;

/// Synthetic marketplace sentinel for `--plugin-dir` plugins.
const INLINE_MARKETPLACE: &str = "inline";

/// Normalize a dependency reference to fully-qualified "name@marketplace" form.
pub fn qualify_dependency(dep: &str, declaring_plugin_id: &str) -> String {
    let parsed = parse_plugin_identifier(dep);
    if parsed.marketplace.is_some() {
        return dep.to_string();
    }
    let declaring = parse_plugin_identifier(declaring_plugin_id);
    match declaring.marketplace {
        Some(ref mkt) if mkt != INLINE_MARKETPLACE => format!("{}@{}", dep, mkt),
        _ => dep.to_string(),
    }
}

/// Minimal shape the resolver needs from a marketplace lookup.
pub struct DependencyLookupResult {
    pub dependencies: Vec<String>,
}

/// Resolution result.
#[derive(Debug, Clone)]
pub enum ResolutionResult {
    Ok {
        closure: Vec<PluginId>,
    },
    Cycle {
        chain: Vec<PluginId>,
    },
    NotFound {
        missing: PluginId,
        required_by: PluginId,
    },
    CrossMarketplace {
        dependency: PluginId,
        required_by: PluginId,
    },
}

/// Walk the transitive dependency closure of `root_id` via DFS.
pub async fn resolve_dependency_closure<F, Fut>(
    root_id: &str,
    lookup: F,
    already_enabled: &HashSet<String>,
    allowed_cross_marketplaces: &HashSet<String>,
) -> ResolutionResult
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Option<DependencyLookupResult>>,
{
    let root_marketplace = parse_plugin_identifier(root_id).marketplace;
    let mut closure: Vec<PluginId> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();

    async fn walk<F2, Fut2>(
        id: &str,
        required_by: &str,
        root_id: &str,
        root_marketplace: &Option<String>,
        lookup: &F2,
        already_enabled: &HashSet<String>,
        allowed_cross_marketplaces: &HashSet<String>,
        closure: &mut Vec<PluginId>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<ResolutionResult>
    where
        F2: Fn(String) -> Fut2,
        Fut2: std::future::Future<Output = Option<DependencyLookupResult>>,
    {
        // Skip already-enabled DEPENDENCIES but never skip the root
        if id != root_id && already_enabled.contains(id) {
            return None;
        }

        // Security: block cross-marketplace
        let id_marketplace = parse_plugin_identifier(id).marketplace;
        if id_marketplace.as_ref() != root_marketplace.as_ref() {
            let allowed = id_marketplace
                .as_ref()
                .map(|m| allowed_cross_marketplaces.contains(m.as_str()))
                .unwrap_or(false);
            if !allowed {
                return Some(ResolutionResult::CrossMarketplace {
                    dependency: id.to_string(),
                    required_by: required_by.to_string(),
                });
            }
        }

        if stack.contains(&id.to_string()) {
            let mut chain = stack.clone();
            chain.push(id.to_string());
            return Some(ResolutionResult::Cycle { chain });
        }

        if visited.contains(id) {
            return None;
        }
        visited.insert(id.to_string());

        let entry = lookup(id.to_string()).await;
        if entry.is_none() {
            return Some(ResolutionResult::NotFound {
                missing: id.to_string(),
                required_by: required_by.to_string(),
            });
        }
        let entry = entry.unwrap();

        stack.push(id.to_string());
        for raw_dep in &entry.dependencies {
            let dep = qualify_dependency(raw_dep, id);
            let err = Box::pin(walk(
                &dep,
                id,
                root_id,
                root_marketplace,
                lookup,
                already_enabled,
                allowed_cross_marketplaces,
                closure,
                visited,
                stack,
            ))
            .await;
            if let Some(e) = err {
                return Some(e);
            }
        }
        stack.pop();

        closure.push(id.to_string());
        None
    }

    let err = walk(
        root_id,
        root_id,
        root_id,
        &root_marketplace,
        &lookup,
        already_enabled,
        allowed_cross_marketplaces,
        &mut closure,
        &mut visited,
        &mut stack,
    )
    .await;

    match err {
        Some(e) => e,
        None => ResolutionResult::Ok { closure },
    }
}

/// Plugin error for dependency verification.
#[derive(Debug, Clone)]
pub struct DependencyError {
    pub error_type: String,
    pub source: String,
    pub plugin: String,
    pub dependency: String,
    pub reason: String,
}

/// Loaded plugin shape for dependency verification.
pub struct LoadedPluginRef<'a> {
    pub source: &'a str,
    pub name: &'a str,
    pub enabled: bool,
    pub dependencies: &'a [String],
}

/// Load-time safety net: verify all manifest dependencies are in the enabled set.
pub fn verify_and_demote(
    plugins: &[LoadedPluginRef<'_>],
) -> (HashSet<String>, Vec<DependencyError>) {
    let known: HashSet<&str> = plugins.iter().map(|p| p.source).collect();
    let mut enabled: HashSet<&str> = plugins
        .iter()
        .filter(|p| p.enabled)
        .map(|p| p.source)
        .collect();

    let known_by_name: HashSet<String> = plugins
        .iter()
        .map(|p| parse_plugin_identifier(p.source).name)
        .collect();

    let mut enabled_by_name: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for id in enabled.iter() {
        let n = parse_plugin_identifier(id).name;
        *enabled_by_name.entry(n).or_default() += 1;
    }

    let mut errors = Vec::new();
    let mut changed = true;

    while changed {
        changed = false;
        for p in plugins {
            if !enabled.contains(p.source) {
                continue;
            }
            for raw_dep in p.dependencies {
                let dep = qualify_dependency(raw_dep, p.source);
                let is_bare = parse_plugin_identifier(&dep).marketplace.is_none();
                let satisfied = if is_bare {
                    enabled_by_name.get(&dep).copied().unwrap_or(0) > 0
                } else {
                    enabled.contains(dep.as_str())
                };
                if !satisfied {
                    enabled.remove(p.source);
                    let count = enabled_by_name.get(p.name).copied().unwrap_or(0);
                    if count <= 1 {
                        enabled_by_name.remove(p.name);
                    } else {
                        enabled_by_name.insert(p.name.to_string(), count - 1);
                    }
                    let reason = if is_bare {
                        if known_by_name.contains(&dep) {
                            "not-enabled"
                        } else {
                            "not-found"
                        }
                    } else {
                        if known.contains(dep.as_str()) {
                            "not-enabled"
                        } else {
                            "not-found"
                        }
                    };
                    errors.push(DependencyError {
                        error_type: "dependency-unsatisfied".to_string(),
                        source: p.source.to_string(),
                        plugin: p.name.to_string(),
                        dependency: dep,
                        reason: reason.to_string(),
                    });
                    changed = true;
                    break;
                }
            }
        }
    }

    let demoted: HashSet<String> = plugins
        .iter()
        .filter(|p| p.enabled && !enabled.contains(p.source))
        .map(|p| p.source.to_string())
        .collect();

    (demoted, errors)
}

/// Find all enabled plugins that declare `plugin_id` as a dependency.
pub fn find_reverse_dependents(plugin_id: &str, plugins: &[LoadedPluginRef<'_>]) -> Vec<String> {
    let target_name = parse_plugin_identifier(plugin_id).name;
    plugins
        .iter()
        .filter(|p| {
            p.enabled
                && p.source != plugin_id
                && p.dependencies.iter().any(|d| {
                    let qualified = qualify_dependency(d, p.source);
                    let parsed = parse_plugin_identifier(&qualified);
                    if parsed.marketplace.is_some() {
                        qualified == plugin_id
                    } else {
                        qualified == target_name
                    }
                })
        })
        .map(|p| p.name.to_string())
        .collect()
}

/// Build the set of plugin IDs currently enabled at a given settings scope.
pub fn get_enabled_plugin_ids_for_scope(
    _setting_source: EditableSettingSource,
    enabled_plugins: &std::collections::HashMap<String, serde_json::Value>,
) -> HashSet<PluginId> {
    enabled_plugins
        .iter()
        .filter(|(_, v)| v.as_bool() == Some(true) || v.is_array())
        .map(|(k, _)| k.clone())
        .collect()
}

/// Format the "(+ N dependencies)" suffix for install success messages.
pub fn format_dependency_count_suffix(installed_deps: &[String]) -> String {
    if installed_deps.is_empty() {
        return String::new();
    }
    let n = installed_deps.len();
    let word = if n == 1 { "dependency" } else { "dependencies" };
    format!(" (+ {} {})", n, word)
}

/// Format the "warning: required by X, Y" suffix for uninstall/disable results.
pub fn format_reverse_dependents_suffix(rdeps: &[String]) -> String {
    if rdeps.is_empty() {
        return String::new();
    }
    format!(" — warning: required by {}", rdeps.join(", "))
}
