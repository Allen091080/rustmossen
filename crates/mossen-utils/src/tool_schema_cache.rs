//! # tool_schema_cache — 工具 Schema 缓存
//!
//! 对应 TypeScript `utils/toolSchemaCache.ts`。
//! 叶子模块，auth.ts 可在不导入 api.ts 的情况下清除缓存。

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// 缓存的 Schema 条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSchema {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
}

static TOOL_SCHEMA_CACHE: Mutex<Option<HashMap<String, CachedSchema>>> = Mutex::new(None);

/// 获取工具 Schema 缓存引用。
pub fn get_tool_schema_cache() -> HashMap<String, CachedSchema> {
    let guard = TOOL_SCHEMA_CACHE.lock().unwrap();
    guard.clone().unwrap_or_default()
}

/// 向缓存中插入一个 schema。
pub fn insert_tool_schema(name: &str, schema: CachedSchema) {
    let mut guard = TOOL_SCHEMA_CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    cache.insert(name.to_string(), schema);
}

/// 清除工具 Schema 缓存。
pub fn clear_tool_schema_cache() {
    let mut guard = TOOL_SCHEMA_CACHE.lock().unwrap();
    if let Some(cache) = guard.as_mut() {
        cache.clear();
    }
}
