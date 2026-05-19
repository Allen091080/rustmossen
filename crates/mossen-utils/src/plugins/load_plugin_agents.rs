//! Load plugin agents — memoized loading of agent definitions from plugins.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub agent_type: String,
    pub when_to_use: String,
    pub tools: Option<Vec<String>>,
    pub disallowed_tools: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub system_prompt: String,
    pub source: String,
    pub color: Option<String>,
    pub model: Option<String>,
    pub filename: String,
    pub plugin: String,
    pub background: Option<bool>,
    pub memory: Option<String>,
    pub isolation: Option<String>,
    pub effort: Option<String>,
    pub max_turns: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PluginForAgents {
    pub name: String,
    pub path: String,
    pub source: String,
    pub agents_path: Option<String>,
    pub agents_paths: Option<Vec<String>>,
    pub manifest_user_config: Option<HashMap<String, serde_json::Value>>,
}

static CACHE: Lazy<Mutex<Option<Vec<AgentDefinition>>>> = Lazy::new(|| Mutex::new(None));

/// Load agents from all enabled plugins (memoized).
pub async fn load_plugin_agents(
    load_all_plugins: impl std::future::Future<Output = PluginsLoadResult>,
    read_file: impl Fn(&str) -> Result<String, std::io::Error> + Send + Sync,
    parse_frontmatter: impl Fn(&str, &str) -> (HashMap<String, String>, String) + Send + Sync,
    stat_path: impl Fn(&str) -> Result<super::load_plugin_output_styles::PathMeta, std::io::Error> + Send + Sync,
    substitute_plugin_variables: impl Fn(&str, &str, &str) -> String + Send + Sync,
    load_plugin_options: impl Fn(&str) -> HashMap<String, serde_json::Value> + Send + Sync,
) -> Vec<AgentDefinition> {
    {
        let guard = CACHE.lock().unwrap();
        if let Some(ref cached) = *guard {
            return cached.clone();
        }
    }

    let result = load_all_plugins.await;
    let mut all_agents = Vec::new();

    for plugin in &result.enabled {
        let mut loaded_paths = HashSet::new();

        if let Some(ref agents_path) = plugin.agents_path {
            match load_agents_from_directory(
                agents_path, &plugin.name, &plugin.source, &plugin.path,
                &mut loaded_paths, &read_file, &parse_frontmatter, &substitute_plugin_variables,
            ).await {
                Ok(agents) => {
                    if !agents.is_empty() {
                        debug!("Loaded {} agents from plugin {} default directory", agents.len(), plugin.name);
                    }
                    all_agents.extend(agents);
                }
                Err(e) => debug!("Failed to load agents from plugin {} default directory: {}", plugin.name, e),
            }
        }

        if let Some(ref paths) = plugin.agents_paths {
            for agent_path in paths {
                match stat_path(agent_path) {
                    Ok(meta) if meta.is_dir => {
                        if let Ok(agents) = load_agents_from_directory(
                            agent_path, &plugin.name, &plugin.source, &plugin.path,
                            &mut loaded_paths, &read_file, &parse_frontmatter, &substitute_plugin_variables,
                        ).await {
                            all_agents.extend(agents);
                        }
                    }
                    Ok(meta) if meta.is_file && agent_path.ends_with(".md") => {
                        if let Some(agent) = load_agent_from_file(
                            agent_path, &plugin.name, &[], &plugin.source, &plugin.path,
                            &mut loaded_paths, &read_file, &parse_frontmatter, &substitute_plugin_variables,
                        ) {
                            all_agents.push(agent);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    debug!("Total plugin agents loaded: {}", all_agents.len());
    let mut guard = CACHE.lock().unwrap();
    *guard = Some(all_agents.clone());
    all_agents
}

async fn load_agents_from_directory(
    agents_path: &str, plugin_name: &str, source_name: &str, plugin_path: &str,
    loaded_paths: &mut HashSet<String>,
    read_file: &dyn Fn(&str) -> Result<String, std::io::Error>,
    parse_frontmatter: &dyn Fn(&str, &str) -> (HashMap<String, String>, String),
    substitute_plugin_variables: &dyn Fn(&str, &str, &str) -> String,
) -> Result<Vec<AgentDefinition>, String> {
    // Walk directory and load agents from .md files
    let mut agents = Vec::new();
    let path = PathBuf::from(agents_path);
    let mut entries = match tokio::fs::read_dir(&path).await {
        Ok(e) => e,
        Err(e) => return Err(e.to_string()),
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let fp = entry.path();
        if fp.extension().and_then(|e| e.to_str()) == Some("md") {
            let fp_str = fp.to_string_lossy().to_string();
            if let Some(agent) = load_agent_from_file(
                &fp_str, plugin_name, &[], source_name, plugin_path,
                loaded_paths, read_file, parse_frontmatter, substitute_plugin_variables,
            ) {
                agents.push(agent);
            }
        }
    }
    Ok(agents)
}

fn load_agent_from_file(
    file_path: &str, plugin_name: &str, namespace: &[String], source_name: &str,
    plugin_path: &str, loaded_paths: &mut HashSet<String>,
    read_file: &dyn Fn(&str) -> Result<String, std::io::Error>,
    parse_frontmatter: &dyn Fn(&str, &str) -> (HashMap<String, String>, String),
    substitute_plugin_variables: &dyn Fn(&str, &str, &str) -> String,
) -> Option<AgentDefinition> {
    if loaded_paths.contains(file_path) { return None; }
    loaded_paths.insert(file_path.to_string());

    let content = match read_file(file_path) {
        Ok(c) => c,
        Err(e) => { debug!("Failed to load agent from {}: {}", file_path, e); return None; }
    };

    let (frontmatter, markdown_content) = parse_frontmatter(&content, file_path);
    let base_name = frontmatter.get("name").cloned().unwrap_or_else(|| {
        Path::new(file_path).file_stem().unwrap_or_default().to_string_lossy().to_string()
    });

    let mut name_parts = vec![plugin_name.to_string()];
    name_parts.extend(namespace.iter().cloned());
    name_parts.push(base_name.clone());
    let agent_type = name_parts.join(":");

    let when_to_use = frontmatter.get("description")
        .or_else(|| frontmatter.get("when-to-use"))
        .cloned()
        .unwrap_or_else(|| format!("Agent from {} plugin", plugin_name));

    let system_prompt = substitute_plugin_variables(markdown_content.trim(), plugin_path, source_name);

    let model = frontmatter.get("model").and_then(|m| {
        let trimmed = m.trim();
        if trimmed.is_empty() { None }
        else if trimmed.eq_ignore_ascii_case("inherit") { Some("inherit".to_string()) }
        else { Some(trimmed.to_string()) }
    });

    let background = frontmatter.get("background").map(|v| v == "true");
    let memory = frontmatter.get("memory").cloned();
    let isolation = frontmatter.get("isolation").and_then(|v| if v == "worktree" { Some("worktree".to_string()) } else { None });
    let max_turns = frontmatter.get("maxTurns").and_then(|v| v.parse::<u32>().ok());

    Some(AgentDefinition {
        agent_type, when_to_use, tools: None, disallowed_tools: None, skills: None,
        system_prompt, source: "plugin".to_string(), color: frontmatter.get("color").cloned(),
        model, filename: base_name, plugin: source_name.to_string(),
        background, memory, isolation, effort: frontmatter.get("effort").cloned(), max_turns,
    })
}

pub fn clear_plugin_agent_cache() {
    let mut guard = CACHE.lock().unwrap();
    *guard = None;
}

#[derive(Debug, Clone, Default)]
pub struct PluginsLoadResult {
    pub enabled: Vec<PluginForAgents>,
    pub errors: Vec<String>,
}
