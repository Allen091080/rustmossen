//! # ca_certs_config — CA 证书配置
//!
//! 对应 TypeScript `utils/caCertsConfig.ts`。
//! 从设置中填充 NODE_EXTRA_CA_CERTS。
//!
//! ## TS 行为
//!
//! `applyExtraCACertsFromConfig` 先看 `process.env.NODE_EXTRA_CA_CERTS`，已设则
//! 跳过；否则按优先级读：
//!   1. user settings (`~/.mossen/settings.json`) 的 `env.NODE_EXTRA_CA_CERTS`
//!   2. global config (`~/.mossen.json`) 的 `env.NODE_EXTRA_CA_CERTS`
//!
//! 找到后写到 `process.env.NODE_EXTRA_CA_CERTS`。Rust 端通过 sibling modules
//! [`crate::config::get_global_config`] 与 [`crate::settings::parse_settings_file`]
//! 实现，避免再读裸 JSON（保证字段重命名/路径迁移一致）。

use std::path::PathBuf;

use crate::config::get_global_config;
use crate::env::get_mossen_config_home_dir;
use crate::settings::parse_settings_file;

/// 应用 CA 证书配置到环境变量。
/// 在 CLI 初始化早期调用，在任何 TLS 连接建立之前。
pub fn apply_ca_certs_config() {
    apply_extra_ca_certs_from_config();
}

/// 对应 TS `applyExtraCACertsFromConfig`：在 init 早期把 settings/config 中的
/// `NODE_EXTRA_CA_CERTS` 提升到进程环境变量。
///
/// 读取优先级（与 TS 一致）：
/// 1. user settings (`~/.mossen/settings.json`) 的 `env.NODE_EXTRA_CA_CERTS`
/// 2. global config (`~/.mossen.json`) 的 `env.NODE_EXTRA_CA_CERTS`
///
/// 只读取用户级配置（不读项目级 `.mossen/settings.json` 或 `.mossen/settings.local.json`），
/// 防止恶意项目在 trust 对话框之前注入 CA 证书。
pub fn apply_extra_ca_certs_from_config() {
    if std::env::var("NODE_EXTRA_CA_CERTS").is_ok() {
        return;
    }
    if let Some(path) = get_extra_certs_path_from_config() {
        // SAFETY: at startup before TLS connections, single-threaded init.
        unsafe {
            std::env::set_var("NODE_EXTRA_CA_CERTS", &path);
        }
        tracing::debug!(
            target = "ca_certs",
            path = %path,
            "applied NODE_EXTRA_CA_CERTS from config"
        );
    }
}

/// 从 user settings + global config 中读取 `NODE_EXTRA_CA_CERTS`。
/// settings 优先于 config（与 TS `applyConfigEnvironmentVariables` 的优先级一致）。
fn get_extra_certs_path_from_config() -> Option<String> {
    // 1. user settings (~/.mossen/settings.json) — 取 env.NODE_EXTRA_CA_CERTS
    let user_settings_path: PathBuf = get_mossen_config_home_dir().join("settings.json");
    let (settings, _errs) = parse_settings_file(&user_settings_path);
    if let Some(s) = settings.as_ref() {
        if let Some(env) = s.env.as_ref() {
            if let Some(v) = env.get("NODE_EXTRA_CA_CERTS") {
                if !v.is_empty() {
                    return Some(v.clone());
                }
            }
        }
    }

    // 2. global config (~/.mossen.json) — 取 env.NODE_EXTRA_CA_CERTS
    let cfg = get_global_config();
    if let Some(v) = cfg.env.get("NODE_EXTRA_CA_CERTS") {
        if !v.is_empty() {
            return Some(v.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_ca_certs_config() {
        // 在 NODE_EXTRA_CA_CERTS 已设的情况下，函数应直接返回，不会 panic。
        apply_ca_certs_config();
    }
}
