//! Query context — shared helpers for building the API cache-key prefix.
//!
//! Provides system prompt parts, user context, and system context for query() calls.

use std::collections::HashMap;

/// System prompt parts result.
pub struct SystemPromptParts {
    pub default_system_prompt: Vec<String>,
    pub user_context: HashMap<String, String>,
    pub system_context: HashMap<String, String>,
}

/// Parameters safe for caching (the cache-key prefix).
pub struct CacheSafeParams {
    pub system_prompt: String,
    pub user_context: HashMap<String, String>,
    pub system_context: HashMap<String, String>,
    pub fork_context_messages: Vec<serde_json::Value>,
}

/// Fetch the three context pieces that form the API cache-key prefix.
///
/// When custom_system_prompt is set, the default getSystemPrompt build and
/// getSystemContext are skipped.
pub async fn fetch_system_prompt_parts(
    custom_system_prompt: Option<&str>,
    get_system_prompt: impl std::future::Future<Output = Vec<String>>,
    get_user_context: impl std::future::Future<Output = HashMap<String, String>>,
    get_system_context: impl std::future::Future<Output = HashMap<String, String>>,
) -> SystemPromptParts {
    let (default_system_prompt, user_context, system_context) = if custom_system_prompt.is_some() {
        let user_ctx = get_user_context.await;
        (Vec::new(), user_ctx, HashMap::new())
    } else {
        let (sp, uc, sc) = tokio::join!(get_system_prompt, get_user_context, get_system_context);
        (sp, uc, sc)
    };

    SystemPromptParts {
        default_system_prompt,
        user_context,
        system_context,
    }
}

/// Build CacheSafeParams from raw inputs when getLastCacheSafeParams() is null.
///
/// Used by the SDK side_question handler on resume before a turn completes.
pub fn build_side_question_fallback_params(
    custom_system_prompt: Option<&str>,
    append_system_prompt: Option<&str>,
    default_system_prompt: &[String],
    user_context: HashMap<String, String>,
    system_context: HashMap<String, String>,
    messages: &[serde_json::Value],
) -> CacheSafeParams {
    let system_prompt_parts: Vec<&str> = if let Some(custom) = custom_system_prompt {
        vec![custom]
    } else {
        default_system_prompt.iter().map(|s| s.as_str()).collect()
    };

    let mut all_parts: Vec<&str> = system_prompt_parts;
    if let Some(append) = append_system_prompt {
        all_parts.push(append);
    }

    let system_prompt = all_parts.join("\n");

    // Strip in-progress assistant message (stop_reason === null)
    let fork_context_messages = if let Some(last) = messages.last() {
        if last.get("type").and_then(|v| v.as_str()) == Some("assistant") {
            if last
                .get("message")
                .and_then(|m| m.get("stop_reason"))
                .map(|v| v.is_null())
                .unwrap_or(false)
            {
                messages[..messages.len() - 1].to_vec()
            } else {
                messages.to_vec()
            }
        } else {
            messages.to_vec()
        }
    } else {
        messages.to_vec()
    };

    CacheSafeParams {
        system_prompt,
        user_context,
        system_context,
        fork_context_messages,
    }
}
