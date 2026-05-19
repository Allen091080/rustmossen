//! # subprocess_env — 子进程环境变量管理
//!
//! 对应 TypeScript `utils/subprocessEnv.ts`。
//! 在 GitHub Actions 中运行时，从子进程环境中移除敏感密钥，
//! 防止通过 shell 扩展的提示注入攻击泄露密钥。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;

/// 在 GitHub Actions 子进程中需要清理的环境变量列表。
///
/// 父进程保留这些变量（用于 API 调用、懒凭据读取）。
/// 仅子进程（bash、shell snapshot、MCP stdio、LSP、hooks）被清理。
///
/// GITHUB_TOKEN / GH_TOKEN 故意不清理——包装脚本（gh.sh）需要它们调用 GitHub API。
const GHA_SUBPROCESS_SCRUB: &[&str] = &[
    // Mossen auth — re-read per-request, subprocesses don't need them
    "MOSSEN_CODE_API_KEY",
    "MOSSEN_CODE_AUTH_TOKEN",
    "MOSSEN_CODE_FOUNDRY_API_KEY",
    "MOSSEN_CODE_CUSTOM_HEADERS",
    // OTLP exporter headers
    "OTEL_EXPORTER_OTLP_HEADERS",
    "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
    "OTEL_EXPORTER_OTLP_METRICS_HEADERS",
    "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
    // Cloud provider creds
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_BEARER_TOKEN_BEDROCK",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "AZURE_CLIENT_SECRET",
    "AZURE_CLIENT_CERTIFICATE_PATH",
    // GitHub Actions OIDC
    "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
    "ACTIONS_ID_TOKEN_REQUEST_URL",
    // GitHub Actions artifact/cache API
    "ACTIONS_RUNTIME_TOKEN",
    "ACTIONS_RUNTIME_URL",
    // mossen-action-specific duplicates
    "ALL_INPUTS",
    "OVERRIDE_GITHUB_TOKEN",
    "DEFAULT_WORKFLOW_TOKEN",
    "SSH_SIGNING_KEY",
];

/// 上游代理环境函数（懒注册）
static UPSTREAM_PROXY_ENV_FN: Lazy<Mutex<Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>>> =
    Lazy::new(|| Mutex::new(None));

/// 注册上游代理环境函数。
///
/// 由 init.ts 在 upstreamproxy 模块被懒加载后调用。
/// 必须在任何子进程被生成之前调用。
pub fn register_upstream_proxy_env_fn(
    f: Box<dyn Fn() -> HashMap<String, String> + Send + Sync>,
) {
    let mut guard = UPSTREAM_PROXY_ENV_FN.lock();
    *guard = Some(f);
}

/// 返回用于子进程的环境变量副本，已移除敏感密钥。
///
/// 受 MOSSEN_CODE_SUBPROCESS_ENV_SCRUB 控制。mossen-action 在
/// `allowed_non_write_users` 配置时自动设置此标志——该标志
/// 将工作流暴露给不受信任的内容（提示注入攻击面）。
pub fn subprocess_env() -> HashMap<String, String> {
    // CCR upstreamproxy: inject HTTPS_PROXY + CA bundle vars
    let proxy_env = {
        let guard = UPSTREAM_PROXY_ENV_FN.lock();
        match &*guard {
            Some(f) => f(),
            None => HashMap::new(),
        }
    };

    let scrub_enabled = std::env::var("MOSSEN_CODE_SUBPROCESS_ENV_SCRUB")
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if !scrub_enabled {
        let mut env: HashMap<String, String> = std::env::vars().collect();
        if !proxy_env.is_empty() {
            env.extend(proxy_env);
        }
        return env;
    }

    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.extend(proxy_env);

    for key in GHA_SUBPROCESS_SCRUB {
        env.remove(*key);
        // GitHub Actions auto-creates INPUT_<NAME> for `with:` inputs
        env.remove(&format!("INPUT_{}", key));
    }

    env
}
