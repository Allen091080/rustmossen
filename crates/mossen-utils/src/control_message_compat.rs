//! # control_message_compat — 控制消息键名标准化
//!
//! 对应 TypeScript `utils/controlMessageCompat.ts`。

use serde_json::Value;

/// 标准化 camelCase `requestId` → snake_case `request_id`。
///
/// 旧版 iOS app 由于缺少 Swift CodingKeys 映射发送 `requestId`。
/// 如果同时存在 `request_id` 和 `requestId`，snake_case 优先。
/// 原地修改对象。
pub fn normalize_control_message_keys(obj: &mut Value) {
    if let Value::Object(ref mut map) = obj {
        // 顶层 requestId → request_id
        if map.contains_key("requestId") && !map.contains_key("request_id") {
            if let Some(val) = map.remove("requestId") {
                map.insert("request_id".to_string(), val);
            }
        }
        // 嵌套 response 中的 requestId → request_id
        if let Some(Value::Object(ref mut response)) = map.get_mut("response") {
            if response.contains_key("requestId") && !response.contains_key("request_id") {
                if let Some(val) = response.remove("requestId") {
                    response.insert("request_id".to_string(), val);
                }
            }
        }
    }
}
