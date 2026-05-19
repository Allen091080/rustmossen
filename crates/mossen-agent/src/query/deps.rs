/// Query I/O dependencies.
///
/// Passing a `deps` override into QueryParams lets tests inject fakes directly.

use std::future::Future;
use std::pin::Pin;

/// Abstract model call result type
pub type ModelCallResult = Result<Vec<serde_json::Value>, String>;

/// Query dependencies trait - injectable for testing
pub trait QueryDeps: Send + Sync {
    /// Call model with streaming
    fn call_model(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ModelCallResult> + Send + '_>>;

    /// Microcompact messages
    fn microcompact(
        &self,
        messages: &[serde_json::Value],
        max_tokens: u64,
    ) -> Vec<serde_json::Value>;

    /// Auto-compact if needed
    fn autocompact(
        &self,
        messages: &[serde_json::Value],
        context: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send + '_>>;

    /// Generate UUID
    fn uuid(&self) -> String;
}

/// Production implementation of query deps
pub struct ProductionDeps;

impl ProductionDeps {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProductionDeps {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryDeps for ProductionDeps {
    fn call_model(
        &self,
        _params: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ModelCallResult> + Send + '_>> {
        Box::pin(async move {
            // In production, this delegates to the actual API client
            // The real implementation would be injected via the context system
            Err("call_model: production implementation requires API client".to_string())
        })
    }

    fn microcompact(
        &self,
        messages: &[serde_json::Value],
        _max_tokens: u64,
    ) -> Vec<serde_json::Value> {
        // Default: return messages unchanged
        messages.to_vec()
    }

    fn autocompact(
        &self,
        messages: &[serde_json::Value],
        _context: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send + '_>> {
        let msgs = messages.to_vec();
        Box::pin(async move { Ok(msgs) })
    }

    fn uuid(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
