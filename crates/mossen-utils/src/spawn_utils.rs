//! # spawn_utils — 生成队友工具函数
//!
//! 对应 TypeScript `utils/swarm/spawnUtils.ts`。
//! 跨不同后端生成队友的共享工具函数。

use std::env;

/// 获取用于生成队友进程的命令。
/// 如果设置了 TEAMMATE_COMMAND_ENV_VAR 则使用它，否则回退到当前进程可执行路径。
pub fn get_teammate_command() -> String {
    if let Ok(cmd) = env::var("MOSSEN_CODE_TEAMMATE_COMMAND") {
        return cmd;
    }

    // 与 TS `isInBundledMode() ? process.execPath : process.argv[1]!` 对齐：
    // bundled 模式下使用当前 exe；否则使用 argv[1]（与 Node CLI 入口对齐）。
    if crate::bundled_mode::is_in_bundled_mode() {
        return std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
    }

    env::args().nth(1).unwrap_or_else(|| {
        std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    })
}

/// 简单 shell 引号化（用于把单个 token 放进 `flag value` 之类的字符串）；
/// 与 TS `quote([value])` 对齐，未含特殊字符时返回原值，否则用单引号包裹。
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let needs_quotes = value.chars().any(|c| {
        matches!(
            c,
            ' ' | '\t'
                | '\n'
                | '"'
                | '\''
                | '$'
                | '`'
                | '\\'
                | '*'
                | '?'
                | '['
                | ']'
                | '{'
                | '}'
                | '('
                | ')'
                | '<'
                | '>'
                | '|'
                | '&'
                | ';'
                | '#'
                | '!'
                | '~'
        )
    });
    if !needs_quotes {
        return value.to_string();
    }
    // 单引号包裹；把内嵌单引号转义为 '\''
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 构建要从当前 session 传递到生成队友的 CLI 标志。
///
/// 这确保队友继承父级的重要设置，如权限模式、模型选择和插件配置。
/// 与 TS `buildInheritedCliFlags` 一致 —— 但 Rust 端从环境变量读取
/// `MOSSEN_CODE_*_OVERRIDE` 等值，因为 `bootstrap/state.ts` 暴露的 getter
/// 在 Rust 端尚未完全集成（CLI 解析层会在启动时把同名 env var 写回）。
pub fn build_inherited_cli_flags(
    plan_mode_required: bool,
    permission_mode: Option<&str>,
) -> String {
    let mut flags: Vec<String> = Vec::new();

    let bypass_env = matches!(
        env::var("MOSSEN_CODE_SESSION_BYPASS_PERMISSIONS")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    );

    // 计划模式优先于 bypass。
    if plan_mode_required {
        // 不继承 bypass 权限。
    } else if permission_mode == Some("bypassPermissions") || bypass_env {
        flags.push("--dangerously-skip-permissions".to_string());
    } else if permission_mode == Some("acceptEdits") {
        flags.push("--permission-mode acceptEdits".to_string());
    }

    // 模型覆盖。
    if let Ok(model) = env::var("MOSSEN_CODE_MAIN_LOOP_MODEL_OVERRIDE") {
        if !model.is_empty() {
            flags.push(format!("--model {}", shell_quote(&model)));
        }
    }

    // 设置文件路径。
    if let Ok(path) = env::var("MOSSEN_CODE_FLAG_SETTINGS_PATH") {
        if !path.is_empty() {
            flags.push(format!("--settings {}", shell_quote(&path)));
        }
    }

    // 内联插件目录（冒号分隔列表）。
    if let Ok(plugins) = env::var("MOSSEN_CODE_INLINE_PLUGIN_DIRS") {
        for dir in plugins.split(':').filter(|s| !s.is_empty()) {
            flags.push(format!("--plugin-dir {}", shell_quote(dir)));
        }
    }

    // 队友模式（默认 "subprocess"，与 TS 端 `getTeammateModeFromSnapshot` 等价）。
    let teammate_mode = env::var("MOSSEN_CODE_TEAMMATE_MODE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "subprocess".to_string());
    flags.push(format!("--teammate-mode {}", shell_quote(&teammate_mode)));

    // Chrome 标志覆盖。
    match env::var("MOSSEN_CODE_CHROME_FLAG_OVERRIDE").ok().as_deref() {
        Some("1") | Some("true") | Some("yes") => flags.push("--chrome".to_string()),
        Some("0") | Some("false") | Some("no") => flags.push("--no-chrome".to_string()),
        _ => {}
    }

    flags.join(" ")
}

/// 必须明确转发到 tmux 生成队友的环境变量列表。
/// Tmux 可能启动新的登录 shell 而不继承父级的 env，
/// 所以我们转发任何在当前进程中设置的变量。
const TEAMMATE_ENV_VARS: &[&str] = &[
    "MOSSEN_CODE_USE_BEDROCK",
    "MOSSEN_CODE_USE_VERTEX",
    "MOSSEN_CODE_USE_FOUNDRY",
    "MOSSEN_CODE_API_BASE_URL",
    "MOSSEN_CONFIG_DIR",
    "MOSSEN_CODE_REMOTE",
    "MOSSEN_CODE_REMOTE_MEMORY_DIR",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
];

/// 构建队友生成命令的 `env KEY=VALUE ...` 字符串。
/// 始终包含 MOSSENCODE=1 和 MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS=1，
/// 加上当前进程中设置的任何 provider/config 环境变量。
pub fn build_inherited_env_vars() -> String {
    let mut env_vars = vec![
        "MOSSENCODE=1".to_string(),
        "MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS=1".to_string(),
    ];

    for key in TEAMMATE_ENV_VARS {
        if let Ok(value) = env::var(key) {
            if !value.is_empty() {
                env_vars.push(format!("{key}={value}"));
            }
        }
    }

    env_vars.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_teammate_command() {
        let cmd = get_teammate_command();
        assert!(!cmd.is_empty());
    }

    #[test]
    fn test_build_inherited_env_vars() {
        let env_str = build_inherited_env_vars();
        assert!(env_str.contains("MOSSENCODE=1"));
        assert!(env_str.contains("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS=1"));
    }
}
