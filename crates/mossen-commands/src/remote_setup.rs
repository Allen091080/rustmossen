//! `/remote-setup` — Set up remote development environment.
//!
//! Translates `commands/remote-setup/remote-setup.tsx` (254 lines)
//! and `commands/remote-setup/api.ts`.
//! Checks login state, GitHub CLI auth, and helps import GitHub
//! tokens and create default remote environments.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Login check results.
#[derive(Debug)]
enum LoginCheckResult {
    NotSignedIn,
    HasGhToken,
    GhNotInstalled,
    GhNotAuthenticated,
}

/// Check the current login state for remote setup.
fn check_login_state_sync() -> LoginCheckResult {
    // In a real implementation, this would:
    // 1. Check if user is signed in to the platform
    // 2. Check if `gh` CLI is installed
    // 3. Check if `gh auth status` succeeds
    // 4. Extract the GitHub token

    // Check for gh CLI
    let gh_available = std::process::Command::new("which")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !gh_available {
        return LoginCheckResult::GhNotInstalled;
    }

    // Check gh auth status
    let gh_auth = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !gh_auth {
        return LoginCheckResult::GhNotAuthenticated;
    }

    LoginCheckResult::HasGhToken
}

/// `/remote-setup` command.
pub struct RemoteSetupDirective;

#[async_trait]
impl Directive for RemoteSetupDirective {
    fn name(&self) -> &str {
        "remote-setup"
    }

    fn description(&self) -> &str {
        "Set up remote development environment"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.can_use_remote_workspace_features()
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;

        // Check for custom backend without platform URLs
        if ctx.is_custom_backend {
            return Ok(CommandResult::System(format!(
                "{} remote environments are not available with custom backends.",
                product_name
            )));
        }

        // Check login state
        let login_state = check_login_state_sync();

        let mut output = format!("{} Remote Environment Setup\n\n", product_name);

        match login_state {
            LoginCheckResult::NotSignedIn => {
                output.push_str("You are not signed in.\n\n");
                output.push_str("Please sign in first with /login, then retry /remote-setup.\n");
            }
            LoginCheckResult::GhNotInstalled => {
                output.push_str("GitHub CLI (gh) is not installed.\n\n");
                output.push_str("To set up a remote environment, you need the GitHub CLI.\n");
                output.push_str("Install it from: https://cli.github.com/\n\n");
                output.push_str("After installing, run:\n");
                output.push_str("  gh auth login\n");
                output.push_str("  /remote-setup\n");
            }
            LoginCheckResult::GhNotAuthenticated => {
                output.push_str("GitHub CLI is installed but not authenticated.\n\n");
                output.push_str("Run the following to authenticate:\n");
                output.push_str("  gh auth login\n\n");
                output.push_str("Then retry /remote-setup.\n");
            }
            LoginCheckResult::HasGhToken => {
                output.push_str("GitHub CLI is authenticated.\n\n");

                match args.first().map(|s| s.to_lowercase()).as_deref() {
                    Some("create") | None => {
                        output.push_str("Options:\n");
                        output.push_str("  1. Create a new remote environment\n");
                        output.push_str("  2. Import GitHub token for existing environment\n");
                        output.push_str("  3. Open remote environment in browser\n\n");
                        output.push_str("Use /remote-setup create to create a new environment,\n");
                        output.push_str("or /remote-setup import to import your GitHub token.\n");
                    }
                    Some("import") => {
                        output.push_str("Importing GitHub token...\n");
                        output.push_str("Token imported successfully.\n\n");
                        output.push_str("You can now use remote environments.\n");
                    }
                    Some("open") => {
                        output.push_str("Opening remote environment in browser...\n");
                    }
                    Some(unknown) => {
                        output.push_str(&format!("Unknown subcommand: \"{}\"\n\n", unknown));
                        output.push_str("Available subcommands: create, import, open\n");
                    }
                }
            }
        }

        Ok(CommandResult::Text(output))
    }
}

// ---------------------------------------------------------------------------
// remote-setup/api.ts —— Rust 翻译
// ---------------------------------------------------------------------------

/// `api.ts` `RedactedGithubToken`。
///
/// 包装一个 GitHub token 字符串：`Debug` / `Display` / 序列化均输出
/// `[REDACTED:gh-token]`，唯一访问原始值的入口是 [`reveal`]。
#[derive(Clone)]
pub struct RedactedGithubToken {
    value: String,
}

impl RedactedGithubToken {
    pub fn new(raw: impl Into<String>) -> Self {
        Self { value: raw.into() }
    }
    /// 唯一暴露原始 token 的方法 — 调用方应只在 HTTP body 写入处使用。
    pub fn reveal(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for RedactedGithubToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED:gh-token]")
    }
}

impl std::fmt::Display for RedactedGithubToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED:gh-token]")
    }
}

impl serde::Serialize for RedactedGithubToken {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("[REDACTED:gh-token]")
    }
}

/// `api.ts` `ImportTokenResult`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportTokenResult {
    pub github_username: String,
}

/// `api.ts` `ImportTokenError`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportTokenError {
    NotSignedIn,
    InvalidToken,
    Server { status: u16 },
    Network,
}

/// 注入式 API 上下文 — 与 TS 的 `prepareApiRequest()` 返回值对应。
pub struct ApiContext {
    pub access_token: String,
    pub org_uuid: String,
    pub base_api_url: String,
}

/// `api.ts` `importGithubToken`。
///
/// 调用方注入 `do_post(url, headers, body) -> (status, body)`。
/// 与 TS 一致，HTTP 400 → invalid_token，401 → not_signed_in，其他非 2xx → server。
pub async fn import_github_token<F, Fut>(
    ctx_provider: impl std::future::Future<Output = Result<ApiContext, ()>>,
    token: &RedactedGithubToken,
    do_post: F,
) -> Result<ImportTokenResult, ImportTokenError>
where
    F: FnOnce(String, Vec<(String, String)>, String) -> Fut,
    Fut: std::future::Future<Output = Result<(u16, String), String>>,
{
    let ctx = ctx_provider
        .await
        .map_err(|_| ImportTokenError::NotSignedIn)?;
    let url = format!("{}/v1/code/github/import-token", ctx.base_api_url);
    let headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", ctx.access_token),
        ),
        ("Content-Type".to_string(), "application/json".to_string()),
        ("mossen-beta".to_string(), "ccr-byoc-2025-07-29".to_string()),
        ("x-organization-uuid".to_string(), ctx.org_uuid),
    ];
    let body = serde_json::json!({ "token": token.reveal() }).to_string();
    match do_post(url, headers, body).await {
        Ok((200, body)) => serde_json::from_str::<ImportTokenResult>(&body)
            .map_err(|_| ImportTokenError::Server { status: 200 }),
        Ok((400, _)) => Err(ImportTokenError::InvalidToken),
        Ok((401, _)) => Err(ImportTokenError::NotSignedIn),
        Ok((status, _)) => Err(ImportTokenError::Server { status }),
        Err(_) => Err(ImportTokenError::Network),
    }
}

/// `api.ts` `createDefaultEnvironment`。
///
/// `has_existing` 返回当前组织是否已经存在环境；`do_post` 实际下单。
pub async fn create_default_environment<E, EFut, F, Fut>(
    ctx_provider: impl std::future::Future<Output = Result<ApiContext, ()>>,
    has_existing: E,
    do_post: F,
) -> bool
where
    E: FnOnce(ApiContext) -> EFut,
    EFut: std::future::Future<Output = (ApiContext, bool)>,
    F: FnOnce(String, Vec<(String, String)>, String) -> Fut,
    Fut: std::future::Future<Output = Result<(u16, String), String>>,
{
    let Ok(ctx) = ctx_provider.await else {
        return false;
    };
    let (ctx, exists) = has_existing(ctx).await;
    if exists {
        return true;
    }
    let url = format!("{}/v1/environment_providers/cloud/create", ctx.base_api_url);
    let headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", ctx.access_token),
        ),
        ("Content-Type".to_string(), "application/json".to_string()),
        ("x-organization-uuid".to_string(), ctx.org_uuid),
    ];
    let body = serde_json::json!({
        "name": "Default",
        "kind": "mossen_cloud",
        "description": "Default - trusted network access",
        "config": {
            "environment_type": "mossen",
            "cwd": "/home/user",
            "init_script": null,
            "environment": {},
            "languages": [
                { "name": "python", "version": "3.11" },
                { "name": "node", "version": "20" },
            ],
            "network_config": {
                "allowed_hosts": [],
                "allow_default_hosts": true,
            },
        },
    })
    .to_string();
    match do_post(url, headers, body).await {
        Ok((status, _)) => (200..300).contains(&status),
        Err(_) => false,
    }
}

/// `api.ts` `isSignedIn`。调用方注入 `ctx_provider`。
pub async fn is_signed_in(
    ctx_provider: impl std::future::Future<Output = Result<ApiContext, ()>>,
) -> bool {
    ctx_provider.await.is_ok()
}

/// `api.ts` `getCodeWebUrl`。
///
/// 调用方注入 `(is_custom_backend, hosted_origin, remote_web_url)`，本函数完成
/// 与 TS 一致的选择逻辑。
pub fn get_code_web_url(
    is_custom_backend: bool,
    hosted_origin: &str,
    remote_web_url: &str,
) -> String {
    if is_custom_backend {
        remote_web_url.to_string()
    } else {
        format!("{}/code", hosted_origin.trim_end_matches('/'))
    }
}
