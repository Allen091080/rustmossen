//! Unified Suggestions hook (useUnifiedSuggestions.ts).
//! Unifies file and command suggestions into one list.

#[derive(Debug, Clone)]
pub struct UnifiedSuggestionsState {
    pub active: bool,
    pub initialized: bool,
}

impl UnifiedSuggestionsState {
    pub fn new() -> Self { Self { active: false, initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
    pub fn activate(&mut self) { self.active = true; }
    pub fn deactivate(&mut self) { self.active = false; }
    pub fn is_active(&self) -> bool { self.active }
}
impl Default for UnifiedSuggestionsState { fn default() -> Self { Self::new() } }

// ============================================================================
// generateUnifiedSuggestions — translated from hooks/unifiedSuggestions.ts
// ============================================================================

use super::file_suggestions::{FileSuggesterState, FileSuggestionItem, PathProvider, generate_file_suggestions};

/// Maximum unified suggestions returned. Mirrors `MAX_UNIFIED_SUGGESTIONS`
/// in TS.
pub const MAX_UNIFIED_SUGGESTIONS: usize = 15;

/// Maximum length of a suggestion's description before truncation.
const DESCRIPTION_MAX_LENGTH: usize = 60;

/// One MCP resource definition. Translated from TS `ServerResource`.
#[derive(Debug, Clone)]
pub struct McpResource {
    pub server: String,
    pub uri: String,
    pub name: String,
    pub description: String,
}

/// One agent definition. Translated from TS `AgentDefinition` (the fields
/// relevant to suggestion generation).
#[derive(Debug, Clone)]
pub struct AgentDefinitionRef {
    pub agent_type: String,
    pub when_to_use: String,
    pub color: Option<String>,
}

/// Tagged source for one suggestion before formatting.
#[derive(Debug, Clone)]
pub enum UnifiedSuggestionSource {
    File {
        display_text: String,
        description: Option<String>,
        path: String,
        score: Option<f64>,
    },
    McpResource {
        display_text: String,
        description: String,
        server: String,
        uri: String,
        name: String,
    },
    Agent {
        display_text: String,
        description: String,
        agent_type: String,
        color: Option<String>,
    },
}

/// One formatted suggestion ready to display.
#[derive(Debug, Clone)]
pub struct UnifiedSuggestion {
    pub id: String,
    pub display_text: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

fn truncate_description(s: &str) -> String {
    if s.chars().count() <= DESCRIPTION_MAX_LENGTH {
        s.to_string()
    } else {
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i >= DESCRIPTION_MAX_LENGTH {
                break;
            }
            out.push(c);
        }
        out
    }
}

fn create_suggestion_from_source(source: UnifiedSuggestionSource) -> UnifiedSuggestion {
    match source {
        UnifiedSuggestionSource::File { display_text, description, path, .. } => UnifiedSuggestion {
            id: format!("file-{}", path),
            display_text,
            description,
            color: None,
        },
        UnifiedSuggestionSource::McpResource { display_text, description, server, uri, .. } => UnifiedSuggestion {
            id: format!("mcp-resource-{}__{}", server, uri),
            display_text,
            description: Some(description),
            color: None,
        },
        UnifiedSuggestionSource::Agent { display_text, description, agent_type, color } => UnifiedSuggestion {
            id: format!("agent-{}", agent_type),
            display_text,
            description: Some(description),
            color,
        },
    }
}

fn generate_agent_suggestions(
    agents: &[AgentDefinitionRef],
    query: &str,
    show_on_empty: bool,
) -> Vec<UnifiedSuggestionSource> {
    if query.is_empty() && !show_on_empty {
        return Vec::new();
    }
    let sources: Vec<UnifiedSuggestionSource> = agents
        .iter()
        .map(|a| UnifiedSuggestionSource::Agent {
            display_text: format!("{} (agent)", a.agent_type),
            description: truncate_description(&a.when_to_use),
            agent_type: a.agent_type.clone(),
            color: a.color.clone(),
        })
        .collect();
    if query.is_empty() {
        return sources;
    }
    let q = query.to_lowercase();
    sources
        .into_iter()
        .filter(|s| match s {
            UnifiedSuggestionSource::Agent { display_text, agent_type, .. } => {
                agent_type.to_lowercase().contains(&q)
                    || display_text.to_lowercase().contains(&q)
            }
            _ => false,
        })
        .collect()
}

/// Generate unified suggestions combining file paths, MCP resources, and
/// agent definitions.
///
/// TS source: `generateUnifiedSuggestions(query, mcpResources, agents,
/// showOnEmpty)`. The TS version uses Fuse.js to fuzzy-rank non-file
/// sources; the Rust port uses a simple lowercase-substring filter with
/// an approximate score (Fuse.js scores in [0, 1] where lower is better,
/// our heuristic returns 0.0 for an exact match and 0.5 for a substring
/// hit so the ordering relative to nucleo file scores is preserved).
pub async fn generate_unified_suggestions<P: PathProvider>(
    query: &str,
    mcp_resources: &[McpResource],
    agents: &[AgentDefinitionRef],
    show_on_empty: bool,
    file_provider: &P,
    file_state: &mut FileSuggesterState,
) -> Vec<UnifiedSuggestion> {
    if query.is_empty() && !show_on_empty {
        return Vec::new();
    }

    let file_items: Vec<FileSuggestionItem> =
        generate_file_suggestions(query, show_on_empty, file_provider, file_state).await;
    let agent_sources = generate_agent_suggestions(agents, query, show_on_empty);

    let file_sources: Vec<UnifiedSuggestionSource> = file_items
        .into_iter()
        .map(|s| UnifiedSuggestionSource::File {
            display_text: s.display_text.clone(),
            description: None,
            path: s.display_text,
            score: s.score,
        })
        .collect();

    let mcp_sources: Vec<UnifiedSuggestionSource> = mcp_resources
        .iter()
        .map(|r| UnifiedSuggestionSource::McpResource {
            display_text: format!("{}:{}", r.server, r.uri),
            description: truncate_description(if !r.description.is_empty() {
                &r.description
            } else if !r.name.is_empty() {
                &r.name
            } else {
                &r.uri
            }),
            server: r.server.clone(),
            uri: r.uri.clone(),
            name: if !r.name.is_empty() { r.name.clone() } else { r.uri.clone() },
        })
        .collect();

    if query.is_empty() {
        let all = file_sources
            .into_iter()
            .chain(mcp_sources.into_iter())
            .chain(agent_sources.into_iter())
            .take(MAX_UNIFIED_SUGGESTIONS)
            .map(create_suggestion_from_source)
            .collect();
        return all;
    }

    // Score: lower is better. File sources keep their nucleo score (or
    // 0.5 default). Other sources get a simple substring-match score:
    // 0.0 for exact-prefix, 0.4 for substring, dropped otherwise.
    let q = query.to_lowercase();
    let mut scored: Vec<(f64, UnifiedSuggestionSource)> = Vec::new();
    for source in file_sources {
        let score = match &source {
            UnifiedSuggestionSource::File { score, .. } => score.unwrap_or(0.5),
            _ => 0.5,
        };
        scored.push((score, source));
    }
    for source in mcp_sources.into_iter().chain(agent_sources.into_iter()) {
        let text = match &source {
            UnifiedSuggestionSource::McpResource { display_text, name, server, description, .. } => {
                format!("{} {} {} {}", display_text, name, server, description)
            }
            UnifiedSuggestionSource::Agent { display_text, agent_type, description, .. } => {
                format!("{} {} {}", display_text, agent_type, description)
            }
            _ => String::new(),
        };
        let text_lower = text.to_lowercase();
        let score = if text_lower.starts_with(&q) {
            0.0
        } else if text_lower.contains(&q) {
            0.4
        } else {
            continue;
        };
        scored.push((score, source));
    }

    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(MAX_UNIFIED_SUGGESTIONS)
        .map(|(_, s)| create_suggestion_from_source(s))
        .collect()
}
