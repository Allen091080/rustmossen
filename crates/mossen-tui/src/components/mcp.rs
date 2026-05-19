//! MCP (Model Context Protocol) server management components.
//!
//! Translates: mcp/index.ts, mcp/CapabilitiesSection.tsx, mcp/ElicitationDialog.tsx,
//! mcp/MCPAgentServerMenu.tsx, mcp/MCPListPanel.tsx, mcp/MCPReconnect.tsx,
//! mcp/MCPRemoteServerMenu.tsx, mcp/MCPSettings.tsx, mcp/MCPStdioServerMenu.tsx,
//! mcp/MCPToolDetailView.tsx, mcp/MCPToolListView.tsx, mcp/McpParsingWarnings.tsx,
//! mcp/utils/reconnectHelpers.tsx

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::components::design_system::DialogWidget;
use crate::theme::Theme;

// ===================================================================
// Types — from mcp/types.ts
// ===================================================================

/// Transport type for an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransport {
    Stdio,
    Sse,
    Http,
    HostedProxy,
}

/// Connection state of an MCP server client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpClientState {
    Connected,
    Pending,
    Failed,
    NeedsAuth,
    Disabled,
}

/// Configuration scope for MCP servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigScope {
    Project,
    Local,
    User,
    Enterprise,
    Dynamic,
}

impl ConfigScope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Project => "Project MCPs",
            Self::Local => "Local MCPs",
            Self::User => "User MCPs",
            Self::Enterprise => "Enterprise MCPs",
            Self::Dynamic => "Built-in",
        }
    }

    pub fn description(&self) -> Option<&'static str> {
        match self {
            Self::Project => Some(".mcp.json"),
            Self::Local => Some(".mossen/mcp.json"),
            Self::User => Some("~/.mossen/mcp.json"),
            Self::Enterprise => Some("enterprise config"),
            Self::Dynamic => Some("built-in"),
        }
    }
}

/// Display order of scopes.
const SCOPE_ORDER: &[ConfigScope] = &[
    ConfigScope::Project,
    ConfigScope::Local,
    ConfigScope::User,
    ConfigScope::Enterprise,
];

/// MCP tool definition.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub server_name: String,
    pub is_read_only: bool,
    pub is_destructive: bool,
    pub is_open_world: bool,
    pub input_schema: Option<McpToolSchema>,
}

/// UI-facing projection of an MCP tool's `input_schema`. Only the fields the
/// inspector renders — property name, type, description, requiredness — are
/// kept; the raw `serde_json::Value` schema continues to be the source of
/// truth held on `McpTool::input_schema`. This view is *deliberately*
/// reduced (not a stub) — it exists so the React-style draw path doesn't
/// have to traverse arbitrary JSON each frame.
#[derive(Debug, Clone)]
pub struct McpToolSchema {
    pub properties: Vec<McpToolProperty>,
    pub required: Vec<String>,
}

/// A single property in a tool's input schema.
#[derive(Debug, Clone)]
pub struct McpToolProperty {
    pub name: String,
    pub type_name: String,
    pub description: Option<String>,
}

/// MCP server info (stdio servers).
#[derive(Debug, Clone)]
pub struct StdioServerInfo {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub scope: ConfigScope,
    pub client_state: McpClientState,
    pub tools_count: usize,
    pub prompts_count: usize,
    pub resources_count: usize,
    pub enabled: bool,
}

/// MCP server info (remote HTTP/SSE servers).
#[derive(Debug, Clone)]
pub struct RemoteServerInfo {
    pub name: String,
    pub url: String,
    pub transport: McpTransport,
    pub scope: ConfigScope,
    pub client_state: McpClientState,
    pub tools_count: usize,
    pub prompts_count: usize,
    pub resources_count: usize,
    pub is_authenticated: bool,
    pub enabled: bool,
}

/// Agent-specific MCP server info.
#[derive(Debug, Clone)]
pub struct AgentMcpServerInfo {
    pub name: String,
    pub url: String,
    pub source_agents: Vec<String>,
    pub transport: McpTransport,
    pub client_state: McpClientState,
    pub needs_auth: bool,
}

/// Unified server info enum.
#[derive(Debug, Clone)]
pub enum ServerInfo {
    Stdio(StdioServerInfo),
    Remote(RemoteServerInfo),
}

impl ServerInfo {
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio(s) => &s.name,
            Self::Remote(s) => &s.name,
        }
    }

    pub fn scope(&self) -> ConfigScope {
        match self {
            Self::Stdio(s) => s.scope,
            Self::Remote(s) => s.scope,
        }
    }

    pub fn client_state(&self) -> &McpClientState {
        match self {
            Self::Stdio(s) => &s.client_state,
            Self::Remote(s) => &s.client_state,
        }
    }

    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Stdio(s) => s.enabled,
            Self::Remote(s) => s.enabled,
        }
    }

    pub fn tools_count(&self) -> usize {
        match self {
            Self::Stdio(s) => s.tools_count,
            Self::Remote(s) => s.tools_count,
        }
    }

    pub fn is_hosted_proxy(&self) -> bool {
        matches!(self, Self::Remote(r) if r.transport == McpTransport::HostedProxy)
    }
}

// ===================================================================
// Reconnect helpers — from mcp/utils/reconnectHelpers.tsx
// ===================================================================

/// Result of a reconnect attempt.
#[derive(Debug, Clone)]
pub struct ReconnectResult {
    pub message: String,
    pub success: bool,
}

/// Handle the result of a reconnect attempt.
pub fn handle_reconnect_result(client_state: &McpClientState, server_name: &str) -> ReconnectResult {
    match client_state {
        McpClientState::Connected => ReconnectResult {
            message: format!("Reconnected to {}.", server_name),
            success: true,
        },
        McpClientState::NeedsAuth => ReconnectResult {
            message: format!("{} requires authentication. Use the 'Authenticate' option.", server_name),
            success: false,
        },
        McpClientState::Failed => ReconnectResult {
            message: format!("Failed to reconnect to {}.", server_name),
            success: false,
        },
        _ => ReconnectResult {
            message: format!("Unknown result when reconnecting to {}.", server_name),
            success: false,
        },
    }
}

/// Handle errors from reconnect attempts.
pub fn handle_reconnect_error(error: &str, server_name: &str) -> String {
    format!("Error reconnecting to {}: {}", server_name, error)
}

// ===================================================================
// CapabilitiesSection — from mcp/CapabilitiesSection.tsx
// ===================================================================

/// Widget showing server capabilities (tools, resources, prompts counts).
pub struct CapabilitiesSectionWidget<'a> {
    pub tools_count: usize,
    pub prompts_count: usize,
    pub resources_count: usize,
    pub theme: &'a Theme,
}

impl<'a> CapabilitiesSectionWidget<'a> {
    pub fn new(tools: usize, prompts: usize, resources: usize, theme: &'a Theme) -> Self {
        Self {
            tools_count: tools,
            prompts_count: prompts,
            resources_count: resources,
            theme,
        }
    }

    fn capabilities_text(&self) -> String {
        let mut parts = Vec::new();
        if self.tools_count > 0 {
            parts.push("tools");
        }
        if self.resources_count > 0 {
            parts.push("resources");
        }
        if self.prompts_count > 0 {
            parts.push("prompts");
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(", ")
        }
    }
}

impl<'a> Widget for CapabilitiesSectionWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 1 {
            return;
        }
        let text = self.capabilities_text();
        let line = Line::from(vec![
            Span::styled("Capabilities: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(text),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

// ===================================================================
// McpParsingWarnings — from mcp/McpParsingWarnings.tsx
// ===================================================================

/// Parsing warnings for MCP configuration.
#[derive(Debug, Clone, Default)]
pub struct McpParsingWarnings {
    pub warnings: Vec<McpParsingWarning>,
}

/// A single parsing warning entry.
#[derive(Debug, Clone)]
pub struct McpParsingWarning {
    pub scope: ConfigScope,
    pub message: String,
}

impl McpParsingWarnings {
    pub fn new() -> Self {
        Self { warnings: Vec::new() }
    }

    pub fn add(&mut self, scope: ConfigScope, message: impl Into<String>) {
        self.warnings.push(McpParsingWarning {
            scope,
            message: message.into(),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// Widget to render MCP parsing warnings.
pub struct McpParsingWarningsWidget<'a> {
    pub warnings: &'a McpParsingWarnings,
    pub theme: &'a Theme,
}

impl<'a> McpParsingWarningsWidget<'a> {
    pub fn new(warnings: &'a McpParsingWarnings, theme: &'a Theme) -> Self {
        Self { warnings, theme }
    }
}

impl<'a> Widget for McpParsingWarningsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.warnings.is_empty() || area.height == 0 {
            return;
        }
        let items: Vec<ListItem> = self
            .warnings
            .warnings
            .iter()
            .map(|w| {
                let scope_label = w.scope.label();
                let line = Line::from(vec![
                    Span::styled("⚠ ", Style::default().fg(self.theme.warning)),
                    Span::styled(
                        format!("[{}] ", scope_label),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&w.message),
                ]);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, area, buf);
    }
}

// ===================================================================
// MCPToolListView — from mcp/MCPToolListView.tsx
// ===================================================================

/// State for the MCP tool list view.
#[derive(Debug, Clone)]
pub struct McpToolListViewState {
    pub server_name: String,
    pub tools: Vec<McpTool>,
    pub selected_index: usize,
}

impl McpToolListViewState {
    pub fn new(server_name: impl Into<String>, tools: Vec<McpTool>) -> Self {
        Self {
            server_name: server_name.into(),
            tools,
            selected_index: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.tools.is_empty() && self.selected_index < self.tools.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn selected_tool(&self) -> Option<&McpTool> {
        self.tools.get(self.selected_index)
    }

    pub fn tool_count_label(&self) -> String {
        let n = self.tools.len();
        if n == 1 {
            "1 tool".to_string()
        } else {
            format!("{} tools", n)
        }
    }
}

/// Widget for the MCP tool list view.
pub struct McpToolListViewWidget<'a> {
    pub state: &'a McpToolListViewState,
    pub theme: &'a Theme,
}

impl<'a> McpToolListViewWidget<'a> {
    pub fn new(state: &'a McpToolListViewState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn build_tool_option(tool: &McpTool) -> Line<'static> {
        let mut spans = vec![Span::raw(tool.display_name.clone())];
        let mut annotations = Vec::new();
        if tool.is_read_only {
            annotations.push("read-only");
        }
        if tool.is_destructive {
            annotations.push("destructive");
        }
        if tool.is_open_world {
            annotations.push("open-world");
        }
        if !annotations.is_empty() {
            spans.push(Span::styled(
                format!(" ({})", annotations.join(", ")),
                Style::default().fg(if tool.is_destructive {
                    Color::Red
                } else if tool.is_read_only {
                    Color::Green
                } else {
                    Color::Gray
                }),
            ));
        }
        Line::from(spans)
    }
}

impl<'a> Widget for McpToolListViewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!("Tools for {}", self.state.server_name);
        let subtitle = self.state.tool_count_label();

        let dialog = DialogWidget::new(&title, self.theme).size(60, 20);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.state.tools.is_empty() {
            let msg = Paragraph::new("No tools available")
                .style(Style::default().fg(Color::DarkGray));
            msg.render(inner, buf);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Subtitle
        Paragraph::new(subtitle)
            .style(Style::default().fg(Color::DarkGray))
            .render(chunks[0], buf);

        // Tool list
        let items: Vec<ListItem> = self
            .state
            .tools
            .iter()
            .enumerate()
            .map(|(i, tool)| {
                let line = Self::build_tool_option(tool);
                let style = if i == self.state.selected_index {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();

        let list = List::new(items);
        Widget::render(list, chunks[1], buf);
    }
}

// ===================================================================
// MCPToolDetailView — from mcp/MCPToolDetailView.tsx
// ===================================================================

/// State for the MCP tool detail view.
#[derive(Debug, Clone)]
pub struct McpToolDetailViewState {
    pub tool: McpTool,
    pub server_name: String,
}

impl McpToolDetailViewState {
    pub fn new(tool: McpTool, server_name: impl Into<String>) -> Self {
        Self {
            tool,
            server_name: server_name.into(),
        }
    }

    fn title_content(&self) -> Vec<Span<'_>> {
        let mut spans = vec![Span::raw(&self.tool.display_name)];
        if self.tool.is_read_only {
            spans.push(Span::styled(" [read-only]", Style::default().fg(Color::Green)));
        }
        if self.tool.is_destructive {
            spans.push(Span::styled(" [destructive]", Style::default().fg(Color::Red)));
        }
        if self.tool.is_open_world {
            spans.push(Span::styled(" [open-world]", Style::default().fg(Color::DarkGray)));
        }
        spans
    }
}

/// Widget for the MCP tool detail view.
pub struct McpToolDetailViewWidget<'a> {
    pub state: &'a McpToolDetailViewState,
    pub theme: &'a Theme,
}

impl<'a> McpToolDetailViewWidget<'a> {
    pub fn new(state: &'a McpToolDetailViewState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for McpToolDetailViewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title_spans = self.state.title_content();
        let title_text: String = title_spans.iter().map(|s| s.content.to_string()).collect();

        let dialog = DialogWidget::new(&title_text, self.theme).size(70, 20);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        // Tool name
        lines.push(Line::from(vec![
            Span::styled("Tool name: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(&self.state.tool.name, Style::default().fg(Color::DarkGray)),
        ]));

        // Full name (same as tool.name in this context)
        lines.push(Line::from(vec![
            Span::styled("Full name: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(&self.state.tool.name, Style::default().fg(Color::DarkGray)),
        ]));

        // Description
        if let Some(ref desc) = self.state.tool.description {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Description:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            // Word-wrap description into available width
            let max_w = inner.width as usize;
            for chunk in desc.as_bytes().chunks(max_w) {
                let s = String::from_utf8_lossy(chunk);
                lines.push(Line::from(s.into_owned()));
            }
        }

        // Parameters
        if let Some(ref schema) = self.state.tool.input_schema {
            if !schema.properties.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Parameters:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for prop in &schema.properties {
                    let is_required = schema.required.contains(&prop.name);
                    let mut parts = vec![
                        Span::raw(format!("  • {}", prop.name)),
                    ];
                    if is_required {
                        parts.push(Span::styled(
                            " (required)",
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                    parts.push(Span::raw(": "));
                    parts.push(Span::styled(
                        &prop.type_name,
                        Style::default().fg(Color::DarkGray),
                    ));
                    if let Some(ref desc) = prop.description {
                        parts.push(Span::styled(
                            format!(" - {}", desc),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                    lines.push(Line::from(parts));
                }
            }
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(inner, buf);
    }
}

// ===================================================================
// MCPReconnect — from mcp/MCPReconnect.tsx
// ===================================================================

/// State for reconnecting an MCP server.
#[derive(Debug, Clone)]
pub struct McpReconnectState {
    pub server_name: String,
    pub is_reconnecting: bool,
    pub result: Option<ReconnectResult>,
    pub error: Option<String>,
}

impl McpReconnectState {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            is_reconnecting: false,
            result: None,
            error: None,
        }
    }

    pub fn start_reconnect(&mut self) {
        self.is_reconnecting = true;
        self.result = None;
        self.error = None;
    }

    pub fn finish_reconnect(&mut self, client_state: &McpClientState) {
        self.is_reconnecting = false;
        self.result = Some(handle_reconnect_result(client_state, &self.server_name));
    }

    pub fn set_error(&mut self, err: impl Into<String>) {
        self.is_reconnecting = false;
        let err_str = err.into();
        self.error = Some(handle_reconnect_error(&err_str, &self.server_name));
    }

    pub fn status_message(&self) -> Option<&str> {
        if self.is_reconnecting {
            return Some("Reconnecting...");
        }
        if let Some(ref err) = self.error {
            return Some(err.as_str());
        }
        if let Some(ref res) = self.result {
            return Some(res.message.as_str());
        }
        None
    }
}

/// Widget for displaying reconnection status.
pub struct McpReconnectWidget<'a> {
    pub state: &'a McpReconnectState,
    pub theme: &'a Theme,
}

impl<'a> McpReconnectWidget<'a> {
    pub fn new(state: &'a McpReconnectState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for McpReconnectWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let msg = match self.state.status_message() {
            Some(m) => m,
            None => return,
        };

        let color = if self.state.is_reconnecting {
            Color::Yellow
        } else if self.state.error.is_some() {
            Color::Red
        } else if self.state.result.as_ref().map_or(false, |r| r.success) {
            Color::Green
        } else {
            Color::Red
        };

        let paragraph = Paragraph::new(msg).style(Style::default().fg(color));
        paragraph.render(area, buf);
    }
}

// ===================================================================
// MCPStdioServerMenu — from mcp/MCPStdioServerMenu.tsx
// ===================================================================

/// Menu actions for stdio servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdioMenuAction {
    ViewTools,
    Reconnect,
    ToggleEnabled,
    Back,
}

/// State for the stdio server menu.
#[derive(Debug, Clone)]
pub struct McpStdioServerMenuState {
    pub server: StdioServerInfo,
    pub selected_index: usize,
    pub is_reconnecting: bool,
    pub error: Option<String>,
}

impl McpStdioServerMenuState {
    pub fn new(server: StdioServerInfo) -> Self {
        Self {
            server,
            selected_index: 0,
            is_reconnecting: false,
            error: None,
        }
    }

    pub fn menu_options(&self) -> Vec<(StdioMenuAction, String)> {
        let mut options = Vec::new();
        if self.server.client_state == McpClientState::Connected && self.server.tools_count > 0 {
            options.push((
                StdioMenuAction::ViewTools,
                format!("View tools ({})", self.server.tools_count),
            ));
        }
        if self.server.client_state != McpClientState::Disabled {
            options.push((
                StdioMenuAction::Reconnect,
                "Reconnect".to_string(),
            ));
        }
        let toggle_label = if self.server.enabled {
            "Disable server"
        } else {
            "Enable server"
        };
        options.push((StdioMenuAction::ToggleEnabled, toggle_label.to_string()));
        options.push((StdioMenuAction::Back, "Back".to_string()));
        options
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.menu_options().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    pub fn selected_action(&self) -> Option<StdioMenuAction> {
        self.menu_options()
            .get(self.selected_index)
            .map(|(action, _)| action.clone())
    }
}

/// Widget for the stdio server menu.
pub struct McpStdioServerMenuWidget<'a> {
    pub state: &'a McpStdioServerMenuState,
    pub theme: &'a Theme,
}

impl<'a> McpStdioServerMenuWidget<'a> {
    pub fn new(state: &'a McpStdioServerMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn status_line(server: &StdioServerInfo, theme: &Theme) -> Line<'static> {
        let (icon, label, color) = match server.client_state {
            McpClientState::Connected => ("✓", "connected", theme.success),
            McpClientState::Pending => ("○", "connecting…", Color::DarkGray),
            McpClientState::Failed => ("✗", "failed", theme.error),
            McpClientState::NeedsAuth => ("△", "needs authentication", theme.warning),
            McpClientState::Disabled => ("○", "disabled", Color::DarkGray),
        };
        Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} {}", icon, label), Style::default().fg(color)),
        ])
    }
}

impl<'a> Widget for McpStdioServerMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new(&self.state.server.name, self.theme).size(60, 18);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Info section
                Constraint::Length(1), // Spacer
                Constraint::Min(1),    // Menu options
            ])
            .split(inner);

        // Info section
        let mut info_lines = Vec::new();
        info_lines.push(Self::status_line(&self.state.server, self.theme));
        info_lines.push(Line::from(vec![
            Span::styled("Command: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(&self.state.server.command, Style::default().fg(Color::DarkGray)),
        ]));
        if let Some(desc) = self.state.server.scope.description() {
            info_lines.push(Line::from(vec![
                Span::styled("Config: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(desc, Style::default().fg(Color::DarkGray)),
            ]));
        }
        if self.state.server.client_state == McpClientState::Connected {
            info_lines.push(Line::from(vec![
                Span::styled("Tools: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{} tools", self.state.server.tools_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        Paragraph::new(info_lines).render(chunks[0], buf);

        // Error display
        if let Some(ref err) = self.state.error {
            let err_line = Paragraph::new(err.as_str())
                .style(Style::default().fg(Color::Red));
            err_line.render(chunks[1], buf);
        }

        // Menu options
        let options = self.state.menu_options();
        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, (_, label))| {
                let prefix = if i == self.state.selected_index { "❯ " } else { "  " };
                let style = if i == self.state.selected_index {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}", prefix, label)).style(style)
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, chunks[2], buf);
    }
}

// ===================================================================
// MCPAgentServerMenu — from mcp/MCPAgentServerMenu.tsx
// ===================================================================

/// Menu actions for agent servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMenuAction {
    Authenticate,
    Back,
}

/// State for the agent server menu.
#[derive(Debug, Clone)]
pub struct McpAgentServerMenuState {
    pub server: AgentMcpServerInfo,
    pub is_authenticating: bool,
    pub error: Option<String>,
    pub authorization_url: Option<String>,
    pub selected_index: usize,
}

impl McpAgentServerMenuState {
    pub fn new(server: AgentMcpServerInfo) -> Self {
        Self {
            server,
            is_authenticating: false,
            error: None,
            authorization_url: None,
            selected_index: 0,
        }
    }

    pub fn menu_options(&self) -> Vec<(AgentMenuAction, String)> {
        let mut options = Vec::new();
        if self.server.needs_auth && !self.is_authenticating {
            options.push((AgentMenuAction::Authenticate, "Authenticate".to_string()));
        }
        options.push((AgentMenuAction::Back, "Back".to_string()));
        options
    }

    pub fn start_auth(&mut self) {
        self.is_authenticating = true;
        self.error = None;
    }

    pub fn finish_auth_success(&mut self, url: String) {
        self.is_authenticating = false;
        self.authorization_url = Some(url);
    }

    pub fn finish_auth_error(&mut self, err: impl Into<String>) {
        self.is_authenticating = false;
        self.error = Some(err.into());
    }

    pub fn cancel_auth(&mut self) {
        self.is_authenticating = false;
        self.error = None;
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.menu_options().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    pub fn selected_action(&self) -> Option<AgentMenuAction> {
        self.menu_options()
            .get(self.selected_index)
            .map(|(a, _)| a.clone())
    }
}

/// Widget for the agent server menu.
pub struct McpAgentServerMenuWidget<'a> {
    pub state: &'a McpAgentServerMenuState,
    pub theme: &'a Theme,
}

impl<'a> McpAgentServerMenuWidget<'a> {
    pub fn new(state: &'a McpAgentServerMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for McpAgentServerMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!("Agent MCP: {}", self.state.server.name);
        let dialog = DialogWidget::new(&title, self.theme).size(60, 14);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Info
                Constraint::Length(1), // Spacer/error
                Constraint::Min(1),    // Menu
            ])
            .split(inner);

        // Info
        let mut info_lines = Vec::new();
        info_lines.push(Line::from(vec![
            Span::styled("Server: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&self.state.server.name),
        ]));
        info_lines.push(Line::from(vec![
            Span::styled("URL: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(&self.state.server.url, Style::default().fg(Color::DarkGray)),
        ]));
        let agents_str = self.state.server.source_agents.join(", ");
        info_lines.push(Line::from(vec![
            Span::styled("Agents: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(agents_str, Style::default().fg(Color::DarkGray)),
        ]));
        Paragraph::new(info_lines).render(chunks[0], buf);

        // Error or authenticating state
        if self.state.is_authenticating {
            let spinner_text = Paragraph::new("Authenticating...")
                .style(Style::default().fg(Color::Yellow));
            spinner_text.render(chunks[1], buf);
        } else if let Some(ref err) = self.state.error {
            let err_text = Paragraph::new(err.as_str())
                .style(Style::default().fg(Color::Red));
            err_text.render(chunks[1], buf);
        } else if let Some(ref url) = self.state.authorization_url {
            let url_text = Paragraph::new(format!("Auth URL: {}", url))
                .style(Style::default().fg(Color::Blue));
            url_text.render(chunks[1], buf);
        }

        // Menu options
        let options = self.state.menu_options();
        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, (_, label))| {
                let prefix = if i == self.state.selected_index { "❯ " } else { "  " };
                let style = if i == self.state.selected_index {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}", prefix, label)).style(style)
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, chunks[2], buf);
    }
}

// ===================================================================
// MCPRemoteServerMenu — from mcp/MCPRemoteServerMenu.tsx
// ===================================================================

/// Menu actions for remote (HTTP/SSE/Hosted) servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteMenuAction {
    ViewTools,
    Authenticate,
    Reauthenticate,
    ClearAuth,
    HostedAuth,
    HostedClearAuth,
    Reconnect,
    ToggleEnabled,
    Back,
}

/// State for the remote server menu.
#[derive(Debug, Clone)]
pub struct McpRemoteServerMenuState {
    pub server: RemoteServerInfo,
    pub selected_index: usize,
    pub is_authenticating: bool,
    pub is_reconnecting: bool,
    pub error: Option<String>,
    pub clipboard_text: Option<String>,
}

impl McpRemoteServerMenuState {
    pub fn new(server: RemoteServerInfo) -> Self {
        Self {
            server,
            selected_index: 0,
            is_authenticating: false,
            is_reconnecting: false,
            error: None,
            clipboard_text: None,
        }
    }

    pub fn menu_options(&self) -> Vec<(RemoteMenuAction, String)> {
        let mut options = Vec::new();

        // View tools if connected with tools
        if self.server.client_state == McpClientState::Connected && self.server.tools_count > 0 {
            options.push((
                RemoteMenuAction::ViewTools,
                format!("View tools ({})", self.server.tools_count),
            ));
        }

        // Auth options based on transport and state
        if self.server.transport == McpTransport::HostedProxy {
            if !self.server.is_authenticated {
                options.push((RemoteMenuAction::HostedAuth, "Authenticate".to_string()));
            } else {
                options.push((RemoteMenuAction::HostedClearAuth, "Clear authentication".to_string()));
            }
        } else {
            match self.server.client_state {
                McpClientState::NeedsAuth => {
                    options.push((RemoteMenuAction::Authenticate, "Authenticate".to_string()));
                }
                McpClientState::Connected => {
                    if self.server.is_authenticated {
                        options.push((RemoteMenuAction::Reauthenticate, "Re-authenticate".to_string()));
                        options.push((RemoteMenuAction::ClearAuth, "Clear authentication".to_string()));
                    } else {
                        options.push((RemoteMenuAction::Authenticate, "Authenticate".to_string()));
                    }
                }
                _ => {}
            }
        }

        // Reconnect
        if self.server.client_state != McpClientState::Disabled {
            options.push((RemoteMenuAction::Reconnect, "Reconnect".to_string()));
        }

        // Toggle enabled
        let toggle_label = if self.server.enabled {
            "Disable server"
        } else {
            "Enable server"
        };
        options.push((RemoteMenuAction::ToggleEnabled, toggle_label.to_string()));

        options.push((RemoteMenuAction::Back, "Back".to_string()));
        options
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.menu_options().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    pub fn selected_action(&self) -> Option<RemoteMenuAction> {
        self.menu_options()
            .get(self.selected_index)
            .map(|(a, _)| a.clone())
    }

    pub fn start_auth(&mut self) {
        self.is_authenticating = true;
        self.error = None;
    }

    pub fn finish_auth(&mut self) {
        self.is_authenticating = false;
    }

    pub fn start_reconnect(&mut self) {
        self.is_reconnecting = true;
        self.error = None;
    }

    pub fn finish_reconnect(&mut self, state: McpClientState) {
        self.is_reconnecting = false;
        self.server.client_state = state;
    }

    pub fn set_error(&mut self, err: impl Into<String>) {
        self.is_authenticating = false;
        self.is_reconnecting = false;
        self.error = Some(err.into());
    }
}

/// Widget for the remote server menu.
pub struct McpRemoteServerMenuWidget<'a> {
    pub state: &'a McpRemoteServerMenuState,
    pub theme: &'a Theme,
}

impl<'a> McpRemoteServerMenuWidget<'a> {
    pub fn new(state: &'a McpRemoteServerMenuState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn status_line(server: &RemoteServerInfo, theme: &Theme) -> Line<'static> {
        let (icon, label, color) = match server.client_state {
            McpClientState::Connected => ("✓", "connected", theme.success),
            McpClientState::Pending => ("○", "connecting…", Color::DarkGray),
            McpClientState::Failed => ("✗", "failed", theme.error),
            McpClientState::NeedsAuth => ("△", "needs authentication", theme.warning),
            McpClientState::Disabled => ("○", "disabled", Color::DarkGray),
        };
        Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} {}", icon, label), Style::default().fg(color)),
        ])
    }

    fn auth_line(server: &RemoteServerInfo, theme: &Theme) -> Line<'static> {
        let (icon, label, color) = if server.is_authenticated {
            ("✓", "authenticated", theme.success)
        } else {
            ("✗", "not authenticated", theme.error)
        };
        Line::from(vec![
            Span::styled("Auth: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} {}", icon, label), Style::default().fg(color)),
        ])
    }
}

impl<'a> Widget for McpRemoteServerMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new(&self.state.server.name, self.theme).size(65, 22);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6), // Info section
                Constraint::Length(1), // Error/status
                Constraint::Min(1),    // Menu
            ])
            .split(inner);

        // Info section
        let mut info_lines = Vec::new();
        info_lines.push(Self::status_line(&self.state.server, self.theme));

        // Auth line (not for hosted-proxy)
        if self.state.server.transport != McpTransport::HostedProxy {
            info_lines.push(Self::auth_line(&self.state.server, self.theme));
        }

        info_lines.push(Line::from(vec![
            Span::styled("URL: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(&self.state.server.url, Style::default().fg(Color::DarkGray)),
        ]));

        if let Some(desc) = self.state.server.scope.description() {
            info_lines.push(Line::from(vec![
                Span::styled("Config location: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(desc, Style::default().fg(Color::DarkGray)),
            ]));
        }

        // Capabilities line
        if self.state.server.client_state == McpClientState::Connected {
            info_lines.push(Line::from(vec![
                Span::styled("Tools: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{} tools", self.state.server.tools_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        Paragraph::new(info_lines).render(chunks[0], buf);

        // Error / authenticating / reconnecting
        if self.state.is_authenticating {
            let text = Paragraph::new("Authenticating...")
                .style(Style::default().fg(Color::Yellow));
            text.render(chunks[1], buf);
        } else if self.state.is_reconnecting {
            let text = Paragraph::new("Reconnecting...")
                .style(Style::default().fg(Color::Yellow));
            text.render(chunks[1], buf);
        } else if let Some(ref err) = self.state.error {
            let text = Paragraph::new(format!("Error: {}", err))
                .style(Style::default().fg(Color::Red));
            text.render(chunks[1], buf);
        }

        // Menu options
        let options = self.state.menu_options();
        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, (_, label))| {
                let prefix = if i == self.state.selected_index { "❯ " } else { "  " };
                let style = if i == self.state.selected_index {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}", prefix, label)).style(style)
            })
            .collect();
        let list = List::new(items);
        Widget::render(list, chunks[2], buf);
    }
}

// ===================================================================
// MCPListPanel — from mcp/MCPListPanel.tsx
// ===================================================================

/// Server item in the list panel.
#[derive(Debug, Clone)]
pub struct McpListPanelItem {
    pub server_info: ServerInfo,
}

/// State for the MCP list panel.
#[derive(Debug, Clone)]
pub struct McpListPanelState {
    pub servers: Vec<ServerInfo>,
    pub agent_servers: Vec<AgentMcpServerInfo>,
    pub selected_index: usize,
    pub parsing_warnings: McpParsingWarnings,
}

impl McpListPanelState {
    pub fn new(servers: Vec<ServerInfo>, agent_servers: Vec<AgentMcpServerInfo>) -> Self {
        Self {
            servers,
            agent_servers,
            selected_index: 0,
            parsing_warnings: McpParsingWarnings::new(),
        }
    }

    pub fn total_count(&self) -> usize {
        self.servers.len() + self.agent_servers.len()
    }

    pub fn total_label(&self) -> String {
        let n = self.total_count();
        if n == 1 {
            "1 server".to_string()
        } else {
            format!("{} servers", n)
        }
    }

    /// Get servers grouped by scope.
    pub fn servers_by_scope(&self) -> Vec<(ConfigScope, Vec<&ServerInfo>)> {
        let mut result = Vec::new();
        for &scope in SCOPE_ORDER {
            let scope_servers: Vec<&ServerInfo> = self
                .servers
                .iter()
                .filter(|s| s.scope() == scope && !s.is_hosted_proxy())
                .collect();
            if !scope_servers.is_empty() {
                result.push((scope, scope_servers));
            }
        }
        result
    }

    /// Get hosted proxy servers.
    pub fn hosted_servers(&self) -> Vec<&ServerInfo> {
        self.servers.iter().filter(|s| s.is_hosted_proxy()).collect()
    }

    /// Get dynamic/built-in servers.
    pub fn dynamic_servers(&self) -> Vec<&ServerInfo> {
        self.servers
            .iter()
            .filter(|s| s.scope() == ConfigScope::Dynamic)
            .collect()
    }

    /// Check if any servers have failed.
    pub fn has_failed_clients(&self) -> bool {
        self.servers
            .iter()
            .any(|s| *s.client_state() == McpClientState::Failed)
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.total_count().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    /// Get the selected item.
    pub fn selected_server(&self) -> Option<&ServerInfo> {
        self.servers.get(self.selected_index)
    }
}

/// Widget for the MCP list panel.
pub struct McpListPanelWidget<'a> {
    pub state: &'a McpListPanelState,
    pub theme: &'a Theme,
}

impl<'a> McpListPanelWidget<'a> {
    pub fn new(state: &'a McpListPanelState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn render_server_item(server: &ServerInfo, is_selected: bool, theme: &Theme) -> ListItem<'static> {
        let (status_icon, status_color) = match server.client_state() {
            McpClientState::Connected => ("✓", theme.success),
            McpClientState::Pending => ("○", Color::DarkGray),
            McpClientState::Failed => ("✗", theme.error),
            McpClientState::NeedsAuth => ("△", theme.warning),
            McpClientState::Disabled => ("○", Color::DarkGray),
        };

        let name = server.name().to_string();
        let tools_count = server.tools_count();
        let tools_label = if tools_count > 0 {
            format!(" ({} tools)", tools_count)
        } else {
            String::new()
        };

        let prefix = if is_selected { "❯ " } else { "  " };
        let style = if is_selected {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let line = Line::from(vec![
            Span::raw(prefix.to_string()),
            Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
            Span::styled(name, style),
            Span::styled(tools_label, Style::default().fg(Color::DarkGray)),
        ]);
        ListItem::new(line)
    }

    fn render_agent_server_item(
        server: &AgentMcpServerInfo,
        is_selected: bool,
        theme: &Theme,
    ) -> ListItem<'static> {
        let (status_icon, status_color) = match server.client_state {
            McpClientState::Connected => ("✓", theme.success),
            McpClientState::Pending => ("○", Color::DarkGray),
            McpClientState::Failed => ("✗", theme.error),
            McpClientState::NeedsAuth => ("△", theme.warning),
            McpClientState::Disabled => ("○", Color::DarkGray),
        };

        let prefix = if is_selected { "❯ " } else { "  " };
        let style = if is_selected {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let line = Line::from(vec![
            Span::raw(prefix.to_string()),
            Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
            Span::styled(server.name.clone(), style),
        ]);
        ListItem::new(line)
    }
}

impl<'a> Widget for McpListPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let subtitle = self.state.total_label();
        let dialog = DialogWidget::new("Manage MCP servers", self.theme).size(65, 24);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Subtitle
                Constraint::Length(1), // Warnings
                Constraint::Min(1),    // Server list
                Constraint::Length(2), // Footer
            ])
            .split(inner);

        // Subtitle
        Paragraph::new(subtitle)
            .style(Style::default().fg(Color::DarkGray))
            .render(chunks[0], buf);

        // Parsing warnings
        if !self.state.parsing_warnings.is_empty() {
            McpParsingWarningsWidget::new(&self.state.parsing_warnings, self.theme)
                .render(chunks[1], buf);
        }

        // Server list grouped by scope
        let mut items: Vec<ListItem> = Vec::new();
        let mut flat_index = 0usize;

        // Scoped servers
        for (scope, scope_servers) in self.state.servers_by_scope() {
            // Scope heading
            let heading_text = format!("  {}", scope.label());
            let heading_path = scope.description().unwrap_or("");
            items.push(ListItem::new(Line::from(vec![
                Span::styled(heading_text, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!(" ({})", heading_path),
                    Style::default().fg(Color::DarkGray),
                ),
            ])));

            for server in scope_servers {
                let is_selected = flat_index == self.state.selected_index;
                items.push(Self::render_server_item(server, is_selected, self.theme));
                flat_index += 1;
            }
        }

        // Hosted servers
        let hosted = self.state.hosted_servers();
        if !hosted.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  Hosted MCPs",
                Style::default().add_modifier(Modifier::BOLD),
            ))));
            for server in hosted {
                let is_selected = flat_index == self.state.selected_index;
                items.push(Self::render_server_item(server, is_selected, self.theme));
                flat_index += 1;
            }
        }

        // Agent servers
        if !self.state.agent_servers.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  Agent MCPs",
                Style::default().add_modifier(Modifier::BOLD),
            ))));
            for agent_server in &self.state.agent_servers {
                let is_selected = flat_index == self.state.selected_index;
                items.push(Self::render_agent_server_item(
                    agent_server,
                    is_selected,
                    self.theme,
                ));
                flat_index += 1;
            }
        }

        let list = List::new(items);
        Widget::render(list, chunks[2], buf);

        // Footer
        let has_failed = self.state.has_failed_clients();
        let mut footer_lines = Vec::new();
        if has_failed {
            footer_lines.push(Line::from(Span::styled(
                "※ Run mossen --debug to see error logs",
                Style::default().fg(Color::DarkGray),
            )));
        }
        footer_lines.push(Line::from(Span::styled(
            "↑↓ navigate  Enter confirm  Esc cancel",
            Style::default().fg(Color::DarkGray),
        )));
        Paragraph::new(footer_lines).render(chunks[3], buf);
    }
}

// ===================================================================
// MCPSettings — from mcp/MCPSettings.tsx (view state machine)
// ===================================================================

/// View state for MCP settings navigation.
#[derive(Debug, Clone)]
pub enum McpViewState {
    List { default_tab: Option<String> },
    ServerMenu { server: ServerInfo },
    ServerTools { server: ServerInfo },
    ServerToolDetail { server: ServerInfo, tool_index: usize },
    AgentServerMenu { agent_server: AgentMcpServerInfo },
}

/// Top-level MCP settings state.
#[derive(Debug, Clone)]
pub struct McpSettingsState {
    pub view: McpViewState,
    pub list_panel: McpListPanelState,
    pub tool_list: Option<McpToolListViewState>,
    pub tool_detail: Option<McpToolDetailViewState>,
    pub stdio_menu: Option<McpStdioServerMenuState>,
    pub remote_menu: Option<McpRemoteServerMenuState>,
    pub agent_menu: Option<McpAgentServerMenuState>,
    pub elicitation: Option<ElicitationDialogState>,
    pub tools: Vec<McpTool>,
}

impl McpSettingsState {
    pub fn new(servers: Vec<ServerInfo>, agent_servers: Vec<AgentMcpServerInfo>, tools: Vec<McpTool>) -> Self {
        let list_panel = McpListPanelState::new(servers, agent_servers);
        Self {
            view: McpViewState::List { default_tab: None },
            list_panel,
            tool_list: None,
            tool_detail: None,
            stdio_menu: None,
            remote_menu: None,
            agent_menu: None,
            elicitation: None,
            tools,
        }
    }

    pub fn navigate_to_server(&mut self, server: ServerInfo) {
        self.view = McpViewState::ServerMenu { server };
    }

    pub fn navigate_to_tools(&mut self, server: ServerInfo) {
        let server_tools: Vec<McpTool> = self
            .tools
            .iter()
            .filter(|t| t.server_name == server.name().to_string())
            .cloned()
            .collect();
        let tool_list = McpToolListViewState::new(server.name(), server_tools);
        self.tool_list = Some(tool_list);
        self.view = McpViewState::ServerTools { server };
    }

    pub fn navigate_to_tool_detail(&mut self, server: ServerInfo, tool_index: usize) {
        let server_tools: Vec<McpTool> = self
            .tools
            .iter()
            .filter(|t| t.server_name == server.name().to_string())
            .cloned()
            .collect();
        if let Some(tool) = server_tools.get(tool_index).cloned() {
            let detail = McpToolDetailViewState::new(tool, server.name());
            self.tool_detail = Some(detail);
            self.view = McpViewState::ServerToolDetail { server, tool_index };
        }
    }

    pub fn navigate_to_agent_server(&mut self, agent_server: AgentMcpServerInfo) {
        let menu = McpAgentServerMenuState::new(agent_server.clone());
        self.agent_menu = Some(menu);
        self.view = McpViewState::AgentServerMenu { agent_server };
    }

    pub fn navigate_back(&mut self) {
        match &self.view {
            McpViewState::ServerMenu { .. } => {
                self.view = McpViewState::List { default_tab: None };
            }
            McpViewState::ServerTools { server } => {
                let s = server.clone();
                self.tool_list = None;
                self.view = McpViewState::ServerMenu { server: s };
            }
            McpViewState::ServerToolDetail { server, .. } => {
                let s = server.clone();
                self.tool_detail = None;
                self.view = McpViewState::ServerTools { server: s };
            }
            McpViewState::AgentServerMenu { .. } => {
                self.agent_menu = None;
                self.view = McpViewState::List {
                    default_tab: Some("Agents".to_string()),
                };
            }
            McpViewState::List { .. } => {}
        }
    }

    /// Filter tools by server name.
    pub fn tools_for_server(&self, server_name: &str) -> Vec<&McpTool> {
        self.tools.iter().filter(|t| t.server_name == server_name).collect()
    }
}

/// Widget for MCP settings (dispatches to sub-widgets based on view state).
pub struct McpSettingsWidget<'a> {
    pub state: &'a McpSettingsState,
    pub theme: &'a Theme,
}

impl<'a> McpSettingsWidget<'a> {
    pub fn new(state: &'a McpSettingsState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for McpSettingsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match &self.state.view {
            McpViewState::List { .. } => {
                McpListPanelWidget::new(&self.state.list_panel, self.theme).render(area, buf);
            }
            McpViewState::ServerMenu { server } => match server {
                ServerInfo::Stdio(s) => {
                    if let Some(ref menu) = self.state.stdio_menu {
                        McpStdioServerMenuWidget::new(menu, self.theme).render(area, buf);
                    } else {
                        let temp = McpStdioServerMenuState::new(s.clone());
                        McpStdioServerMenuWidget::new(&temp, self.theme).render(area, buf);
                    }
                }
                ServerInfo::Remote(r) => {
                    if let Some(ref menu) = self.state.remote_menu {
                        McpRemoteServerMenuWidget::new(menu, self.theme).render(area, buf);
                    } else {
                        let temp = McpRemoteServerMenuState::new(r.clone());
                        McpRemoteServerMenuWidget::new(&temp, self.theme).render(area, buf);
                    }
                }
            },
            McpViewState::ServerTools { .. } => {
                if let Some(ref tool_list) = self.state.tool_list {
                    McpToolListViewWidget::new(tool_list, self.theme).render(area, buf);
                }
            }
            McpViewState::ServerToolDetail { .. } => {
                if let Some(ref detail) = self.state.tool_detail {
                    McpToolDetailViewWidget::new(detail, self.theme).render(area, buf);
                }
            }
            McpViewState::AgentServerMenu { .. } => {
                if let Some(ref menu) = self.state.agent_menu {
                    McpAgentServerMenuWidget::new(menu, self.theme).render(area, buf);
                }
            }
        }
    }
}

// ===================================================================
// ElicitationDialog — from mcp/ElicitationDialog.tsx
// ===================================================================

/// Schema field types for elicitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElicitFieldType {
    Text,
    Number,
    Boolean,
    Enum(Vec<String>),
    MultiSelectEnum(Vec<String>),
    Date,
}

/// A single field in the elicitation schema.
#[derive(Debug, Clone)]
pub struct ElicitSchemaField {
    pub name: String,
    pub label: String,
    pub field_type: ElicitFieldType,
    pub is_required: bool,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
}

impl ElicitSchemaField {
    pub fn is_text_field(&self) -> bool {
        matches!(self.field_type, ElicitFieldType::Text | ElicitFieldType::Number | ElicitFieldType::Date)
    }

    pub fn is_enum_schema(&self) -> bool {
        matches!(self.field_type, ElicitFieldType::Enum(_) | ElicitFieldType::MultiSelectEnum(_))
    }

    pub fn is_multi_select(&self) -> bool {
        matches!(self.field_type, ElicitFieldType::MultiSelectEnum(_))
    }

    pub fn enum_options(&self) -> &[String] {
        match &self.field_type {
            ElicitFieldType::Enum(opts) | ElicitFieldType::MultiSelectEnum(opts) => opts,
            _ => &[],
        }
    }
}

/// Validation error for a field.
#[derive(Debug, Clone)]
pub struct FieldValidationError {
    pub field_name: String,
    pub message: String,
}

/// State for the elicitation dialog.
#[derive(Debug, Clone)]
pub struct ElicitationDialogState {
    pub server_name: String,
    pub message: String,
    pub fields: Vec<ElicitSchemaField>,
    pub form_values: std::collections::HashMap<String, String>,
    pub multi_select_values: std::collections::HashMap<String, Vec<String>>,
    pub current_field_index: Option<usize>,
    pub focused_button: Option<ElicitButton>,
    pub validation_errors: Vec<FieldValidationError>,
    pub text_input_value: String,
    pub text_input_cursor: usize,
    pub enum_typeahead: String,
}

/// Button focus state in elicitation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElicitButton {
    Submit,
    Cancel,
}

impl ElicitationDialogState {
    pub fn new(
        server_name: impl Into<String>,
        message: impl Into<String>,
        fields: Vec<ElicitSchemaField>,
    ) -> Self {
        let mut form_values = std::collections::HashMap::new();
        for field in &fields {
            if let Some(ref default) = field.default_value {
                form_values.insert(field.name.clone(), default.clone());
            }
        }
        let initial_field = if fields.is_empty() { None } else { Some(0) };
        Self {
            server_name: server_name.into(),
            message: message.into(),
            fields,
            form_values,
            multi_select_values: std::collections::HashMap::new(),
            current_field_index: initial_field,
            focused_button: None,
            validation_errors: Vec::new(),
            text_input_value: String::new(),
            text_input_cursor: 0,
            enum_typeahead: String::new(),
        }
    }

    pub fn current_field(&self) -> Option<&ElicitSchemaField> {
        self.current_field_index.and_then(|i| self.fields.get(i))
    }

    pub fn move_to_next_field(&mut self) {
        if let Some(idx) = self.current_field_index {
            if idx + 1 < self.fields.len() {
                self.current_field_index = Some(idx + 1);
                self.sync_text_input();
            } else {
                self.focused_button = Some(ElicitButton::Submit);
                self.current_field_index = None;
            }
        }
    }

    pub fn move_to_prev_field(&mut self) {
        if let Some(btn) = self.focused_button {
            match btn {
                ElicitButton::Submit | ElicitButton::Cancel => {
                    if !self.fields.is_empty() {
                        self.current_field_index = Some(self.fields.len() - 1);
                        self.focused_button = None;
                        self.sync_text_input();
                    }
                }
            }
        } else if let Some(idx) = self.current_field_index {
            if idx > 0 {
                self.current_field_index = Some(idx - 1);
                self.sync_text_input();
            }
        }
    }

    pub fn toggle_button(&mut self) {
        match self.focused_button {
            Some(ElicitButton::Submit) => self.focused_button = Some(ElicitButton::Cancel),
            Some(ElicitButton::Cancel) => self.focused_button = Some(ElicitButton::Submit),
            None => {}
        }
    }

    fn sync_text_input(&mut self) {
        if let Some(field) = self.current_field() {
            if field.is_text_field() && !field.is_enum_schema() {
                let val = self
                    .form_values
                    .get(&field.name)
                    .cloned()
                    .unwrap_or_default();
                self.text_input_cursor = val.len();
                self.text_input_value = val;
            }
        }
    }

    pub fn set_field_value(&mut self, field_name: &str, value: String) {
        self.form_values.insert(field_name.to_string(), value);
    }

    pub fn toggle_multi_select(&mut self, field_name: &str, value: String) {
        let entry = self
            .multi_select_values
            .entry(field_name.to_string())
            .or_default();
        if let Some(pos) = entry.iter().position(|v| v == &value) {
            entry.remove(pos);
        } else {
            entry.push(value);
        }
    }

    pub fn validate(&mut self) -> bool {
        self.validation_errors.clear();
        for field in &self.fields {
            if field.is_required {
                if field.is_multi_select() {
                    let selected = self
                        .multi_select_values
                        .get(&field.name)
                        .map_or(0, |v| v.len());
                    if let Some(min) = field.min_items {
                        if selected < min {
                            self.validation_errors.push(FieldValidationError {
                                field_name: field.name.clone(),
                                message: format!("At least {} selection(s) required", min),
                            });
                        }
                    }
                    if selected == 0 {
                        self.validation_errors.push(FieldValidationError {
                            field_name: field.name.clone(),
                            message: "This field is required".to_string(),
                        });
                    }
                } else {
                    let val = self.form_values.get(&field.name);
                    if val.map_or(true, |v| v.trim().is_empty()) {
                        self.validation_errors.push(FieldValidationError {
                            field_name: field.name.clone(),
                            message: "This field is required".to_string(),
                        });
                    }
                }
            }
            // Check max_items for multi-select
            if field.is_multi_select() {
                if let Some(max) = field.max_items {
                    let selected = self
                        .multi_select_values
                        .get(&field.name)
                        .map_or(0, |v| v.len());
                    if selected > max {
                        self.validation_errors.push(FieldValidationError {
                            field_name: field.name.clone(),
                            message: format!("At most {} selection(s) allowed", max),
                        });
                    }
                }
            }
        }
        self.validation_errors.is_empty()
    }

    pub fn field_error(&self, field_name: &str) -> Option<&str> {
        self.validation_errors
            .iter()
            .find(|e| e.field_name == field_name)
            .map(|e| e.message.as_str())
    }

    /// Build submission payload.
    pub fn submission_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
        let mut data = std::collections::HashMap::new();
        for field in &self.fields {
            match &field.field_type {
                ElicitFieldType::Boolean => {
                    let val = self
                        .form_values
                        .get(&field.name)
                        .map_or(false, |v| v == "true");
                    data.insert(field.name.clone(), serde_json::Value::Bool(val));
                }
                ElicitFieldType::Number => {
                    if let Some(val) = self.form_values.get(&field.name) {
                        if let Ok(n) = val.parse::<f64>() {
                            data.insert(
                                field.name.clone(),
                                serde_json::json!(n),
                            );
                        }
                    }
                }
                ElicitFieldType::MultiSelectEnum(_) => {
                    let selected = self
                        .multi_select_values
                        .get(&field.name)
                        .cloned()
                        .unwrap_or_default();
                    data.insert(
                        field.name.clone(),
                        serde_json::json!(selected),
                    );
                }
                _ => {
                    if let Some(val) = self.form_values.get(&field.name) {
                        data.insert(
                            field.name.clone(),
                            serde_json::Value::String(val.clone()),
                        );
                    }
                }
            }
        }
        data
    }
}

/// Widget for the elicitation dialog.
pub struct ElicitationDialogWidget<'a> {
    pub state: &'a ElicitationDialogState,
    pub theme: &'a Theme,
}

impl<'a> ElicitationDialogWidget<'a> {
    pub fn new(state: &'a ElicitationDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn render_field(
        field: &ElicitSchemaField,
        value: Option<&str>,
        multi_values: Option<&Vec<String>>,
        is_focused: bool,
        error: Option<&str>,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Field label
        let label_style = if is_focused {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let mut label_spans = vec![Span::styled(field.label.clone(), label_style)];
        if field.is_required {
            label_spans.push(Span::styled(" *", Style::default().fg(Color::Red)));
        }
        lines.push(Line::from(label_spans));

        // Description
        if let Some(ref desc) = field.description {
            lines.push(Line::from(Span::styled(
                desc.clone(),
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Value display
        match &field.field_type {
            ElicitFieldType::Boolean => {
                let checked = value.map_or(false, |v| v == "true");
                let indicator = if checked { "[x]" } else { "[ ]" };
                lines.push(Line::from(Span::raw(indicator.to_string())));
            }
            ElicitFieldType::Enum(opts) => {
                for opt in opts {
                    let selected = value.map_or(false, |v| v == opt);
                    let prefix = if selected { "● " } else { "○ " };
                    let style = if selected {
                        Style::default().fg(theme.primary)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", prefix, opt),
                        style,
                    )));
                }
            }
            ElicitFieldType::MultiSelectEnum(opts) => {
                let selected_set = multi_values.map_or(Vec::new(), |v| v.clone());
                for opt in opts {
                    let checked = selected_set.contains(opt);
                    let prefix = if checked { "[x] " } else { "[ ] " };
                    let style = if checked {
                        Style::default().fg(theme.primary)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", prefix, opt),
                        style,
                    )));
                }
            }
            _ => {
                // Text/Number/Date input
                let display = value.unwrap_or("");
                let input_style = if is_focused {
                    Style::default().fg(theme.primary)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                lines.push(Line::from(Span::styled(
                    if display.is_empty() {
                        "(empty)".to_string()
                    } else {
                        display.to_string()
                    },
                    input_style,
                )));
            }
        }

        // Error
        if let Some(err) = error {
            lines.push(Line::from(Span::styled(
                err.to_string(),
                Style::default().fg(Color::Red),
            )));
        }

        lines.push(Line::from(""));
        lines
    }
}

impl<'a> Widget for ElicitationDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!("{} needs your input", self.state.server_name);
        let dialog = DialogWidget::new(&title, self.theme).size(70, 24);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Message
                Constraint::Min(1),    // Fields
                Constraint::Length(2), // Buttons
            ])
            .split(inner);

        // Message
        Paragraph::new(self.state.message.as_str())
            .wrap(Wrap { trim: false })
            .render(chunks[0], buf);

        // Fields
        let mut all_lines: Vec<Line> = Vec::new();
        for (i, field) in self.state.fields.iter().enumerate() {
            let is_focused = self.state.current_field_index == Some(i);
            let value = self.state.form_values.get(&field.name).map(|s| s.as_str());
            let multi_values = self.state.multi_select_values.get(&field.name);
            let error = self.state.field_error(&field.name);
            let field_lines =
                Self::render_field(field, value, multi_values, is_focused, error, self.theme);
            all_lines.extend(field_lines);
        }
        Paragraph::new(all_lines)
            .wrap(Wrap { trim: false })
            .render(chunks[1], buf);

        // Buttons
        let submit_style = match self.state.focused_button {
            Some(ElicitButton::Submit) => Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::DarkGray),
        };
        let cancel_style = match self.state.focused_button {
            Some(ElicitButton::Cancel) => Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::DarkGray),
        };
        let submit_prefix = if self.state.focused_button == Some(ElicitButton::Submit) {
            "❯ "
        } else {
            "  "
        };
        let cancel_prefix = if self.state.focused_button == Some(ElicitButton::Cancel) {
            "❯ "
        } else {
            "  "
        };
        let btn_line = Line::from(vec![
            Span::styled(format!("{}Submit", submit_prefix), submit_style),
            Span::raw("    "),
            Span::styled(format!("{}Cancel", cancel_prefix), cancel_style),
        ]);
        Paragraph::new(btn_line).render(chunks[2], buf);
    }
}

// ===================================================================
// OAuthBrowserDialog — from ElicitationDialog (openUrl sub-component)
// ===================================================================

/// Phase of the OAuth browser dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthPhase {
    Prompt,
    Waiting,
}

/// Buttons for OAuth browser dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthButton {
    Accept,
    Decline,
    Open,
    Action,
    Cancel,
}

/// State for the OAuth browser redirect dialog.
#[derive(Debug, Clone)]
pub struct OAuthBrowserDialogState {
    pub server_name: String,
    pub url: String,
    pub message: String,
    pub phase: OAuthPhase,
    pub focused_button: OAuthButton,
    pub show_cancel: bool,
    pub action_label: String,
    pub completed: bool,
}

impl OAuthBrowserDialogState {
    pub fn new(
        server_name: impl Into<String>,
        url: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            url: url.into(),
            message: message.into(),
            phase: OAuthPhase::Prompt,
            focused_button: OAuthButton::Accept,
            show_cancel: false,
            action_label: "Continue without waiting".to_string(),
            completed: false,
        }
    }

    pub fn domain(&self) -> String {
        // Extract domain from URL without external crate
        if let Some(start) = self.url.find("://") {
            let after_scheme = &self.url[start + 3..];
            let end = after_scheme.find('/').unwrap_or(after_scheme.len());
            let host_port = &after_scheme[..end];
            let host = host_port.split(':').next().unwrap_or(host_port);
            host.to_string()
        } else {
            self.url.clone()
        }
    }

    pub fn url_before_domain(&self) -> String {
        let domain = self.domain();
        if let Some(idx) = self.url.find(&domain) {
            self.url[..idx].to_string()
        } else {
            String::new()
        }
    }

    pub fn url_after_domain(&self) -> String {
        let domain = self.domain();
        if let Some(idx) = self.url.find(&domain) {
            self.url[idx + domain.len()..].to_string()
        } else {
            String::new()
        }
    }

    pub fn accept(&mut self) {
        self.phase = OAuthPhase::Waiting;
        self.focused_button = OAuthButton::Open;
    }

    pub fn toggle_prompt_button(&mut self) {
        self.focused_button = match self.focused_button {
            OAuthButton::Accept => OAuthButton::Decline,
            OAuthButton::Decline => OAuthButton::Accept,
            other => other,
        };
    }

    pub fn toggle_waiting_button(&mut self, forward: bool) {
        let buttons: Vec<OAuthButton> = if self.show_cancel {
            vec![OAuthButton::Open, OAuthButton::Action, OAuthButton::Cancel]
        } else {
            vec![OAuthButton::Open, OAuthButton::Action]
        };
        let idx = buttons.iter().position(|b| *b == self.focused_button).unwrap_or(0);
        let delta: isize = if forward { 1 } else { -1 };
        let new_idx = ((idx as isize + delta).rem_euclid(buttons.len() as isize)) as usize;
        self.focused_button = buttons[new_idx];
    }

    pub fn check_auto_dismiss(&self) -> bool {
        self.phase == OAuthPhase::Waiting && self.completed
    }
}

/// Widget for the OAuth browser dialog.
pub struct OAuthBrowserDialogWidget<'a> {
    pub state: &'a OAuthBrowserDialogState,
    pub theme: &'a Theme,
}

impl<'a> OAuthBrowserDialogWidget<'a> {
    pub fn new(state: &'a OAuthBrowserDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for OAuthBrowserDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.state.phase {
            OAuthPhase::Prompt => {
                format!(
                    "MCP server \u{201c}{}\u{201d} wants to open a URL",
                    self.state.server_name
                )
            }
            OAuthPhase::Waiting => {
                format!(
                    "MCP server \u{201c}{}\u{201d} \u{2014} waiting for completion",
                    self.state.server_name
                )
            }
        };

        let dialog = DialogWidget::new(&title, self.theme).size(70, 14);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Message
                Constraint::Length(2), // URL display
                Constraint::Length(1), // Waiting message (if applicable)
                Constraint::Length(2), // Buttons
            ])
            .split(inner);

        // Message
        Paragraph::new(self.state.message.as_str())
            .wrap(Wrap { trim: false })
            .render(chunks[0], buf);

        // URL with highlighted domain
        let domain = self.state.domain();
        let before = self.state.url_before_domain();
        let after = self.state.url_after_domain();
        let url_line = Line::from(vec![
            Span::raw(before),
            Span::styled(domain, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(after),
        ]);
        Paragraph::new(url_line).render(chunks[1], buf);

        // Waiting message
        if self.state.phase == OAuthPhase::Waiting {
            Paragraph::new("Waiting for the server to confirm completion…")
                .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
                .render(chunks[2], buf);
        }

        // Buttons
        match self.state.phase {
            OAuthPhase::Prompt => {
                let accept_style = if self.state.focused_button == OAuthButton::Accept {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let decline_style = if self.state.focused_button == OAuthButton::Decline {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let accept_ptr = if self.state.focused_button == OAuthButton::Accept { "❯" } else { " " };
                let decline_ptr = if self.state.focused_button == OAuthButton::Decline { "❯" } else { " " };
                let btn_line = Line::from(vec![
                    Span::styled(format!("{} Accept  ", accept_ptr), accept_style),
                    Span::styled(format!("{} Decline", decline_ptr), decline_style),
                ]);
                Paragraph::new(btn_line).render(chunks[3], buf);
            }
            OAuthPhase::Waiting => {
                let mut spans = Vec::new();
                let buttons: Vec<(OAuthButton, &str, Color)> = if self.state.show_cancel {
                    vec![
                        (OAuthButton::Open, "Reopen URL", Color::Green),
                        (OAuthButton::Action, &self.state.action_label, Color::Green),
                        (OAuthButton::Cancel, "Cancel", Color::Red),
                    ]
                } else {
                    vec![
                        (OAuthButton::Open, "Reopen URL", Color::Green),
                        (OAuthButton::Action, &self.state.action_label, Color::Green),
                    ]
                };
                for (btn, label, color) in &buttons {
                    let is_focused = self.state.focused_button == *btn;
                    let ptr = if is_focused { "❯" } else { " " };
                    let style = if is_focused {
                        Style::default().fg(*color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    spans.push(Span::styled(format!("{} {}  ", ptr, label), style));
                }
                let btn_line = Line::from(spans);
                Paragraph::new(btn_line).render(chunks[3], buf);
            }
        }
    }
}

// ===================================================================
// MCP UI surfaces
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct MCPReconnect {
    pub server_name: String,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CapabilitiesSection {
    pub tools: Vec<String>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MCPToolListView {
    pub server: String,
    pub tools: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ElicitationDialog {
    pub server: String,
    pub prompt: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct MCPSettings {
    pub default_scope: String,
    pub auto_approve: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MCPAgentServerMenu {
    pub servers: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MCPStdioServerMenu {
    pub servers: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MCPToolDetailView {
    pub tool_name: String,
    pub description: String,
    pub schema: String,
}

#[derive(Debug, Clone, Default)]
pub struct MCPRemoteServerMenu {
    pub servers: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MCPListPanel {
    pub servers: Vec<String>,
    pub selected: usize,
    pub filter: String,
}
