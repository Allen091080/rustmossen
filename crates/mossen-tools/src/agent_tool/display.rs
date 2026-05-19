//! # display — Agent display utilities
//!
//! Translates `tools/AgentTool/agentDisplay.ts`.
//! Utilities for displaying agent information, resolving overrides,
//! model display strings, and sorting.

use std::collections::{HashMap, HashSet};

/// Agent source type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    PolicySettings,
    Plugin,
    FlagSettings,
    BuiltIn,
}

impl AgentSource {
    /// Human-readable lowercase label for the source.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::UserSettings => "user",
            Self::ProjectSettings => "project",
            Self::LocalSettings => "local",
            Self::PolicySettings => "managed",
            Self::Plugin => "plugin",
            Self::FlagSettings => "cli arg",
            Self::BuiltIn => "built-in",
        }
    }
}

/// Agent source group for display ordering.
#[derive(Debug, Clone)]
pub struct AgentSourceGroup {
    pub label: &'static str,
    pub source: AgentSource,
}

/// Ordered list of agent source groups for display.
/// Both the CLI and interactive UI should use this to ensure consistent ordering.
pub const AGENT_SOURCE_GROUPS: &[(&str, &str)] = &[
    ("User agents", "userSettings"),
    ("Project agents", "projectSettings"),
    ("Local agents", "localSettings"),
    ("Managed agents", "policySettings"),
    ("Plugin agents", "plugin"),
    ("CLI arg agents", "flagSettings"),
    ("Built-in agents", "built-in"),
];

/// Returns the ordered agent source groups.
pub fn get_agent_source_groups() -> Vec<AgentSourceGroup> {
    vec![
        AgentSourceGroup { label: "User agents", source: AgentSource::UserSettings },
        AgentSourceGroup { label: "Project agents", source: AgentSource::ProjectSettings },
        AgentSourceGroup { label: "Local agents", source: AgentSource::LocalSettings },
        AgentSourceGroup { label: "Managed agents", source: AgentSource::PolicySettings },
        AgentSourceGroup { label: "Plugin agents", source: AgentSource::Plugin },
        AgentSourceGroup { label: "CLI arg agents", source: AgentSource::FlagSettings },
        AgentSourceGroup { label: "Built-in agents", source: AgentSource::BuiltIn },
    ]
}

/// Minimal agent definition for display purposes.
#[derive(Debug, Clone)]
pub struct AgentDefinitionRef {
    pub agent_type: String,
    pub source: AgentSource,
    pub model: Option<String>,
    pub when_to_use: String,
}

/// Resolved agent with optional override information.
#[derive(Debug, Clone)]
pub struct ResolvedAgent {
    pub agent_type: String,
    pub source: AgentSource,
    pub model: Option<String>,
    pub when_to_use: String,
    pub overridden_by: Option<AgentSource>,
}

/// Annotate agents with override information by comparing against the active
/// (winning) agent list. An agent is "overridden" when another agent with the
/// same type from a higher-priority source takes precedence.
///
/// Also deduplicates by (agentType, source) to handle git worktree duplicates
/// where the same agent file is loaded from both the worktree and main repo.
pub fn resolve_agent_overrides(
    all_agents: &[AgentDefinitionRef],
    active_agents: &[AgentDefinitionRef],
) -> Vec<ResolvedAgent> {
    let mut active_map: HashMap<&str, &AgentDefinitionRef> = HashMap::new();
    for agent in active_agents {
        active_map.insert(&agent.agent_type, agent);
    }

    let mut seen = HashSet::new();
    let mut resolved = Vec::new();

    for agent in all_agents {
        let key = format!("{}:{:?}", agent.agent_type, agent.source);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        let overridden_by = active_map
            .get(agent.agent_type.as_str())
            .and_then(|active| {
                if active.source != agent.source {
                    Some(active.source.clone())
                } else {
                    None
                }
            });

        resolved.push(ResolvedAgent {
            agent_type: agent.agent_type.clone(),
            source: agent.source.clone(),
            model: agent.model.clone(),
            when_to_use: agent.when_to_use.clone(),
            overridden_by,
        });
    }

    resolved
}

/// Resolve the display model string for an agent.
/// Returns the model alias or "inherit" for display purposes.
pub fn resolve_agent_model_display(model: Option<&str>, default_model: Option<&str>) -> Option<String> {
    let effective = model.or(default_model)?;
    if effective.eq_ignore_ascii_case("inherit") {
        Some("inherit".to_string())
    } else {
        Some(effective.to_string())
    }
}

/// Get a human-readable label for the source that overrides an agent.
/// Returns lowercase, e.g. "user", "project", "managed".
pub fn get_override_source_label(source: &AgentSource) -> &'static str {
    source.display_name()
}

/// Compare agents alphabetically by name (case-insensitive).
pub fn compare_agents_by_name(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}
