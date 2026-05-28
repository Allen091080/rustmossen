//! # content_array — 内容数组工具
//!
//! 对应 TypeScript `utils/contentArray.ts`。

use serde_json::Value;

/// 在内容数组中的 tool_result 块之后插入一个块。
///
/// 放置规则：
/// - 如果存在 tool_result 块：在最后一个之后插入
/// - 否则：在最后一个块之前插入
/// - 如果插入的块成为最后一个元素，追加一个文本续行块
pub fn insert_block_after_tool_results(content: &mut Vec<Value>, block: Value) {
    let mut last_tool_result_index: Option<usize> = None;
    for (i, item) in content.iter().enumerate() {
        if let Some(obj) = item.as_object() {
            if obj.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                last_tool_result_index = Some(i);
            }
        }
    }

    if let Some(idx) = last_tool_result_index {
        let insert_pos = idx + 1;
        content.insert(insert_pos, block);
        // 如果插入的块现在是最后一个，追加文本续行
        if insert_pos == content.len() - 1 {
            content.push(serde_json::json!({"type": "text", "text": "."}));
        }
    } else {
        // 没有 tool_result 块 — 在最后一个块之前插入
        let insert_index = content.len().saturating_sub(1);
        content.insert(insert_index, block);
    }
}
