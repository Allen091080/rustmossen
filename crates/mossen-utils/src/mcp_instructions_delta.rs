use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// MCP instructions delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInstructionsDelta {
    /// Server names — for stateless-scan reconstruction
    pub added_names: Vec<String>,
    /// Rendered "## {name}\n{instructions}" blocks for added_names
    pub added_blocks: Vec<String>,
    pub removed_names: Vec<String>,
}

/// Client-authored instruction block for a server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSideInstruction {
    pub server_name: String,
    pub block: String,
}

/// Simplified MCP server connection for delta calculation
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub connected: bool,
    pub instructions: Option<String>,
}

/// Simplified message for delta scanning
#[derive(Debug, Clone)]
pub struct McpDeltaMessage {
    pub msg_type: String,
    pub attachment_type: Option<String>,
    pub added_names: Vec<String>,
    pub removed_names: Vec<String>,
}

/// Check if MCP instructions delta feature is enabled
pub fn is_mcp_instructions_delta_enabled(
    env_override: Option<&str>,
    user_type: Option<&str>,
    feature_flag: bool,
) -> bool {
    if let Some(val) = env_override {
        let lower = val.to_lowercase();
        if matches!(lower.as_str(), "1" | "true" | "yes") {
            return true;
        }
        if matches!(lower.as_str(), "0" | "false" | "no") {
            return false;
        }
    }
    user_type == Some("internal") || feature_flag
}

/// Diff the current set of connected MCP servers that have instructions
/// against what's already been announced in this conversation. Returns None if
/// nothing changed.
pub fn get_mcp_instructions_delta(
    mcp_clients: &[McpServerInfo],
    messages: &[McpDeltaMessage],
    client_side_instructions: &[ClientSideInstruction],
) -> Option<McpInstructionsDelta> {
    let mut announced = HashSet::new();

    for msg in messages {
        if msg.msg_type != "attachment" {
            continue;
        }
        if msg.attachment_type.as_deref() != Some("mcp_instructions_delta") {
            continue;
        }
        for n in &msg.added_names {
            announced.insert(n.clone());
        }
        for n in &msg.removed_names {
            announced.remove(n);
        }
    }

    let connected: Vec<&McpServerInfo> = mcp_clients.iter().filter(|c| c.connected).collect();
    let connected_names: HashSet<String> = connected.iter().map(|c| c.name.clone()).collect();

    // Servers with instructions to announce
    let mut blocks = std::collections::HashMap::new();
    for c in &connected {
        if let Some(ref instructions) = c.instructions {
            blocks.insert(c.name.clone(), format!("## {}\n{}", c.name, instructions));
        }
    }
    for ci in client_side_instructions {
        if !connected_names.contains(&ci.server_name) {
            continue;
        }
        let existing = blocks.get(&ci.server_name).cloned();
        blocks.insert(
            ci.server_name.clone(),
            match existing {
                Some(existing) => format!("{}\n\n{}", existing, ci.block),
                None => format!("## {}\n{}", ci.server_name, ci.block),
            },
        );
    }

    let mut added: Vec<(String, String)> = Vec::new();
    for (name, block) in &blocks {
        if !announced.contains(name) {
            added.push((name.clone(), block.clone()));
        }
    }

    // Previously-announced servers that are no longer connected → removed
    let mut removed: Vec<String> = Vec::new();
    for n in &announced {
        if !connected_names.contains(n) {
            removed.push(n.clone());
        }
    }

    if added.is_empty() && removed.is_empty() {
        return None;
    }

    added.sort_by(|a, b| a.0.cmp(&b.0));
    removed.sort();

    Some(McpInstructionsDelta {
        added_names: added.iter().map(|a| a.0.clone()).collect(),
        added_blocks: added.iter().map(|a| a.1.clone()).collect(),
        removed_names: removed,
    })
}
