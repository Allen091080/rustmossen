//! # env_validation — 环境变量验证
//!
//! 对应 TypeScript `utils/envValidation.ts`。

/// 环境变量验证结果。
#[derive(Debug, Clone)]
pub struct EnvVarValidationResult {
    pub effective: i64,
    pub status: EnvVarStatus,
    pub message: Option<String>,
}

/// 验证状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvVarStatus {
    Valid,
    Capped,
    Invalid,
}

/// 验证有界整数环境变量。
///
/// 如果值无效或为空，返回 default_value。
/// 如果值超过上限，返回 upper_limit（capped）。
pub fn validate_bounded_int_env_var(
    name: &str,
    value: Option<&str>,
    default_value: i64,
    upper_limit: i64,
) -> EnvVarValidationResult {
    let Some(val_str) = value else {
        return EnvVarValidationResult {
            effective: default_value,
            status: EnvVarStatus::Valid,
            message: None,
        };
    };

    if val_str.is_empty() {
        return EnvVarValidationResult {
            effective: default_value,
            status: EnvVarStatus::Valid,
            message: None,
        };
    }

    match val_str.parse::<i64>() {
        Ok(parsed) if parsed > 0 => {
            if parsed > upper_limit {
                let msg = format!("Capped from {} to {}", parsed, upper_limit);
                tracing::debug!("{} {}", name, msg);
                EnvVarValidationResult {
                    effective: upper_limit,
                    status: EnvVarStatus::Capped,
                    message: Some(msg),
                }
            } else {
                EnvVarValidationResult {
                    effective: parsed,
                    status: EnvVarStatus::Valid,
                    message: None,
                }
            }
        }
        _ => {
            let msg = format!(
                "Invalid value \"{}\" (using default: {})",
                val_str, default_value
            );
            tracing::debug!("{} {}", name, msg);
            EnvVarValidationResult {
                effective: default_value,
                status: EnvVarStatus::Invalid,
                message: Some(msg),
            }
        }
    }
}
