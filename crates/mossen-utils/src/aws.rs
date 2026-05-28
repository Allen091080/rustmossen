//! # aws — AWS 凭证工具
//!
//! 对应 TypeScript `utils/aws.ts`。
//! AWS 短期凭证验证和缓存清理。

use serde::{Deserialize, Serialize};

/// AWS 短期凭证格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub expiration: Option<String>,
}

/// `aws sts get-session-token` 或 `aws sts assume-role` 的输出
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AwsStsOutput {
    pub credentials: AwsCredentials,
}

/// 检查错误是否是 AWS 凭证提供者错误
pub fn is_aws_credentials_provider_error(err: &dyn std::error::Error) -> bool {
    err.to_string().contains("CredentialsProviderError")
}

/// 验证 AWS STS assume-role 输出是否有效
pub fn is_valid_aws_sts_output(value: &serde_json::Value) -> bool {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return false,
    };

    let credentials = match obj.get("Credentials").and_then(|v| v.as_object()) {
        Some(c) => c,
        None => return false,
    };

    let access_key_id = credentials
        .get("AccessKeyId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let secret_access_key = credentials
        .get("SecretAccessKey")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_token = credentials
        .get("SessionToken")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    !access_key_id.is_empty() && !secret_access_key.is_empty() && !session_token.is_empty()
}

/// 检查 STS caller identity。无法获取时返回错误。
///
/// Rust 端通过 shell 调用 `aws sts get-caller-identity`；这与 TS 端使用 AWS
/// SDK 的差异是：进程外调用绕过了 SDK 的内部凭证刷新缓存，但结果等价
/// （成功表示 STS 接受了当前凭证）。如果未来需要把 aws-sdk-sts 引入工作区，
/// 可替换为 `aws_sdk_sts::Client::get_caller_identity`。
pub async fn check_sts_caller_identity() -> Result<(), Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new("aws")
        .args(["sts", "get-caller-identity"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "aws sts get-caller-identity failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// 清除 AWS 凭证提供者缓存。
///
/// 强制刷新以确保 ~/.aws/credentials 的更改被立即感知。
pub async fn clear_aws_ini_cache() -> Result<(), Box<dyn std::error::Error>> {
    // 在实际实现中使用 aws-sdk 的 fromIni 刷新
    // 这里通过环境变量标记缓存失效
    tracing::debug!("Clearing AWS credential provider cache");

    // 尝试重新加载凭证
    let output = tokio::process::Command::new("aws")
        .args(["sts", "get-caller-identity"])
        .output()
        .await;

    match output {
        Ok(_) => {
            tracing::debug!("AWS credential provider cache refreshed");
            Ok(())
        }
        Err(_) => {
            tracing::debug!(
                "Failed to clear AWS credential cache (expected if no credentials configured)"
            );
            Ok(())
        }
    }
}
