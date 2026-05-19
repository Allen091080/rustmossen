//! # connector_text — 连接器文本类型
//!
//! 对应 TypeScript `types/connectorText.ts`。

use serde::{Deserialize, Serialize};

/// 连接器文本块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorTextBlock {
    /// 连接器文本内容。
    pub connector_text: String,
}

/// 连接器文本增量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorTextDelta {
    /// 连接器文本增量内容。
    pub connector_text: String,
}

/// 连接器文本联合类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConnectorText {
    /// 完整文本块。
    #[serde(rename = "connector_text")]
    Block(ConnectorTextBlock),
    /// 增量文本。
    #[serde(rename = "connector_text_delta")]
    Delta(ConnectorTextDelta),
}

/// 检查值是否为连接器文本块。
pub fn is_connector_text_block(value: &serde_json::Value) -> bool {
    value.get("type").and_then(|v| v.as_str()) == Some("connector_text")
        && value
            .get("connector_text")
            .and_then(|v| v.as_str())
            .is_some()
}
