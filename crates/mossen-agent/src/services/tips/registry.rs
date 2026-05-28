//! Tip registry - defines all available tips and their eligibility conditions

use std::collections::HashMap;

/// A tip that can be shown to the user
#[derive(Debug, Clone)]
pub struct Tip {
    pub id: &'static str,
    pub message: String,
    pub category: TipCategory,
    pub priority: u32,
    pub min_sessions_between: u32,
}

/// Tip categories
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TipCategory {
    Feature,
    Workflow,
    Performance,
    Security,
    Plugin,
    Shortcut,
    Model,
    Integration,
}

/// Context for evaluating tip eligibility
pub struct TipContext {
    pub sessions_since_last_tip: u32,
    pub is_hosted_subscriber: bool,
    pub is_first_party_customer: bool,
    pub has_github_workflow: bool,
    pub platform: String,
    pub is_supported_terminal: bool,
    pub concurrent_sessions: u32,
    pub worktree_count: u32,
    pub model_name: String,
    pub is_custom_backend: bool,
    pub has_plugins: bool,
}

/// Evaluate which tips are eligible given the current context
pub fn get_eligible_tips(context: &TipContext) -> Vec<Tip> {
    let all_tips = get_all_tips(context);
    all_tips
        .into_iter()
        .filter(|tip| context.sessions_since_last_tip >= tip.min_sessions_between)
        .collect()
}

/// Get all registered tips for the given context
fn get_all_tips(ctx: &TipContext) -> Vec<Tip> {
    let mut tips = Vec::new();

    // Feature tips
    if ctx.is_supported_terminal {
        tips.push(Tip {
            id: "vim_mode",
            message: "You can enable vim keybindings in settings for faster editing.".to_string(),
            category: TipCategory::Shortcut,
            priority: 3,
            min_sessions_between: 20,
        });
    }

    if ctx.concurrent_sessions > 1 {
        tips.push(Tip {
            id: "concurrent_sessions",
            message: format!(
                "You have {} concurrent sessions. Use /sessions to manage them.",
                ctx.concurrent_sessions
            ),
            category: TipCategory::Workflow,
            priority: 5,
            min_sessions_between: 10,
        });
    }

    if ctx.worktree_count > 1 {
        tips.push(Tip {
            id: "worktrees",
            message:
                "Multiple git worktrees detected. Each session works in its own worktree context."
                    .to_string(),
            category: TipCategory::Workflow,
            priority: 4,
            min_sessions_between: 15,
        });
    }

    if !ctx.has_plugins && !ctx.is_custom_backend {
        tips.push(Tip {
            id: "plugins",
            message: "Extend capabilities with plugins. Run /plugins to browse available plugins."
                .to_string(),
            category: TipCategory::Plugin,
            priority: 6,
            min_sessions_between: 30,
        });
    }

    if ctx.is_hosted_subscriber {
        tips.push(Tip {
            id: "model_selection",
            message: "You can switch models mid-conversation with /model for different tasks."
                .to_string(),
            category: TipCategory::Model,
            priority: 4,
            min_sessions_between: 25,
        });
    }

    tips.push(Tip {
        id: "keyboard_shortcuts",
        message: "Press Escape to interrupt, Ctrl+C to cancel. Use up/down arrows for history."
            .to_string(),
        category: TipCategory::Shortcut,
        priority: 2,
        min_sessions_between: 50,
    });

    tips.push(Tip {
        id: "compact",
        message: "Long conversations can be compacted with /compact to reduce token usage while preserving context.".to_string(),
        category: TipCategory::Performance,
        priority: 5,
        min_sessions_between: 20,
    });

    if ctx.has_github_workflow {
        tips.push(Tip {
            id: "github_actions",
            message: "GitHub Actions integration available. Run CI workflows directly from the conversation.".to_string(),
            category: TipCategory::Integration,
            priority: 4,
            min_sessions_between: 30,
        });
    }

    tips
}

/// Select the best tip to show from eligible tips
pub fn select_tip(eligible: &[Tip]) -> Option<&Tip> {
    eligible.iter().max_by_key(|t| t.priority)
}

/// Get relevant tips for the current context (async version for scheduler).
pub async fn get_relevant_tips(_context: Option<&TipContext>) -> Vec<Tip> {
    // In production, this would evaluate the context and return eligible tips.
    // For now, return a basic set of always-relevant tips filtered by cooldown.
    let ctx = TipContext {
        sessions_since_last_tip: 5,
        is_hosted_subscriber: false,
        is_first_party_customer: false,
        has_github_workflow: false,
        platform: std::env::consts::OS.to_string(),
        is_supported_terminal: true,
        concurrent_sessions: 1,
        worktree_count: 1,
        model_name: String::new(),
        is_custom_backend: false,
        has_plugins: false,
    };
    get_eligible_tips(&ctx)
}
