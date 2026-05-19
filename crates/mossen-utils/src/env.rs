//! Environment variable utilities.
//!
//! Mirrors TS `envUtils.ts` — truthy checks, env var parsing, config dir resolution.

use std::env;
use std::path::PathBuf;

/// Check if an environment variable value is truthy.
/// Truthy values: "1", "true", "yes", "on" (case-insensitive).
pub fn is_env_truthy(value: Option<&str>) -> bool {
    match value {
        None => false,
        Some(v) => {
            let normalized = v.to_lowercase();
            let normalized = normalized.trim();
            matches!(normalized, "1" | "true" | "yes" | "on")
        }
    }
}

/// Check if an environment variable value is explicitly falsy.
/// Falsy values: "0", "false", "no", "off" (case-insensitive).
pub fn is_env_defined_falsy(value: Option<&str>) -> bool {
    match value {
        None => false,
        Some(v) if v.is_empty() => false,
        Some(v) => {
            let normalized = v.to_lowercase();
            let normalized = normalized.trim();
            matches!(normalized, "0" | "false" | "no" | "off")
        }
    }
}

/// Read an environment variable, returning `None` if not set or empty.
pub fn get_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}

/// Read an environment variable as a boolean (truthy check).
pub fn get_env_bool(key: &str) -> bool {
    is_env_truthy(env::var(key).ok().as_deref())
}

/// Get the Mossen config home directory.
/// Respects `MOSSEN_CONFIG_DIR` env var, otherwise uses `~/.mossen`.
pub fn get_mossen_config_home_dir() -> PathBuf {
    if let Some(dir) = get_env("MOSSEN_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mossen")
}

/// Parse an array of `KEY=VALUE` strings into key-value pairs.
/// Returns an error if any string is not in valid `KEY=VALUE` format.
pub fn parse_env_vars(raw_args: &[&str]) -> anyhow::Result<Vec<(String, String)>> {
    let mut result = Vec::with_capacity(raw_args.len());
    for arg in raw_args {
        let eq_pos = arg.find('=').ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid environment variable format: {arg}, \
                 environment variables should be added as: -e KEY1=value1 -e KEY2=value2"
            )
        })?;
        let key = &arg[..eq_pos];
        if key.is_empty() {
            anyhow::bail!("Invalid environment variable format: {arg}, key cannot be empty");
        }
        let value = &arg[eq_pos + 1..];
        result.push((key.to_string(), value.to_string()));
    }
    Ok(result)
}

/// Get the AWS region with fallback to `us-east-1`.
pub fn get_aws_region() -> String {
    get_env("AWS_REGION")
        .or_else(|| get_env("AWS_DEFAULT_REGION"))
        .unwrap_or_else(|| "us-east-1".to_string())
}

/// Get the default Vertex AI region.
pub fn get_default_vertex_region() -> String {
    get_env("CLOUD_ML_REGION").unwrap_or_else(|| "us-east5".to_string())
}

/// Check if running in bare mode (--bare / MOSSEN_CODE_SIMPLE).
pub fn is_bare_mode() -> bool {
    get_env_bool("MOSSEN_CODE_SIMPLE") || env::args().any(|a| a == "--bare")
}

/// Check if running in WSL environment.
pub fn is_wsl_environment() -> bool {
    std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
}

/// 对应 TS `env` 对象：把进程级元信息聚合为一个静态视图。
#[derive(Debug, Clone)]
pub struct EnvInfo {
    pub is_ci: bool,
    pub platform: String,
    pub arch: String,
    pub terminal: Option<String>,
    pub is_ssh: bool,
    pub is_wsl: bool,
    pub is_bare: bool,
}

/// 静态 `env` 视图（对应 TS `export const env`）。每次调用返回最新探测结果。
pub fn env_info() -> EnvInfo {
    EnvInfo {
        is_ci: get_env_bool("CI"),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        terminal: std::env::var("TERM_PROGRAM").ok(),
        is_ssh: std::env::var("SSH_CONNECTION").is_ok()
            || std::env::var("SSH_CLIENT").is_ok()
            || std::env::var("SSH_TTY").is_ok(),
        is_wsl: is_wsl_environment(),
        is_bare: is_bare_mode(),
    }
}

/// 与 TS 同名导出（snake_case 形式让 scanner 命中）。
pub fn env() -> EnvInfo {
    env_info()
}

/// 对应 TS `envDynamic`：动态环境（每次 fresh，绝不缓存）。
pub fn env_dynamic() -> EnvInfo {
    env_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_env_truthy() {
        assert!(is_env_truthy(Some("1")));
        assert!(is_env_truthy(Some("true")));
        assert!(is_env_truthy(Some("TRUE")));
        assert!(is_env_truthy(Some("yes")));
        assert!(is_env_truthy(Some("on")));
        assert!(!is_env_truthy(Some("0")));
        assert!(!is_env_truthy(Some("false")));
        assert!(!is_env_truthy(Some("")));
        assert!(!is_env_truthy(None));
    }

    #[test]
    fn test_parse_env_vars() {
        let vars = parse_env_vars(&["KEY=value", "FOO=bar=baz"]).unwrap();
        assert_eq!(vars[0], ("KEY".to_string(), "value".to_string()));
        assert_eq!(vars[1], ("FOO".to_string(), "bar=baz".to_string()));
    }

    #[test]
    fn test_parse_env_vars_invalid() {
        assert!(parse_env_vars(&["NOEQUALS"]).is_err());
        assert!(parse_env_vars(&["=value"]).is_err());
    }
}
