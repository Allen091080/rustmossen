/// Away summary generation - produces a short session recap for the "while you were away" card.

const RECENT_MESSAGE_WINDOW: usize = 30;

/// Trait for external dependencies
#[async_trait::async_trait]
pub trait AwaySummaryContext: Send + Sync {
    async fn get_session_memory_content(&self) -> Option<String>;
    async fn query_model_without_streaming(
        &self,
        messages: Vec<serde_json::Value>,
        system_prompt: &str,
        signal: &tokio_util::sync::CancellationToken,
    ) -> Result<Option<String>, AwaySummaryError>;
}

#[derive(Debug)]
pub enum AwaySummaryError {
    Aborted,
    ApiError(String),
    Other(String),
}

impl std::fmt::Display for AwaySummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aborted => write!(f, "aborted"),
            Self::ApiError(msg) => write!(f, "API error: {}", msg),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

fn build_away_summary_prompt(memory: Option<&str>) -> String {
    let memory_block = match memory {
        Some(m) => format!("Session memory (broader context):\n{}\n\n", m),
        None => String::new(),
    };
    format!(
        "{}The user stepped away and is coming back. Write exactly 1-3 short sentences. \
         Start by stating the high-level task — what they are building or debugging, not \
         implementation details. Next: the concrete next step. Skip status reports and commit recaps.",
        memory_block
    )
}

/// Generates a short session recap for the "while you were away" card.
/// Returns None on abort, empty transcript, or error.
pub async fn generate_away_summary(
    ctx: &dyn AwaySummaryContext,
    messages: &[serde_json::Value],
    signal: &tokio_util::sync::CancellationToken,
) -> Option<String> {
    if messages.is_empty() {
        return None;
    }

    if signal.is_cancelled() {
        return None;
    }

    let memory = ctx.get_session_memory_content().await;
    let start = if messages.len() > RECENT_MESSAGE_WINDOW {
        messages.len() - RECENT_MESSAGE_WINDOW
    } else {
        0
    };
    let mut recent: Vec<serde_json::Value> = messages[start..].to_vec();

    let prompt = build_away_summary_prompt(memory.as_deref());
    recent.push(serde_json::json!({
        "role": "user",
        "content": prompt,
    }));

    match ctx.query_model_without_streaming(recent, "", signal).await {
        Ok(Some(text)) => Some(text),
        Ok(None) => None,
        Err(AwaySummaryError::Aborted) => None,
        Err(e) => {
            tracing::debug!("[awaySummary] generation failed: {}", e);
            None
        }
    }
}
