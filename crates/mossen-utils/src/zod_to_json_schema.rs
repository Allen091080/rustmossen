//! # zod_to_json_schema — Schema 转 JSON Schema
//!
//! 对应 TypeScript `utils/zodToJsonSchema.ts`。
//! 在 Rust 中使用 serde_json::Value 表示 JSON Schema。

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// JSON Schema 类型别名。
pub type JsonSchema7Type = Value;

static TOOL_SCHEMA_CACHE: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

/// 将 schema 定义转换为 JSON Schema 格式（带缓存）。
///
/// 使用名称作为缓存键，避免重复转换。
pub fn cached_json_schema(name: &str, schema: Value) -> Value {
    let mut guard = TOOL_SCHEMA_CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(cached) = cache.get(name) {
        return cached.clone();
    }
    cache.insert(name.to_string(), schema.clone());
    schema
}

/// 清除 schema 缓存。
pub fn clear_json_schema_cache() {
    let mut guard = TOOL_SCHEMA_CACHE.lock().unwrap();
    if let Some(cache) = guard.as_mut() {
        cache.clear();
    }
}

/// 对应 TS `zodToJsonSchema(schema)`：将 schema 转换为 JSON Schema 格式。
///
/// 在 Rust 端我们不依赖 Zod，调用方应传入已经构造好的 JSON Schema 值；
/// 该函数透传该值，并按结构体身份做一次缓存。
pub fn zod_to_json_schema(schema: Value) -> Value {
    // 用 schema 的 type 字段作为缓存键；缺省回退到字符串 hash。
    let key = schema
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("anon:{}", schema.to_string().len()));
    cached_json_schema(&key, schema)
}
