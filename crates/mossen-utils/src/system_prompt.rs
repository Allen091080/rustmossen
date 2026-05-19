use serde::{Deserialize, Serialize};

/// System prompt wrapper type (opaque branded array of strings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPrompt(Vec<String>);

impl SystemPrompt {
    pub fn new(items: Vec<String>) -> Self {
        Self(items)
    }

    pub fn items(&self) -> &[String] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Convert a list of strings into a SystemPrompt.
pub fn as_system_prompt(items: Vec<String>) -> SystemPrompt {
    SystemPrompt::new(items)
}

/// Agent definition for system prompt building.
pub struct AgentDefinition {
    pub agent_type: String,
    pub memory: Option<String>,
    pub is_built_in: bool,
    pub system_prompt: Option<String>,
}

/// Build the effective system prompt array.
///
/// Priority:
/// 0. Override system prompt (if set - REPLACES all other prompts)
/// 1. Coordinator system prompt (if coordinator mode is active)
/// 2. Agent system prompt (replaces or appends depending on mode)
/// 3. Custom system prompt
/// 4. Default system prompt
///
/// Plus appendSystemPrompt is always added at the end (except when override is set).
pub fn build_effective_system_prompt(
    main_thread_agent_definition: Option<&AgentDefinition>,
    custom_system_prompt: Option<&str>,
    default_system_prompt: &[String],
    append_system_prompt: Option<&str>,
    override_system_prompt: Option<&str>,
    coordinator_mode: bool,
    coordinator_prompt_fn: impl Fn() -> String,
    proactive_active: bool,
) -> SystemPrompt {
    // 0. Override
    if let Some(override_prompt) = override_system_prompt {
        return as_system_prompt(vec![override_prompt.to_string()]);
    }

    // 1. Coordinator mode
    if coordinator_mode && main_thread_agent_definition.is_none() {
        let mut items = vec![coordinator_prompt_fn()];
        if let Some(append) = append_system_prompt {
            items.push(append.to_string());
        }
        return as_system_prompt(items);
    }

    // 2. Agent system prompt
    let agent_system_prompt = main_thread_agent_definition
        .and_then(|def| def.system_prompt.clone());

    // In proactive mode, agent instructions are appended to the default prompt
    if let Some(ref agent_prompt) = agent_system_prompt {
        if proactive_active {
            let mut items: Vec<String> = default_system_prompt.to_vec();
            items.push(format!("\n# Custom Agent Instructions\n{}", agent_prompt));
            if let Some(append) = append_system_prompt {
                items.push(append.to_string());
            }
            return as_system_prompt(items);
        }
    }

    // Standard priority chain
    let base_items: Vec<String> = if let Some(agent_prompt) = agent_system_prompt {
        vec![agent_prompt]
    } else if let Some(custom) = custom_system_prompt {
        vec![custom.to_string()]
    } else {
        default_system_prompt.to_vec()
    };

    let mut items = base_items;
    if let Some(append) = append_system_prompt {
        items.push(append.to_string());
    }

    as_system_prompt(items)
}
