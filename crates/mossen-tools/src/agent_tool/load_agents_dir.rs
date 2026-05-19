//! # load_agents_dir — Agent definition loading and resolution
//!
//! Translates `tools/AgentTool/loadAgentsDir.ts`.
//! Loads agent definitions from markdown files, JSON configs, built-in agents,
//! and plugin agents. Provides the AgentDefinition type and resolution logic.

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::color_manager::AgentColorName;
use super::memory::AgentMemoryScope;
use super::utils::PermissionMode;

/// MCP server specification in agent definitions.
/// Can be either a reference to an existing server by name, or an inline definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentMcpServerSpec {
    /// Reference to existing server by name (e.g., "slack")
    Reference(String),
    /// Inline definition as { name: config }
    Inline(HashMap<String, serde_json::Value>),
}

/// Hook settings for an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksSettings {
    #[serde(default)]
    pub pre_tool_use: Option<Vec<String>>,
    #[serde(default)]
    pub post_tool_use: Option<Vec<String>>,
}

/// Effort level for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EffortValue {
    Level(String),
    Numeric(i32),
}

/// Agent definition — the complete specification for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub agent_type: String,
    pub when_to_use: String,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub disallowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub mcp_servers: Option<Vec<AgentMcpServerSpec>>,
    #[serde(default)]
    pub hooks: Option<HooksSettings>,
    #[serde(default)]
    pub color: Option<AgentColorName>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<EffortValue>,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    #[serde(default)]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub background: Option<bool>,
    #[serde(default)]
    pub isolation: Option<String>,
    #[serde(default)]
    pub memory: Option<AgentMemoryScope>,
    #[serde(default)]
    pub initial_prompt: Option<String>,
    #[serde(default)]
    pub use_exact_tools: Option<bool>,
    /// Dynamic system prompt generator (closure not serializable, so we store the result)
    #[serde(skip)]
    pub system_prompt: Option<String>,
}

/// Built-in agent definition with a system prompt generator function.
pub struct BuiltInAgentDefinition {
    pub agent_type: &'static str,
    pub when_to_use: &'static str,
    pub tools: Option<&'static [&'static str]>,
    pub disallowed_tools: Option<&'static [&'static str]>,
    pub color: Option<AgentColorName>,
    pub model: &'static str,
    pub permission_mode: PermissionMode,
    pub source: &'static str,
    pub base_dir: &'static str,
    pub background: Option<bool>,
    pub max_turns: Option<u32>,
    pub get_system_prompt: fn() -> String,
}

impl BuiltInAgentDefinition {
    /// Convert to a standard AgentDefinition.
    pub fn to_agent_definition(&self) -> AgentDefinition {
        AgentDefinition {
            agent_type: self.agent_type.to_string(),
            when_to_use: self.when_to_use.to_string(),
            tools: self.tools.map(|t| t.iter().map(|s| s.to_string()).collect()),
            disallowed_tools: self
                .disallowed_tools
                .map(|t| t.iter().map(|s| s.to_string()).collect()),
            skills: None,
            mcp_servers: None,
            hooks: None,
            color: self.color.clone(),
            model: Some(self.model.to_string()),
            effort: None,
            permission_mode: Some(self.permission_mode.clone()),
            max_turns: self.max_turns,
            filename: None,
            base_dir: Some(self.base_dir.to_string()),
            source: self.source.to_string(),
            background: self.background,
            isolation: None,
            memory: None,
            initial_prompt: None,
            use_exact_tools: None,
            system_prompt: Some((self.get_system_prompt)()),
        }
    }
}

/// Check if an agent is a built-in agent.
pub fn is_built_in_agent(agent: &AgentDefinition) -> bool {
    agent.source == "built-in"
}

/// Collection of resolved agents.
#[derive(Debug, Clone, Default)]
pub struct AgentDefinitions {
    pub active_agents: Vec<AgentDefinition>,
    pub all_agents: Vec<AgentDefinition>,
}

/// Load agents from a directory of markdown files.
pub async fn load_agents_from_dir(
    dir: &Path,
    source: &str,
) -> Vec<AgentDefinition> {
    let mut agents = Vec::new();

    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return agents,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str());
        match ext {
            Some("md") => {
                if let Some(agent) = parse_markdown_agent(&path, source).await {
                    agents.push(agent);
                }
            }
            Some("json") => {
                if let Some(mut loaded) = parse_json_agents(&path, source).await {
                    agents.append(&mut loaded);
                }
            }
            _ => continue,
        }
    }

    agents
}

/// Parse a markdown file into an agent definition.
async fn parse_markdown_agent(path: &Path, source: &str) -> Option<AgentDefinition> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    let filename = path.file_stem()?.to_string_lossy().to_string();

    // Parse frontmatter (YAML between --- delimiters)
    let (frontmatter, body) = parse_frontmatter(&content)?;

    let agent_type = filename.clone();
    let when_to_use = extract_field(&frontmatter, "description")
        .unwrap_or_else(|| format!("Agent: {}", agent_type));

    let tools = extract_string_array(&frontmatter, "tools");
    let disallowed_tools = extract_string_array(&frontmatter, "disallowedTools");
    let skills = extract_string_array(&frontmatter, "skills");
    let model = extract_field(&frontmatter, "model");
    let max_turns = extract_field(&frontmatter, "maxTurns")
        .and_then(|s| s.parse::<u32>().ok());
    let permission_mode = extract_field(&frontmatter, "permissionMode")
        .and_then(|s| parse_permission_mode(&s));
    let memory = extract_field(&frontmatter, "memory")
        .and_then(|s| parse_memory_scope(&s));
    let background = extract_field(&frontmatter, "background")
        .and_then(|s| s.parse::<bool>().ok());
    let isolation = extract_field(&frontmatter, "isolation");

    Some(AgentDefinition {
        agent_type,
        when_to_use,
        tools,
        disallowed_tools,
        skills,
        mcp_servers: None,
        hooks: None,
        color: None,
        model,
        effort: None,
        permission_mode,
        max_turns,
        filename: Some(filename),
        base_dir: path.parent().map(|p| p.to_string_lossy().to_string()),
        source: source.to_string(),
        background,
        isolation,
        memory,
        initial_prompt: None,
        use_exact_tools: None,
        system_prompt: Some(body),
    })
}

/// Parse a JSON file containing agent definitions.
async fn parse_json_agents(path: &Path, source: &str) -> Option<Vec<AgentDefinition>> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    let parsed: HashMap<String, serde_json::Value> = serde_json::from_str(&content).ok()?;

    let mut agents = Vec::new();
    for (name, config) in parsed {
        let description = config.get("description")?.as_str()?.to_string();
        let prompt = config.get("prompt").and_then(|p| p.as_str()).unwrap_or("").to_string();
        let tools = config
            .get("tools")
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
        let disallowed_tools = config
            .get("disallowedTools")
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
        let model = config.get("model").and_then(|m| m.as_str()).map(|s| {
            if s.eq_ignore_ascii_case("inherit") {
                "inherit".to_string()
            } else {
                s.to_string()
            }
        });
        let max_turns = config
            .get("maxTurns")
            .and_then(|n| n.as_u64())
            .map(|n| n as u32);
        let permission_mode = config
            .get("permissionMode")
            .and_then(|p| p.as_str())
            .and_then(|s| parse_permission_mode(s));
        let memory = config
            .get("memory")
            .and_then(|m| m.as_str())
            .and_then(|s| parse_memory_scope(s));
        let background = config.get("background").and_then(|b| b.as_bool());

        agents.push(AgentDefinition {
            agent_type: name,
            when_to_use: description,
            tools,
            disallowed_tools,
            skills: None,
            mcp_servers: None,
            hooks: None,
            color: None,
            model,
            effort: None,
            permission_mode,
            max_turns,
            filename: path.file_stem().map(|s| s.to_string_lossy().to_string()),
            base_dir: path.parent().map(|p| p.to_string_lossy().to_string()),
            source: source.to_string(),
            background,
            isolation: None,
            memory,
            initial_prompt: None,
            use_exact_tools: None,
            system_prompt: Some(prompt),
        });
    }

    Some(agents)
}

/// Parse frontmatter from a markdown document.
/// Returns (frontmatter_lines, body) if valid frontmatter exists.
fn parse_frontmatter(content: &str) -> Option<(Vec<String>, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        // No frontmatter, treat entire content as body
        return Some((Vec::new(), content.to_string()));
    }

    let end_idx = lines[1..].iter().position(|l| l.trim() == "---")?;
    let frontmatter: Vec<String> = lines[1..=end_idx].iter().map(|s| s.to_string()).collect();
    let body = lines[end_idx + 2..].join("\n");
    Some((frontmatter, body))
}

/// Extract a field value from frontmatter lines.
fn extract_field(frontmatter: &[String], key: &str) -> Option<String> {
    let prefix = format!("{}: ", key);
    for line in frontmatter {
        let trimmed = line.trim();
        if trimmed.starts_with(&prefix) {
            let value = trimmed[prefix.len()..].trim().to_string();
            // Remove surrounding quotes if present
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                return Some(value[1..value.len() - 1].to_string());
            }
            return Some(value);
        }
    }
    None
}

/// Extract a string array from frontmatter (YAML list format).
fn extract_string_array(frontmatter: &[String], key: &str) -> Option<Vec<String>> {
    let header = format!("{}:", key);
    let mut found = false;
    let mut values = Vec::new();

    for line in frontmatter {
        let trimmed = line.trim();
        if trimmed == header || trimmed.starts_with(&format!("{}: ", key)) {
            // Check if it's inline: "tools: [a, b, c]"
            if let Some(rest) = trimmed.strip_prefix(&format!("{}: ", key)) {
                let rest = rest.trim();
                if rest.starts_with('[') && rest.ends_with(']') {
                    let inner = &rest[1..rest.len() - 1];
                    return Some(
                        inner
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                            .filter(|s| !s.is_empty())
                            .collect(),
                    );
                }
            }
            found = true;
            continue;
        }
        if found {
            if trimmed.starts_with("- ") {
                values.push(trimmed[2..].trim().trim_matches('"').trim_matches('\'').to_string());
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                break;
            }
        }
    }

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

/// Parse a permission mode string.
fn parse_permission_mode(s: &str) -> Option<PermissionMode> {
    match s {
        "acceptEdits" => Some(PermissionMode::AcceptEdits),
        "dontAsk" => Some(PermissionMode::DontAsk),
        "plan" => Some(PermissionMode::Plan),
        "bubble" => Some(PermissionMode::Bubble),
        _ => None,
    }
}

/// Parse a memory scope string.
fn parse_memory_scope(s: &str) -> Option<AgentMemoryScope> {
    match s {
        "user" => Some(AgentMemoryScope::User),
        "project" => Some(AgentMemoryScope::Project),
        "local" => Some(AgentMemoryScope::Local),
        _ => None,
    }
}

/// Load all agent definitions from multiple sources (user, project, built-in, etc.).
pub async fn load_all_agents(
    user_agents_dir: Option<&Path>,
    project_agents_dir: Option<&Path>,
    local_agents_dir: Option<&Path>,
) -> AgentDefinitions {
    let mut all_agents = Vec::new();

    // Load user agents
    if let Some(dir) = user_agents_dir {
        let agents = load_agents_from_dir(dir, "userSettings").await;
        all_agents.extend(agents);
    }

    // Load project agents
    if let Some(dir) = project_agents_dir {
        let agents = load_agents_from_dir(dir, "projectSettings").await;
        all_agents.extend(agents);
    }

    // Load local agents
    if let Some(dir) = local_agents_dir {
        let agents = load_agents_from_dir(dir, "localSettings").await;
        all_agents.extend(agents);
    }

    // Load built-in agents
    let built_in = super::built_in_agents::get_built_in_agents();
    all_agents.extend(built_in.clone());

    // Resolve active agents (highest priority source wins per agent_type)
    let active_agents = resolve_active_agents(&all_agents);

    AgentDefinitions {
        active_agents,
        all_agents,
    }
}

/// Resolve active agents from all loaded agents.
/// The first agent for each type wins (sources are loaded in priority order).
fn resolve_active_agents(all_agents: &[AgentDefinition]) -> Vec<AgentDefinition> {
    let mut seen = std::collections::HashSet::new();
    let mut active = Vec::new();

    for agent in all_agents {
        if seen.contains(&agent.agent_type) {
            continue;
        }
        seen.insert(agent.agent_type.clone());
        active.push(agent.clone());
    }

    active
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/AgentTool/loadAgentsDir.ts` exports.
// ---------------------------------------------------------------------------

use std::sync::Mutex;
use once_cell::sync::Lazy;

/// `loadAgentsDir.ts` `isCustomAgent` — not built-in and not plugin.
pub fn is_custom_agent(agent: &AgentDefinition) -> bool {
    agent.source != "built-in" && agent.source != "plugin"
}

/// `loadAgentsDir.ts` `isPluginAgent`.
pub fn is_plugin_agent(agent: &AgentDefinition) -> bool {
    agent.source == "plugin"
}

/// `loadAgentsDir.ts` `AgentDefinitionsResult`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentDefinitionsResult {
    pub active_agents: Vec<AgentDefinition>,
    pub all_agents: Vec<AgentDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_files: Vec<FailedFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agent_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedFile {
    pub path: String,
    pub error: String,
}

/// `loadAgentsDir.ts` `getActiveAgentsFromList` — priority-ordered source
/// resolution. First source wins per `agent_type`.
pub fn get_active_agents_from_list(all_agents: &[AgentDefinition]) -> Vec<AgentDefinition> {
    let groups = [
        "built-in",
        "plugin",
        "userSettings",
        "projectSettings",
        "flagSettings",
        "policySettings",
    ];
    let mut agent_map: HashMap<String, AgentDefinition> = HashMap::new();
    for source in groups {
        for a in all_agents.iter().filter(|a| a.source == source) {
            agent_map.insert(a.agent_type.clone(), a.clone());
        }
    }
    agent_map.into_values().collect()
}

/// Extension trait helper — agents may declare required MCP servers in
/// the markdown frontmatter. We piggy-back on `mcp_servers` for the check
/// when `required_mcp_servers` isn't separately surfaced.
fn agent_required_mcp_servers(agent: &AgentDefinition) -> Vec<String> {
    let Some(specs) = &agent.mcp_servers else {
        return Vec::new();
    };
    specs
        .iter()
        .filter_map(|spec| match spec {
            AgentMcpServerSpec::Reference(name) => Some(name.clone()),
            AgentMcpServerSpec::Inline(_) => None,
        })
        .collect()
}

/// `loadAgentsDir.ts` `hasRequiredMcpServers`.
pub fn has_required_mcp_servers(
    agent: &AgentDefinition,
    available_servers: &[String],
) -> bool {
    let required = agent_required_mcp_servers(agent);
    if required.is_empty() {
        return true;
    }
    required.iter().all(|pattern| {
        let lp = pattern.to_lowercase();
        available_servers
            .iter()
            .any(|s| s.to_lowercase().contains(&lp))
    })
}

/// `loadAgentsDir.ts` `filterAgentsByMcpRequirements`.
pub fn filter_agents_by_mcp_requirements(
    agents: &[AgentDefinition],
    available_servers: &[String],
) -> Vec<AgentDefinition> {
    agents
        .iter()
        .filter(|a| has_required_mcp_servers(a, available_servers))
        .cloned()
        .collect()
}

static DEFINITIONS_CACHE: Lazy<Mutex<Option<AgentDefinitionsResult>>> =
    Lazy::new(|| Mutex::new(None));

/// `loadAgentsDir.ts` `getAgentDefinitionsWithOverrides` — memoized resolver.
/// In the Rust port we cache the last computed `AgentDefinitionsResult`.
pub fn get_agent_definitions_with_overrides() -> Option<AgentDefinitionsResult> {
    DEFINITIONS_CACHE.lock().unwrap().clone()
}

/// Set the cached definitions (used by load_all_agents-style helpers).
pub fn set_agent_definitions_cache(value: AgentDefinitionsResult) {
    *DEFINITIONS_CACHE.lock().unwrap() = Some(value);
}

/// `loadAgentsDir.ts` `clearAgentDefinitionsCache`.
pub fn clear_agent_definitions_cache() {
    *DEFINITIONS_CACHE.lock().unwrap() = None;
}

/// `loadAgentsDir.ts` `parseAgentFromJson` — build an AgentDefinition from a
/// JSON descriptor.
pub fn parse_agent_from_json(
    name: &str,
    def: &serde_json::Value,
    source: &str,
) -> Option<AgentDefinition> {
    let mut agent = AgentDefinition {
        agent_type: name.to_string(),
        when_to_use: def
            .get("description")
            .or_else(|| def.get("whenToUse"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tools: def
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        disallowed_tools: def
            .get("disallowedTools")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        skills: def
            .get("skills")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        mcp_servers: None,
        hooks: None,
        color: None,
        model: def.get("model").and_then(|v| v.as_str()).map(String::from),
        effort: None,
        permission_mode: None,
        max_turns: def
            .get("maxTurns")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        filename: None,
        base_dir: None,
        source: source.to_string(),
        background: def.get("background").and_then(|v| v.as_bool()),
        isolation: def.get("isolation").and_then(|v| v.as_str()).map(String::from),
        memory: None,
        initial_prompt: def
            .get("initialPrompt")
            .and_then(|v| v.as_str())
            .map(String::from),
        use_exact_tools: def.get("useExactTools").and_then(|v| v.as_bool()),
        system_prompt: def
            .get("systemPrompt")
            .or_else(|| def.get("prompt"))
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    if let Some(servers) = def.get("mcpServers").and_then(|v| v.as_array()) {
        let mut specs = Vec::new();
        for s in servers {
            if let Some(name) = s.as_str() {
                specs.push(AgentMcpServerSpec::Reference(name.to_string()));
            } else if let Some(obj) = s.as_object() {
                let mut map = HashMap::new();
                for (k, v) in obj {
                    map.insert(k.clone(), v.clone());
                }
                specs.push(AgentMcpServerSpec::Inline(map));
            }
        }
        if !specs.is_empty() {
            agent.mcp_servers = Some(specs);
        }
    }
    Some(agent)
}

/// `loadAgentsDir.ts` `parseAgentsFromJson`.
pub fn parse_agents_from_json(
    obj: &serde_json::Value,
    source: &str,
) -> Vec<AgentDefinition> {
    let Some(map) = obj.as_object() else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(name, def)| parse_agent_from_json(name, def, source))
        .collect()
}

/// `loadAgentsDir.ts` `BaseAgentDefinition` — shared fields across all
/// agent variants. Mirrors the TS structural type as a typed view.
pub type BaseAgentDefinition = AgentDefinition;

/// `loadAgentsDir.ts` `CustomAgentDefinition` — user-defined agent.
pub type CustomAgentDefinition = AgentDefinition;

/// `loadAgentsDir.ts` `PluginAgentDefinition` — plugin-provided agent.
pub type PluginAgentDefinition = AgentDefinition;

/// `loadAgentsDir.ts` `parseAgentFromMarkdown` — parses a YAML-frontmatter
/// markdown file into an AgentDefinition.
pub fn parse_agent_from_markdown(
    filename: &str,
    raw: &str,
    source: &str,
    base_dir: &str,
) -> Option<AgentDefinition> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after = &trimmed[3..];
    let close_idx = after.find("\n---")?;
    let frontmatter = &after[..close_idx];
    let body = after[close_idx + 4..].trim_start_matches('\n').to_string();
    let yaml: serde_yaml::Value = serde_yaml::from_str(frontmatter).ok()?;
    let map = yaml.as_mapping()?;
    let agent_type = map
        .get(&serde_yaml::Value::String("name".to_string()))
        .or_else(|| map.get(&serde_yaml::Value::String("agentType".to_string())))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            Path::new(filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("agent")
        })
        .to_string();
    let when_to_use = map
        .get(&serde_yaml::Value::String("description".to_string()))
        .or_else(|| map.get(&serde_yaml::Value::String("whenToUse".to_string())))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut def = AgentDefinition {
        agent_type,
        when_to_use,
        tools: map
            .get(&serde_yaml::Value::String("tools".to_string()))
            .and_then(|v| v.as_sequence())
            .map(|s| s.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        disallowed_tools: map
            .get(&serde_yaml::Value::String("disallowedTools".to_string()))
            .and_then(|v| v.as_sequence())
            .map(|s| s.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        skills: map
            .get(&serde_yaml::Value::String("skills".to_string()))
            .and_then(|v| v.as_sequence())
            .map(|s| s.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        mcp_servers: None,
        hooks: None,
        color: None,
        model: map
            .get(&serde_yaml::Value::String("model".to_string()))
            .and_then(|v| v.as_str())
            .map(String::from),
        effort: None,
        permission_mode: None,
        max_turns: map
            .get(&serde_yaml::Value::String("maxTurns".to_string()))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        filename: Some(filename.to_string()),
        base_dir: Some(base_dir.to_string()),
        source: source.to_string(),
        background: map
            .get(&serde_yaml::Value::String("background".to_string()))
            .and_then(|v| v.as_bool()),
        isolation: map
            .get(&serde_yaml::Value::String("isolation".to_string()))
            .and_then(|v| v.as_str())
            .map(String::from),
        memory: None,
        initial_prompt: None,
        use_exact_tools: map
            .get(&serde_yaml::Value::String("useExactTools".to_string()))
            .and_then(|v| v.as_bool()),
        system_prompt: Some(body),
    };
    if let Some(servers) = map
        .get(&serde_yaml::Value::String("mcpServers".to_string()))
        .and_then(|v| v.as_sequence())
    {
        let mut specs = Vec::new();
        for s in servers {
            if let Some(name) = s.as_str() {
                specs.push(AgentMcpServerSpec::Reference(name.to_string()));
            }
        }
        if !specs.is_empty() {
            def.mcp_servers = Some(specs);
        }
    }
    Some(def)
}
