//! Agent color manager — assigns and retrieves colors for subagent types.

use std::collections::HashMap;
use std::sync::Mutex;

/// Available agent color names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentColorName {
    Red,
    Blue,
    Green,
    Yellow,
    Purple,
    Orange,
    Pink,
    Cyan,
}

impl AgentColorName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Purple => "purple",
            Self::Orange => "orange",
            Self::Pink => "pink",
            Self::Cyan => "cyan",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "red" => Some(Self::Red),
            "blue" => Some(Self::Blue),
            "green" => Some(Self::Green),
            "yellow" => Some(Self::Yellow),
            "purple" => Some(Self::Purple),
            "orange" => Some(Self::Orange),
            "pink" => Some(Self::Pink),
            "cyan" => Some(Self::Cyan),
            _ => None,
        }
    }

    /// Get the theme color key for this agent color.
    pub fn theme_color_key(&self) -> &'static str {
        match self {
            Self::Red => "red_FOR_SUBAGENTS_ONLY",
            Self::Blue => "blue_FOR_SUBAGENTS_ONLY",
            Self::Green => "green_FOR_SUBAGENTS_ONLY",
            Self::Yellow => "yellow_FOR_SUBAGENTS_ONLY",
            Self::Purple => "purple_FOR_SUBAGENTS_ONLY",
            Self::Orange => "orange_FOR_SUBAGENTS_ONLY",
            Self::Pink => "pink_FOR_SUBAGENTS_ONLY",
            Self::Cyan => "cyan_FOR_SUBAGENTS_ONLY",
        }
    }
}

/// All available agent colors in order.
pub const AGENT_COLORS: &[AgentColorName] = &[
    AgentColorName::Red,
    AgentColorName::Blue,
    AgentColorName::Green,
    AgentColorName::Yellow,
    AgentColorName::Purple,
    AgentColorName::Orange,
    AgentColorName::Pink,
    AgentColorName::Cyan,
];

/// `agentColorManager.ts` `AGENT_COLOR_TO_THEME_COLOR` — fixed mapping from
/// each agent color name to its corresponding theme color key. The ordering
/// matches `AGENT_COLORS` above.
pub const AGENT_COLOR_TO_THEME_COLOR: &[(AgentColorName, &str)] = &[
    (AgentColorName::Red, "red_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Blue, "blue_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Green, "green_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Yellow, "yellow_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Purple, "purple_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Orange, "orange_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Pink, "pink_FOR_SUBAGENTS_ONLY"),
    (AgentColorName::Cyan, "cyan_FOR_SUBAGENTS_ONLY"),
];

/// Thread-safe agent color map.
static AGENT_COLOR_MAP: std::sync::LazyLock<Mutex<HashMap<String, AgentColorName>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get the assigned color for an agent type.
/// Returns None for "general-purpose" agents or unassigned types.
pub fn get_agent_color(agent_type: &str) -> Option<&'static str> {
    if agent_type == "general-purpose" {
        return None;
    }

    let map = AGENT_COLOR_MAP.lock().unwrap();
    map.get(agent_type).map(|c| c.theme_color_key())
}

/// Set or clear the color assignment for an agent type.
pub fn set_agent_color(agent_type: &str, color: Option<AgentColorName>) {
    let mut map = AGENT_COLOR_MAP.lock().unwrap();
    match color {
        Some(c) => {
            map.insert(agent_type.to_string(), c);
        }
        None => {
            map.remove(agent_type);
        }
    }
}

/// Assign the next available color to an agent type.
pub fn assign_next_color(agent_type: &str) -> AgentColorName {
    let mut map = AGENT_COLOR_MAP.lock().unwrap();
    let used_colors: std::collections::HashSet<AgentColorName> = map.values().copied().collect();

    let color = AGENT_COLORS
        .iter()
        .find(|c| !used_colors.contains(c))
        .copied()
        .unwrap_or(AGENT_COLORS[0]);

    map.insert(agent_type.to_string(), color);
    color
}
