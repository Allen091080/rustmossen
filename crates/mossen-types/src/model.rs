//! # model — 模型相关类型
//!
//! 定义模型提供商、模型信息等类型。
//! 对应 TypeScript 中 model 相关的类型和配置。

use serde::{Deserialize, Serialize};

/// 模型提供商。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    /// Mossen。
    Mossen,
    /// Bedrock。
    Bedrock,
    /// Vertex。
    Vertex,
    /// 自定义。
    Custom,
}

/// 模型能力。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// 是否支持思考/推理。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    /// 是否支持图像。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
    /// 是否支持工具使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use: Option<bool>,
    /// 最大上下文窗口（token 数）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<u64>,
    /// 最大输出 token 数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

/// 模型信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 模型 ID。
    pub id: String,
    /// 显示名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 营销名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketing_name: Option<String>,
    /// 提供商。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ModelProvider>,
    /// 能力。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
    /// 知识截止日期。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_cutoff: Option<String>,
}

/// 模型层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// Opus（最强）。
    Opus,
    /// Sonnet（均衡）。
    Sonnet,
    /// Haiku（快速）。
    Haiku,
}
