//! 文件操作分析工具。
//!
//! 翻译自 `utils/fileOperationAnalytics.ts`。

use sha2::{Digest, Sha256};

/// 记录分析事件（桩函数，由上层模块注入实际实现）。
fn log_event(event_name: &str, metadata: serde_json::Value) {
    tracing::info!(event = event_name, ?metadata, "file_operation_analytics");
}

/// 分析元数据类型别名（隐私安全字符串）。
pub type AnalyticsMetadata = String;

/// 创建文件路径的截断 SHA256 哈希（16 字符）。
/// 用于保护隐私的文件操作分析。
fn hash_file_path(file_path: &str) -> AnalyticsMetadata {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)[..16].to_string()
}

/// 创建文件内容的完整 SHA256 哈希（64 字符）。
/// 用于去重和变更检测分析。
fn hash_file_content(content: &str) -> AnalyticsMetadata {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// 最大内容哈希大小（100KB）。
/// 防止哈希大文件（如 base64 编码的图片）时内存耗尽。
const MAX_CONTENT_HASH_SIZE: usize = 100 * 1024;

/// 文件操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperation {
    Read,
    Write,
    Edit,
}

impl FileOperation {
    /// 转换为字符串表示。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Edit => "edit",
        }
    }
}

/// 文件操作工具类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperationTool {
    FileReadTool,
    FileWriteTool,
    FileEditTool,
}

impl FileOperationTool {
    /// 转换为字符串表示。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileReadTool => "FileReadTool",
            Self::FileWriteTool => "FileWriteTool",
            Self::FileEditTool => "FileEditTool",
        }
    }
}

/// 文件操作写入类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWriteType {
    Create,
    Update,
}

impl FileWriteType {
    /// 转换为字符串表示。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
        }
    }
}

/// 文件操作日志参数。
pub struct LogFileOperationParams<'a> {
    pub operation: FileOperation,
    pub tool: FileOperationTool,
    pub file_path: &'a str,
    pub content: Option<&'a str>,
    pub write_type: Option<FileWriteType>,
}

/// 记录文件操作分析数据到 Statsig。
pub fn log_file_operation(params: LogFileOperationParams<'_>) {
    let mut metadata = serde_json::Map::new();

    metadata.insert(
        "operation".to_string(),
        serde_json::Value::String(params.operation.as_str().to_string()),
    );
    metadata.insert(
        "tool".to_string(),
        serde_json::Value::String(params.tool.as_str().to_string()),
    );
    metadata.insert(
        "filePathHash".to_string(),
        serde_json::Value::String(hash_file_path(params.file_path)),
    );

    // 仅在内容存在且低于大小限制时哈希内容
    // 防止哈希大文件（如 base64 编码图片）时内存耗尽
    if let Some(content) = params.content {
        if content.len() <= MAX_CONTENT_HASH_SIZE {
            metadata.insert(
                "contentHash".to_string(),
                serde_json::Value::String(hash_file_content(content)),
            );
        }
    }

    if let Some(write_type) = params.write_type {
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String(write_type.as_str().to_string()),
        );
    }

    log_event("tengu_file_operation", serde_json::Value::Object(metadata));
}
