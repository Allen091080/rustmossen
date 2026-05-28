//! # auth_portable — 认证工具
//!
//! 对应 TypeScript `utils/authPortable.ts`。
//! macOS Keychain 集成和 API 密钥规范化。

/// 规范化 API 密钥用于配置存储。
pub fn normalize_api_key_for_config(api_key: &str) -> String {
    api_key
        .chars()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// 在 macOS 下尝试从 keychain 中删除 API key，失败抛出错误。
///
/// 对应 TS `maybeRemoveApiKeyFromMacOSKeychainThrows`。非 darwin 平台为 no-op。
pub async fn maybe_remove_api_key_from_macos_keychain_throws() -> anyhow::Result<()> {
    if std::env::consts::OS != "macos" {
        return Ok(());
    }
    // Storage service name mirrors `getMacOsKeychainStorageServiceName`. We
    // import it indirectly via env override for testability, falling back to
    // the canonical "mossen-cli" name used by the TS implementation.
    let service =
        std::env::var("MOSSEN_KEYCHAIN_SERVICE").unwrap_or_else(|_| "mossen-cli".to_string());
    let user = std::env::var("USER").unwrap_or_default();
    let output = std::process::Command::new("security")
        .arg("delete-generic-password")
        .arg("-a")
        .arg(&user)
        .arg("-s")
        .arg(&service)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("Failed to delete keychain entry");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_api_key() {
        let key = "abcdefghijklmnopqrst";
        let result = normalize_api_key_for_config(key);
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn test_normalize_short_key() {
        let key = "abc";
        let result = normalize_api_key_for_config(key);
        assert_eq!(result, "abc");
    }
}
