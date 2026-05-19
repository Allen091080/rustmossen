//! Merged clients hook (useMergedClients.ts).
//! Combines multiple MCP client connections into a unified client list.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct McpClientEntry {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub connected: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MergedClientsState {
    pub clients: HashMap<String, McpClientEntry>,
    pub merged_order: Vec<String>,
}

impl MergedClientsState {
    pub fn new() -> Self { Self { clients: HashMap::new(), merged_order: Vec::new() } }
    pub fn add_client(&mut self, client: McpClientEntry) {
        let id = client.id.clone();
        self.clients.insert(id.clone(), client);
        if !self.merged_order.contains(&id) { self.merged_order.push(id); }
    }
    pub fn remove_client(&mut self, id: &str) {
        self.clients.remove(id);
        self.merged_order.retain(|i| i != id);
    }
    pub fn get_client(&self, id: &str) -> Option<&McpClientEntry> { self.clients.get(id) }
    pub fn connected_clients(&self) -> Vec<&McpClientEntry> {
        self.merged_order.iter().filter_map(|id| self.clients.get(id)).filter(|c| c.connected).collect()
    }
    pub fn all_clients(&self) -> Vec<&McpClientEntry> {
        self.merged_order.iter().filter_map(|id| self.clients.get(id)).collect()
    }
}
impl Default for MergedClientsState { fn default() -> Self { Self::new() } }

/// Merge two MCP-client lists, deduplicating by client name. The initial
/// list comes first; entries from the second list with names already
/// present are dropped.
///
/// TS source: `mergeClients(initialClients, mcpClients)`.
pub fn merge_clients(
    initial_clients: &[McpClientEntry],
    mcp_clients: &[McpClientEntry],
) -> Vec<McpClientEntry> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut out = Vec::with_capacity(initial_clients.len() + mcp_clients.len());
    for c in initial_clients {
        if seen.insert(c.name.clone()) {
            out.push(c.clone());
        }
    }
    for c in mcp_clients {
        if seen.insert(c.name.clone()) {
            out.push(c.clone());
        }
    }
    out
}
