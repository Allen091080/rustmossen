use serde::{Deserialize, Serialize};

/// A path entry within an extension path group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionPathEntry {
    pub kind: ExtensionPathKind,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionPathKind {
    Skills,
    Commands,
    Agents,
    PluginsRoot,
    PluginCache,
    Marketplaces,
    Seed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionPathScope {
    User,
    Project,
    Policy,
    Plugin,
}

/// A group of extension paths sharing the same scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionPathGroup {
    pub label: String,
    pub scope: ExtensionPathScope,
    pub paths: Vec<ExtensionPathEntry>,
}

/// Summary of all extension paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionPathsSummary {
    pub config_home: String,
    pub groups: Vec<ExtensionPathGroup>,
    pub notes: Vec<String>,
}

/// Describes all extension paths for the current configuration.
pub fn describe_extension_paths(
    config_home: &str,
    get_skills_path: impl Fn(&str, &str) -> String,
    get_primary_scoped_config_dir: impl Fn() -> String,
    get_canonical_config_dir_name: impl Fn() -> String,
    get_plugins_directory: impl Fn() -> String,
    get_marketplaces_cache_dir: impl Fn() -> String,
    get_plugin_seed_dirs: impl Fn() -> Vec<String>,
) -> ExtensionPathsSummary {
    let managed = get_primary_scoped_config_dir();
    let project_config = get_canonical_config_dir_name();
    let plugin_root = get_plugins_directory();

    let mut plugin_paths = vec![
        ExtensionPathEntry {
            kind: ExtensionPathKind::PluginsRoot,
            path: plugin_root.clone(),
        },
        ExtensionPathEntry {
            kind: ExtensionPathKind::PluginCache,
            path: format!("{}/cache", plugin_root),
        },
        ExtensionPathEntry {
            kind: ExtensionPathKind::Marketplaces,
            path: get_marketplaces_cache_dir(),
        },
    ];
    for seed_path in get_plugin_seed_dirs() {
        plugin_paths.push(ExtensionPathEntry {
            kind: ExtensionPathKind::Seed,
            path: seed_path,
        });
    }

    ExtensionPathsSummary {
        config_home: config_home.to_string(),
        groups: vec![
            ExtensionPathGroup {
                label: "User extensions".to_string(),
                scope: ExtensionPathScope::User,
                paths: vec![
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Skills,
                        path: get_skills_path("userSettings", "skills"),
                    },
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Commands,
                        path: get_skills_path("userSettings", "commands"),
                    },
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Agents,
                        path: format!("{}/agents", config_home),
                    },
                ],
            },
            ExtensionPathGroup {
                label: "Project extensions".to_string(),
                scope: ExtensionPathScope::Project,
                paths: vec![
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Skills,
                        path: get_skills_path("projectSettings", "skills"),
                    },
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Commands,
                        path: get_skills_path("projectSettings", "commands"),
                    },
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Agents,
                        path: format!("{}/agents", project_config),
                    },
                ],
            },
            ExtensionPathGroup {
                label: "Policy extensions".to_string(),
                scope: ExtensionPathScope::Policy,
                paths: vec![
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Skills,
                        path: get_skills_path("policySettings", "skills"),
                    },
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Commands,
                        path: get_skills_path("policySettings", "commands"),
                    },
                    ExtensionPathEntry {
                        kind: ExtensionPathKind::Agents,
                        path: format!("{}/agents", managed),
                    },
                ],
            },
            ExtensionPathGroup {
                label: "Plugin extension system".to_string(),
                scope: ExtensionPathScope::Plugin,
                paths: plugin_paths,
            },
        ],
        notes: vec![
            "Project paths are relative to the current working directory.".to_string(),
            "Plugin components are loaded through plugin manifests and marketplace entries."
                .to_string(),
            "This summary is read-only and does not create directories.".to_string(),
        ],
    }
}
