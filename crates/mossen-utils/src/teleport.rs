//! # teleport — Teleport (远程会话) 核心逻辑
//!
//! 对应 TypeScript `utils/teleport.tsx`。
//!
//! 本模块负责：
//! - Git 状态校验、分支切换（teleport resume 的本地副作用）
//! - 远程会话的创建、轮询、归档（Sessions API 调用）
//! - 仓库匹配校验（防止跨仓库 teleport）
//! - Haiku 自动生成标题 + 分支名
//!
//! ## 子模块
//! - [`api`] — Sessions API 的底层 HTTP 与类型定义（来自旧版 `teleport/mod.rs`）。

pub mod api;
pub mod environments;
pub mod environment_selection;
pub mod git_bundle;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;

use crate::conversation_recovery::{deserialize_messages, TeleportRemoteResponse};
use crate::cwd::get_cwd;
use crate::debug::{log_for_debugging, DebugLogLevel};
use crate::detect_repository::{
    detect_current_repository_with_host, parse_git_remote, parse_github_repository,
    ParsedRepository,
};
use crate::errors::TeleportOperationError;
use crate::exec_file_no_throw::{exec_file_no_throw, ExecFileOptions};
use crate::git::{find_git_root, get_is_clean, git_exe};
use crate::json::safe_parse_json_value;
use crate::log::log_error_str;
use crate::messages::{create_system_message, create_user_message, CreateUserMessageParams};
use crate::session_storage::is_transcript_message;
use crate::truncate::truncate_to_width;
use crate::teleport::api::{
    GitRepositoryOutcome, OutcomeGitInfo, SessionContextSource, SessionResource,
    get_branch_from_session,
};

// ---------------------------------------------------------------------------
// 公开类型
// ---------------------------------------------------------------------------

/// Teleport 完成结果：消息流 + 当前分支名。
#[derive(Debug, Clone)]
pub struct TeleportResult {
    pub messages: Vec<Value>,
    pub branch_name: String,
}

/// Teleport 流程的进度阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeleportProgressStep {
    Validating,
    FetchingLogs,
    FetchingBranch,
    CheckingOut,
    Done,
}

impl TeleportProgressStep {
    pub fn as_str(self) -> &'static str {
        match self {
            TeleportProgressStep::Validating => "validating",
            TeleportProgressStep::FetchingLogs => "fetching_logs",
            TeleportProgressStep::FetchingBranch => "fetching_branch",
            TeleportProgressStep::CheckingOut => "checking_out",
            TeleportProgressStep::Done => "done",
        }
    }
}

/// 进度回调：每进入一个阶段时被调用一次。
pub type TeleportProgressCallback = Arc<dyn Fn(TeleportProgressStep) + Send + Sync>;

/// 通过 Sessions API 创建远程会话后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeleportToRemoteResponse {
    pub id: String,
    pub title: String,
}

/// 由 Haiku 模型生成的标题和分支名。
#[derive(Debug, Clone)]
pub struct TitleAndBranch {
    pub title: String,
    pub branch_name: String,
}

/// 仓库匹配状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoValidationStatus {
    Match,
    Mismatch,
    NotInRepo,
    NoRepoRequired,
    Error,
}

impl RepoValidationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RepoValidationStatus::Match => "match",
            RepoValidationStatus::Mismatch => "mismatch",
            RepoValidationStatus::NotInRepo => "not_in_repo",
            RepoValidationStatus::NoRepoRequired => "no_repo_required",
            RepoValidationStatus::Error => "error",
        }
    }
}

/// 仓库匹配校验结果。
#[derive(Debug, Clone, Default)]
pub struct RepoValidationResult {
    pub status: RepoValidationStatus,
    pub session_repo: Option<String>,
    pub current_repo: Option<String>,
    /// 会话仓库所在主机（如 github.com、ghe.corp.com），仅用于展示。
    pub session_host: Option<String>,
    /// 当前仓库所在主机，仅用于展示。
    pub current_host: Option<String>,
    pub error_message: Option<String>,
}

impl Default for RepoValidationStatus {
    fn default() -> Self {
        RepoValidationStatus::NoRepoRequired
    }
}

/// 轮询远程会话事件的响应（增量）。
#[derive(Debug, Clone, Default)]
pub struct PollRemoteSessionResponse {
    pub new_events: Vec<Value>,
    pub last_event_id: Option<String>,
    pub branch: Option<String>,
    pub session_status: Option<String>,
}

/// 轮询时的额外参数。
#[derive(Debug, Clone, Default)]
pub struct PollRemoteSessionOptions {
    /// 跳过每次拉取后调用 GET /v1/sessions/{id}（不需要 branch/status 时）。
    pub skip_metadata: bool,
}

/// 取消信号：包装 `tokio::sync::Notify`，供 abort-style API 共享。
#[derive(Clone, Default)]
pub struct AbortSignal {
    inner: Arc<Notify>,
    aborted: Arc<std::sync::atomic::AtomicBool>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn abort(&self) {
        self.aborted
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.inner.notify_waiters();
    }
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub async fn wait_aborted(&self) {
        if self.is_aborted() {
            return;
        }
        self.inner.notified().await;
    }
}

impl std::fmt::Debug for AbortSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortSignal")
            .field("aborted", &self.is_aborted())
            .finish()
    }
}

/// 创建远程会话时的可选参数。
#[derive(Clone, Default)]
pub struct TeleportToRemoteOptions {
    pub initial_message: Option<String>,
    pub branch_name: Option<String>,
    pub title: Option<String>,
    /// 用于生成 title/branch（除非显式给出）的描述。
    pub description: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub ultraplan: bool,
    pub signal: Option<AbortSignal>,
    pub use_default_environment: bool,
    /// 显式 environment_id（例如 code_review 合成 env）。
    pub environment_id: Option<String>,
    pub environment_variables: Option<HashMap<String, String>>,
    /// 与 environment_id 一起使用：上传本地工作树 bundle 作为 seed。
    pub use_bundle: bool,
    /// 当 bundle 路径失败时回调，展示用户级错误。
    pub on_bundle_fail: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// 完全禁用 git-bundle 回退（用于例如 autofix 必须推到 GitHub 的场景）。
    pub skip_bundle: bool,
    /// 复用此分支作为 outcome（不再新建 mossen/ 分支）。
    pub reuse_outcome_branch: Option<String>,
    /// 关联到此会话的 GitHub PR。
    pub github_pr: Option<GithubPrRef>,
}

impl std::fmt::Debug for TeleportToRemoteOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeleportToRemoteOptions")
            .field("initial_message", &self.initial_message)
            .field("branch_name", &self.branch_name)
            .field("title", &self.title)
            .field("description", &self.description)
            .field("model", &self.model)
            .field("permission_mode", &self.permission_mode)
            .field("ultraplan", &self.ultraplan)
            .field("signal", &self.signal)
            .field("use_default_environment", &self.use_default_environment)
            .field("environment_id", &self.environment_id)
            .field("environment_variables", &self.environment_variables)
            .field("use_bundle", &self.use_bundle)
            .field("on_bundle_fail", &self.on_bundle_fail.as_ref().map(|_| "<callback>"))
            .field("skip_bundle", &self.skip_bundle)
            .field("reuse_outcome_branch", &self.reuse_outcome_branch)
            .field("github_pr", &self.github_pr)
            .finish()
    }
}

/// 引用 GitHub PR（owner/repo/number）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPrRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

// ---------------------------------------------------------------------------
// 内部 helpers — 消息构造
// ---------------------------------------------------------------------------

/// 创建一条 system 消息，告知用户会话已从另一台机器恢复。
fn create_teleport_resume_system_message(branch_error: Option<&str>) -> Value {
    match branch_error {
        None => create_system_message("Session resumed", "suggestion", None, None),
        Some(msg) => create_system_message(
            &format!("Session resumed without branch: {}", msg),
            "warning",
            None,
            None,
        ),
    }
}

/// 创建一条 user 消息（isMeta=true），告知模型当前的工作目录。
fn create_teleport_resume_user_message() -> Value {
    let cwd = get_cwd();
    create_user_message(CreateUserMessageParams {
        content: Some(Value::String(format!(
            "This session is being continued from another machine. Application state may have changed. The updated working directory is {}",
            cwd
        ))),
        is_meta: Some(true),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// 标题 + 分支名生成
// ---------------------------------------------------------------------------

const SESSION_TITLE_AND_BRANCH_PROMPT: &str = r#"You are coming up with a succinct title and git branch name for a coding session based on the provided description. The title should be clear, concise, and accurately reflect the content of the coding task.
You should keep it short and simple, ideally no more than 6 words. Avoid using jargon or overly technical terms unless absolutely necessary. The title should be easy to understand for anyone reading it.
Use sentence case for the title (capitalize only the first word and proper nouns), not Title Case.

The branch name should be clear, concise, and accurately reflect the content of the coding task.
You should keep it short and simple, ideally no more than 4 words. The branch should always start with "mossen/" and should be all lower case, with words separated by dashes.

Return a JSON object with "title" and "branch" fields.

Example 1: {"title": "Fix login button not working on mobile", "branch": "mossen/fix-mobile-login-button"}
Example 2: {"title": "Update README with installation instructions", "branch": "mossen/update-readme"}
Example 3: {"title": "Improve performance of data processing script", "branch": "mossen/improve-data-processing"}

Here is the session description:
<description>{description}</description>
Please generate a title and branch name for this session."#;

/// Haiku 客户端接口：调用方需提供（在 mossen-agent 中已有实现）。
/// 默认实现：无 Haiku 可用，直接走回退路径。
#[async_trait::async_trait]
pub trait HaikuTitleClient: Send + Sync {
    /// 生成 JSON 文本（必须能被 `serde_json::from_str` 解析）。
    async fn generate(&self, prompt: &str) -> Result<String>;
}

/// Haiku 不可用时的 no-op 实现：返回错误，让调用方走 fallback。
pub struct NoopHaikuClient;

#[async_trait::async_trait]
impl HaikuTitleClient for NoopHaikuClient {
    async fn generate(&self, _prompt: &str) -> Result<String> {
        anyhow::bail!("Haiku title client not configured")
    }
}

/// 用 Mossen Haiku 给会话生成标题和分支名。
///
/// 失败时回退到从描述截取（title）和 `mossen/task`（branch）。
pub async fn generate_title_and_branch(
    description: &str,
    client: &dyn HaikuTitleClient,
) -> TitleAndBranch {
    let fallback_title = truncate_to_width(description, 75);
    let fallback_branch = "mossen/task".to_string();

    let prompt = SESSION_TITLE_AND_BRANCH_PROMPT.replace("{description}", description);

    match client.generate(&prompt).await {
        Ok(json_text) => {
            let trimmed = json_text.trim();
            if let Some(parsed) = safe_parse_json_value(trimmed) {
                let title = parsed
                    .get("title")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| fallback_title.clone());
                let branch = parsed
                    .get("branch")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| fallback_branch.clone());
                TitleAndBranch {
                    title,
                    branch_name: branch,
                }
            } else {
                TitleAndBranch {
                    title: fallback_title,
                    branch_name: fallback_branch,
                }
            }
        }
        Err(e) => {
            log_error_str(&format!("Error generating title and branch: {}", e));
            TitleAndBranch {
                title: fallback_title,
                branch_name: fallback_branch,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Git 状态校验 + 分支操作
// ---------------------------------------------------------------------------

/// 校验当前 git 工作目录是否干净（忽略 untracked 文件）。
///
/// 失败时抛出 `TeleportOperationError`，调用方应中断 teleport 流程。
pub async fn validate_git_state() -> Result<(), TeleportOperationError> {
    let is_clean = get_is_clean(true).await;
    if !is_clean {
        return Err(TeleportOperationError::new(
            "Git working directory is not clean. Please commit or stash your changes before using --teleport.",
            "Error: Git working directory is not clean. Please commit or stash your changes before using --teleport.\n",
        ));
    }
    Ok(())
}

/// 从远端 origin 拉取分支（不指定时拉取全部）。
async fn fetch_from_origin(branch: Option<&str>) {
    let fetch_args: Vec<String> = match branch {
        Some(b) => vec!["fetch".into(), "origin".into(), format!("{0}:{0}", b)],
        None => vec!["fetch".into(), "origin".into()],
    };
    let args_ref: Vec<&str> = fetch_args.iter().map(|s| s.as_str()).collect();
    let result = exec_file_no_throw(&git_exe(), &args_ref, ExecFileOptions::default()).await;
    if result.code != 0 {
        // 如果是指定分支失败，且涉及 refspec，再试一次 fetch 不映射本地分支
        if let Some(b) = branch {
            if result.stderr.contains("refspec") {
                log_for_debugging(
                    &format!("Specific branch fetch failed, trying to fetch ref: {}", b),
                    DebugLogLevel::Debug,
                );
                let fallback = exec_file_no_throw(
                    &git_exe(),
                    &["fetch", "origin", b],
                    ExecFileOptions::default(),
                )
                .await;
                if fallback.code != 0 {
                    log_error_str(&format!(
                        "Failed to fetch from remote origin: {}",
                        fallback.stderr
                    ));
                }
                return;
            }
        }
        log_error_str(&format!(
            "Failed to fetch from remote origin: {}",
            result.stderr
        ));
    }
}

/// 确保 `branch_name` 设置了 upstream；若 origin/<branch_name> 存在但未跟踪，则设置之。
async fn ensure_upstream_is_set(branch_name: &str) {
    let upstream_check = exec_file_no_throw(
        &git_exe(),
        &[
            "rev-parse",
            "--abbrev-ref",
            &format!("{}@{{upstream}}", branch_name),
        ],
        ExecFileOptions::default(),
    )
    .await;
    if upstream_check.code == 0 {
        log_for_debugging(
            &format!("Branch '{}' already has upstream set", branch_name),
            DebugLogLevel::Debug,
        );
        return;
    }

    let remote_check = exec_file_no_throw(
        &git_exe(),
        &["rev-parse", "--verify", &format!("origin/{}", branch_name)],
        ExecFileOptions::default(),
    )
    .await;
    if remote_check.code == 0 {
        log_for_debugging(
            &format!(
                "Setting upstream for '{}' to 'origin/{}'",
                branch_name, branch_name
            ),
            DebugLogLevel::Debug,
        );
        let set = exec_file_no_throw(
            &git_exe(),
            &[
                "branch",
                "--set-upstream-to",
                &format!("origin/{}", branch_name),
                branch_name,
            ],
            ExecFileOptions::default(),
        )
        .await;
        if set.code != 0 {
            log_for_debugging(
                &format!(
                    "Failed to set upstream for '{}': {}",
                    branch_name, set.stderr
                ),
                DebugLogLevel::Debug,
            );
        } else {
            log_for_debugging(
                &format!("Successfully set upstream for '{}'", branch_name),
                DebugLogLevel::Debug,
            );
        }
    } else {
        log_for_debugging(
            &format!(
                "Remote branch 'origin/{}' does not exist, skipping upstream setup",
                branch_name
            ),
            DebugLogLevel::Debug,
        );
    }
}

/// 切换到指定分支：先尝试本地，失败则从 origin 跟踪。
async fn checkout_branch(branch_name: &str) -> Result<(), TeleportOperationError> {
    let mut checkout = exec_file_no_throw(
        &git_exe(),
        &["checkout", branch_name],
        ExecFileOptions::default(),
    )
    .await;

    if checkout.code != 0 {
        log_for_debugging(
            &format!(
                "Local checkout failed, trying to checkout from origin: {}",
                checkout.stderr
            ),
            DebugLogLevel::Debug,
        );

        let remote_ref = format!("origin/{}", branch_name);
        checkout = exec_file_no_throw(
            &git_exe(),
            &["checkout", "-b", branch_name, "--track", &remote_ref],
            ExecFileOptions::default(),
        )
        .await;

        if checkout.code != 0 {
            log_for_debugging(
                &format!(
                    "Remote checkout with -b failed, trying without -b: {}",
                    checkout.stderr
                ),
                DebugLogLevel::Debug,
            );
            checkout = exec_file_no_throw(
                &git_exe(),
                &["checkout", "--track", &remote_ref],
                ExecFileOptions::default(),
            )
            .await;
        }
    }

    if checkout.code != 0 {
        return Err(TeleportOperationError::new(
            format!(
                "Failed to checkout branch '{}': {}",
                branch_name, checkout.stderr
            ),
            format!("Failed to checkout branch '{}'\n", branch_name),
        ));
    }

    ensure_upstream_is_set(branch_name).await;
    Ok(())
}

/// 获取当前分支名。
pub async fn get_current_branch() -> String {
    let result = exec_file_no_throw(
        &git_exe(),
        &["branch", "--show-current"],
        ExecFileOptions::default(),
    )
    .await;
    result.stdout.trim().to_string()
}

/// 处理 teleport resume 时的消息列表：
/// 1) 走 `deserialize_messages` 复用 resume 的中断处理逻辑
/// 2) 追加 user/system 通知消息
pub fn process_messages_for_teleport_resume(
    messages: Vec<Value>,
    error: Option<&str>,
) -> Vec<Value> {
    let cwd = get_cwd();
    let mut result = deserialize_messages(messages, &cwd);
    result.push(create_teleport_resume_user_message());
    result.push(create_teleport_resume_system_message(error));
    result
}

/// 切换到 teleported 会话的目标分支。
///
/// 即使分支切换失败也返回当前分支名（让调用方继续 teleport，仅记录错误）。
pub async fn check_out_teleported_session_branch(
    branch: Option<&str>,
) -> (String, Option<TeleportOperationError>) {
    let original = get_current_branch().await;
    log_for_debugging(
        &format!("Current branch before teleport: '{}'", original),
        DebugLogLevel::Debug,
    );

    if let Some(b) = branch {
        log_for_debugging(
            &format!("Switching to branch '{}'...", b),
            DebugLogLevel::Debug,
        );
        fetch_from_origin(Some(b)).await;
        if let Err(e) = checkout_branch(b).await {
            let current = get_current_branch().await;
            return (current, Some(e));
        }
        let new_branch = get_current_branch().await;
        log_for_debugging(
            &format!("Branch after checkout: '{}'", new_branch),
            DebugLogLevel::Debug,
        );
    } else {
        log_for_debugging("No branch specified, staying on current branch", DebugLogLevel::Debug);
    }

    let final_branch = get_current_branch().await;
    (final_branch, None)
}

// ---------------------------------------------------------------------------
// 仓库匹配校验
// ---------------------------------------------------------------------------

fn strip_port(host: &str) -> String {
    // 把末尾的 ":<digits>" 去掉
    if let Some(idx) = host.rfind(':') {
        let after = &host[idx + 1..];
        if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
            return host[..idx].to_string();
        }
    }
    host.to_string()
}

/// 校验当前仓库是否匹配 session 的仓库。
///
/// 不抛错；返回结果对象供调用方解读。
pub async fn validate_session_repository(session_data: &SessionResource) -> RepoValidationResult {
    let current_parsed = detect_repo_in_cwd().await;
    let current_repo = current_parsed
        .as_ref()
        .map(|p| format!("{}/{}", p.owner, p.name));

    let git_source_url = session_data
        .session_context
        .sources
        .iter()
        .find_map(|s| match s {
            SessionContextSource::Git { url, .. } => Some(url.clone()),
            _ => None,
        });

    let Some(url) = git_source_url else {
        log_for_debugging(
            if current_repo.is_some() {
                "Session has no associated repository, proceeding without validation"
            } else {
                "Session has no repo requirement and not in git directory, proceeding"
            },
            DebugLogLevel::Debug,
        );
        return RepoValidationResult {
            status: RepoValidationStatus::NoRepoRequired,
            ..Default::default()
        };
    };

    let session_parsed = parse_git_remote(&url);
    let session_repo = match &session_parsed {
        Some(p) => Some(format!("{}/{}", p.owner, p.name)),
        None => parse_github_repository(&url),
    };

    let Some(session_repo) = session_repo else {
        return RepoValidationResult {
            status: RepoValidationStatus::NoRepoRequired,
            ..Default::default()
        };
    };

    log_for_debugging(
        &format!(
            "Session is for repository: {}, current repo: {}",
            session_repo,
            current_repo.as_deref().unwrap_or("none")
        ),
        DebugLogLevel::Debug,
    );

    let Some(current_repo_str) = current_repo.clone() else {
        return RepoValidationResult {
            status: RepoValidationStatus::NotInRepo,
            session_repo: Some(session_repo),
            session_host: session_parsed.map(|p| p.host),
            current_repo: None,
            ..Default::default()
        };
    };

    let repo_match = current_repo_str.to_lowercase() == session_repo.to_lowercase();
    let host_match = match (&current_parsed, &session_parsed) {
        (Some(c), Some(s)) => {
            strip_port(&c.host.to_lowercase()) == strip_port(&s.host.to_lowercase())
        }
        _ => true,
    };

    if repo_match && host_match {
        return RepoValidationResult {
            status: RepoValidationStatus::Match,
            session_repo: Some(session_repo),
            current_repo: Some(current_repo_str),
            ..Default::default()
        };
    }

    RepoValidationResult {
        status: RepoValidationStatus::Mismatch,
        session_repo: Some(session_repo),
        current_repo: Some(current_repo_str),
        session_host: session_parsed.map(|p| p.host),
        current_host: current_parsed.map(|p| p.host),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// 会话日志拉取
// ---------------------------------------------------------------------------

/// 由调用方注入的「拉取会话事件 / 日志」接口。
///
/// 同时提供 v2（GetTeleportEvents）和 fallback（session-ingress）两路；
/// 实现可以选择其一。如果都失败，返回 `Ok(None)`。
#[async_trait::async_trait]
pub trait TeleportLogClient: Send + Sync {
    async fn get_teleport_events(
        &self,
        session_id: &str,
        access_token: &str,
        org_uuid: &str,
    ) -> Result<Option<Vec<Value>>>;

    async fn get_session_logs_via_oauth(
        &self,
        session_id: &str,
        access_token: &str,
        org_uuid: &str,
    ) -> Result<Option<Vec<Value>>>;
}

/// 从 Sessions API 拉取一个会话的事件流，
/// 经 transcript 过滤后返回 `TeleportRemoteResponse`。
pub async fn teleport_from_sessions_api(
    session_id: &str,
    org_uuid: &str,
    access_token: &str,
    client: &dyn TeleportLogClient,
    progress: Option<TeleportProgressCallback>,
    session_data: Option<&SessionResource>,
) -> Result<TeleportRemoteResponse, TeleportOperationError> {
    if let Some(cb) = progress.as_ref() {
        cb(TeleportProgressStep::FetchingLogs);
    }

    let logs = match client
        .get_teleport_events(session_id, access_token, org_uuid)
        .await
    {
        Ok(Some(events)) => Some(events),
        Ok(None) => {
            log_for_debugging(
                "[teleport] v2 endpoint returned null, trying session-ingress",
                DebugLogLevel::Debug,
            );
            client
                .get_session_logs_via_oauth(session_id, access_token, org_uuid)
                .await
                .ok()
                .flatten()
        }
        Err(e) => {
            log_error_str(&format!("get_teleport_events failed: {}", e));
            client
                .get_session_logs_via_oauth(session_id, access_token, org_uuid)
                .await
                .ok()
                .flatten()
        }
    };

    let Some(logs) = logs else {
        return Err(TeleportOperationError::new(
            "Failed to fetch session logs",
            "Failed to fetch session logs\n",
        ));
    };

    // 过滤 transcript 消息，排除 sidechain
    let messages: Vec<Value> = logs
        .into_iter()
        .filter(|entry| {
            if !is_transcript_message(entry) {
                return false;
            }
            !entry
                .get("isSidechain")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .collect();

    if let Some(cb) = progress.as_ref() {
        cb(TeleportProgressStep::FetchingBranch);
    }

    let branch = session_data
        .and_then(|s| get_branch_from_session(s))
        .map(|b| b.to_string());

    if let Some(ref b) = branch {
        log_for_debugging(
            &format!("[teleport] Found branch: {}", b),
            DebugLogLevel::Debug,
        );
    }

    Ok(TeleportRemoteResponse {
        log: messages,
        branch,
    })
}

// ---------------------------------------------------------------------------
// 远程会话 — 创建
// ---------------------------------------------------------------------------

/// 由调用方注入：完成 Sessions API 的 HTTP 调用 + 鉴权 +
/// 仓库 detection / bundle 上传等高层动作。
///
/// 该 trait 把 React/Ink 渲染、analytics、growthbook 等保留在 CLI 层（mossen-cli），
/// 让 mossen-utils 保持纯逻辑。
#[async_trait::async_trait]
pub trait TeleportRemoteClient: Send + Sync {
    /// 创建一个新的远程会话；返回 `Ok(Some(...))` 表示成功。
    async fn create_remote_session(
        &self,
        options: &TeleportToRemoteOptions,
        title: &str,
        sources: Vec<SessionContextSource>,
        outcomes: Vec<GitRepositoryOutcome>,
        seed_bundle_file_id: Option<String>,
        permission_mode: Option<&str>,
        ultraplan: bool,
        environment_id: Option<String>,
        access_token: &str,
        org_uuid: &str,
        events: Vec<Value>,
    ) -> Result<Option<TeleportToRemoteResponse>>;

    /// 拉取可用 environments；返回 `(environment_id, kind, name)` 列表。
    async fn fetch_environments(
        &self,
        access_token: &str,
        org_uuid: &str,
    ) -> Result<Vec<(String, String, String)>>;

    /// `(success, file_id, bundle_size_bytes, scope, has_wip, fail_reason, error_text)`
    async fn create_and_upload_git_bundle(
        &self,
        access_token: &str,
    ) -> Result<BundleUploadOutcome>;

    /// 检查 GitHub App 是否已安装，决定 GitHub 路径是否可行。
    async fn check_github_app_installed(&self, owner: &str, name: &str) -> bool;

    /// 检查 GrowthBook gate（bundle seed 是否开启）。
    async fn check_bundle_seed_gate(&self) -> bool;

    /// 获取主循环用的 model 名称（用于 session_context.model 默认值）。
    fn get_main_loop_model(&self) -> String;

    /// 获取默认分支（用于未指定 branchName 时的 revision fallback）。
    async fn get_default_branch(&self) -> Option<String>;

    /// 提供已可用的 hosted OAuth access token；None 表示未登录。
    async fn get_access_token(&self) -> Option<String>;

    /// 获取 organization UUID。
    async fn get_organization_uuid(&self) -> Option<String>;

    /// 刷新 OAuth 令牌（如有需要）。
    async fn refresh_oauth_if_needed(&self);

    /// 默认环境 ID（来自用户 settings）。`None` 表示未配置。
    fn default_environment_id(&self) -> Option<String>;

    /// 是否允许通过策略使用远程会话（policyLimits.allow_remote_sessions）。
    fn is_policy_allowed_remote_sessions(&self) -> bool;
}

/// 创建并上传 git bundle 的输出结构。
#[derive(Debug, Clone, Default)]
pub struct BundleUploadOutcome {
    pub success: bool,
    pub file_id: Option<String>,
    pub bundle_size_bytes: u64,
    pub scope: String,
    pub has_wip: bool,
    pub fail_reason: Option<BundleFailReason>,
    pub error: Option<String>,
}

/// Bundle 上传失败的具体原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleFailReason {
    EmptyRepo,
    TooLarge,
    GitError,
}

/// 从 source 选型阶段产出的「为什么走这条路」标签（用于 analytics）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceReason {
    GithubPreflightOk,
    GhesOptimistic,
    GithubPreflightFailed,
    NoGithubRemote,
    ForcedBundle,
    NoGitAtAll,
}

impl SourceReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceReason::GithubPreflightOk => "github_preflight_ok",
            SourceReason::GhesOptimistic => "ghes_optimistic",
            SourceReason::GithubPreflightFailed => "github_preflight_failed",
            SourceReason::NoGithubRemote => "no_github_remote",
            SourceReason::ForcedBundle => "forced_bundle",
            SourceReason::NoGitAtAll => "no_git_at_all",
        }
    }
}

/// 创建远程 hosted 会话。
///
/// 该函数承担「选型」（GitHub vs bundle vs 空沙盒）+ Haiku 标题生成 +
/// environment 选择，然后把组装好的请求委托给 `client.create_remote_session`。
pub async fn teleport_to_remote(
    options: TeleportToRemoteOptions,
    client: &dyn TeleportRemoteClient,
    haiku: &dyn HaikuTitleClient,
) -> Option<TeleportToRemoteResponse> {
    client.refresh_oauth_if_needed().await;
    let access_token = client.get_access_token().await?;
    let org_uuid = client.get_organization_uuid().await?;

    // 1) 显式 environmentId 短路：跳过 Haiku、跳过 env 选择。
    if let Some(ref environment_id) = options.environment_id {
        return create_explicit_env_session(
            &options,
            environment_id,
            &access_token,
            &org_uuid,
            client,
        )
        .await;
    }

    // 2) Source 选型
    let repo_info = detect_repo_in_cwd().await;
    let (title, branch_name) = resolve_title_and_branch(&options, haiku).await;

    let force_bundle = !options.skip_bundle && is_truthy_env("CCR_FORCE_BUNDLE");
    let git_root = find_git_root(&get_cwd());
    let bundle_seed_gate_on = !options.skip_bundle
        && git_root.is_some()
        && (is_truthy_env("CCR_ENABLE_BUNDLE") || client.check_bundle_seed_gate().await);

    let mut gh_viable = false;
    let mut source_reason = SourceReason::NoGitAtAll;

    if repo_info.is_some() && !force_bundle {
        let info = repo_info.as_ref().unwrap();
        if info.host == "github.com" {
            gh_viable = client.check_github_app_installed(&info.owner, &info.name).await;
            source_reason = if gh_viable {
                SourceReason::GithubPreflightOk
            } else {
                SourceReason::GithubPreflightFailed
            };
        } else {
            gh_viable = true;
            source_reason = SourceReason::GhesOptimistic;
        }
    } else if force_bundle {
        source_reason = SourceReason::ForcedBundle;
    } else if git_root.is_some() {
        source_reason = SourceReason::NoGithubRemote;
    }

    // 当 GH preflight 失败但 bundle 没开，乐观地继续走 GH 路径
    if !gh_viable && !bundle_seed_gate_on && repo_info.is_some() {
        gh_viable = true;
    }

    let mut sources: Vec<SessionContextSource> = Vec::new();
    let mut outcomes: Vec<GitRepositoryOutcome> = Vec::new();

    if gh_viable {
        if let Some(info) = &repo_info {
            let revision = options
                .branch_name
                .clone()
                .or_else(|| futures::executor::block_on(client.get_default_branch()));
            log_for_debugging(
                &format!(
                    "[teleportToRemote] Git source: {}/{}/{}, revision: {}",
                    info.host,
                    info.owner,
                    info.name,
                    revision.as_deref().unwrap_or("none")
                ),
                DebugLogLevel::Debug,
            );
            sources.push(SessionContextSource::Git {
                url: format!("https://{}/{}/{}", info.host, info.owner, info.name),
                revision,
                allow_unrestricted_git_push: if options.reuse_outcome_branch.is_some() {
                    Some(true)
                } else {
                    None
                },
            });
            outcomes.push(GitRepositoryOutcome {
                outcome_type: "git_repository".to_string(),
                git_info: OutcomeGitInfo {
                    outcome_type: "github".to_string(),
                    repo: format!("{}/{}", info.owner, info.name),
                    branches: vec![branch_name.clone()],
                },
            });
        }
    }

    // 3) Bundle fallback
    let mut seed_bundle_file_id: Option<String> = None;
    if sources.is_empty() && bundle_seed_gate_on {
        log_for_debugging(
            &format!("[teleportToRemote] Bundling (reason: {})", source_reason.as_str()),
            DebugLogLevel::Debug,
        );
        match client.create_and_upload_git_bundle(&access_token).await {
            Ok(bundle) => {
                if !bundle.success {
                    let msg = bundle_failure_message(&bundle, repo_info.is_some());
                    log_error_str(&format!(
                        "Bundle upload failed: {}",
                        bundle.error.as_deref().unwrap_or("?")
                    ));
                    if let Some(cb) = options.on_bundle_fail.as_ref() {
                        cb(&msg);
                    }
                    return None;
                }
                seed_bundle_file_id = bundle.file_id;
            }
            Err(e) => {
                log_error_str(&format!("Bundle upload error: {}", e));
                return None;
            }
        }
    }

    if sources.is_empty() && seed_bundle_file_id.is_none() {
        log_for_debugging(
            "[teleportToRemote] No repository detected — session will have an empty sandbox",
            DebugLogLevel::Debug,
        );
    }

    // 4) Environment 选择
    let environments = match client.fetch_environments(&access_token, &org_uuid).await {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => {
            log_error_str("No environments available for session creation");
            return None;
        }
        Err(e) => {
            log_error_str(&format!("fetch_environments error: {}", e));
            return None;
        }
    };

    let default_env_id = if options.use_default_environment {
        None
    } else {
        client.default_environment_id()
    };
    let cloud_env = environments
        .iter()
        .find(|(_, kind, _)| kind == "mossen_cloud")
        .cloned();

    let selected = if let Some(id) = default_env_id.as_ref() {
        environments
            .iter()
            .find(|(eid, _, _)| eid == id)
            .cloned()
            .or(cloud_env.clone())
    } else {
        cloud_env.clone()
    }
    .or_else(|| environments.iter().find(|(_, kind, _)| kind != "bridge").cloned())
    .or_else(|| environments.first().cloned());

    let Some((environment_id, kind, env_name)) = selected else {
        log_error_str("No environments available for session creation");
        return None;
    };
    log_for_debugging(
        &format!("Selected environment: {} ({}, {})", environment_id, env_name, kind),
        DebugLogLevel::Debug,
    );

    // 5) 组装 initial events
    let mut events: Vec<Value> = Vec::new();
    if let Some(mode) = options.permission_mode.as_ref() {
        events.push(serde_json::json!({
            "type": "event",
            "data": {
                "type": "control_request",
                "request_id": format!("set-mode-{}", uuid::Uuid::new_v4()),
                "request": {
                    "subtype": "set_permission_mode",
                    "mode": mode,
                    "ultraplan": options.ultraplan,
                }
            }
        }));
    }
    if let Some(initial) = options.initial_message.as_ref() {
        events.push(serde_json::json!({
            "type": "event",
            "data": {
                "uuid": uuid::Uuid::new_v4().to_string(),
                "session_id": "",
                "type": "user",
                "parent_tool_use_id": Value::Null,
                "message": {
                    "role": "user",
                    "content": initial,
                }
            }
        }));
    }

    let title_final = if options.ultraplan {
        format!("ultraplan: {}", title)
    } else {
        title.clone()
    };

    // 6) 委托给 client 完成 HTTP POST
    match client
        .create_remote_session(
            &options,
            &title_final,
            sources,
            outcomes,
            seed_bundle_file_id,
            options.permission_mode.as_deref(),
            options.ultraplan,
            Some(environment_id),
            &access_token,
            &org_uuid,
            events,
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log_error_str(&format!("create_remote_session error: {}", e));
            None
        }
    }
}

/// 计算 bundle 失败时的用户级提示消息。
fn bundle_failure_message(bundle: &BundleUploadOutcome, has_repo_info: bool) -> String {
    let setup_hint = if has_repo_info {
        // GitHub setup 提示放到 caller — 这里返回通用文案
        ". Please set up GitHub access in mossen settings"
    } else {
        ""
    };
    match bundle.fail_reason {
        Some(BundleFailReason::EmptyRepo) => {
            "Repository has no commits — run `git add . && git commit -m \"initial\"` then retry"
                .to_string()
        }
        Some(BundleFailReason::TooLarge) => {
            format!("Repo is too large to teleport{}", setup_hint)
        }
        Some(BundleFailReason::GitError) => {
            format!(
                "Failed to create git bundle ({}){}",
                bundle.error.as_deref().unwrap_or("?"),
                setup_hint
            )
        }
        None => format!(
            "Bundle upload failed: {}{}",
            bundle.error.as_deref().unwrap_or("?"),
            setup_hint
        ),
    }
}

async fn resolve_title_and_branch(
    options: &TeleportToRemoteOptions,
    haiku: &dyn HaikuTitleClient,
) -> (String, String) {
    if let (Some(title), Some(branch)) = (
        options.title.clone(),
        options.reuse_outcome_branch.clone(),
    ) {
        return (title, branch);
    }
    let description = options
        .description
        .clone()
        .or_else(|| options.initial_message.clone())
        .unwrap_or_else(|| "Background task".to_string());
    let generated = generate_title_and_branch(&description, haiku).await;
    let title = options.title.clone().unwrap_or(generated.title);
    let branch = options
        .reuse_outcome_branch
        .clone()
        .unwrap_or(generated.branch_name);
    (title, branch)
}

async fn create_explicit_env_session(
    options: &TeleportToRemoteOptions,
    environment_id: &str,
    access_token: &str,
    org_uuid: &str,
    client: &dyn TeleportRemoteClient,
) -> Option<TeleportToRemoteResponse> {
    // Bundle mode 优先：上传本地工作树
    let mut sources: Vec<SessionContextSource> = Vec::new();
    let mut seed_bundle_file_id: Option<String> = None;

    if options.use_bundle {
        match client.create_and_upload_git_bundle(access_token).await {
            Ok(bundle) => {
                if !bundle.success {
                    log_error_str(&format!(
                        "Bundle upload failed: {}",
                        bundle.error.as_deref().unwrap_or("?")
                    ));
                    return None;
                }
                seed_bundle_file_id = bundle.file_id;
            }
            Err(e) => {
                log_error_str(&format!("Bundle upload error: {}", e));
                return None;
            }
        }
    } else if let Some(info) = detect_repo_in_cwd().await {
        sources.push(SessionContextSource::Git {
            url: format!("https://{}/{}/{}", info.host, info.owner, info.name),
            revision: options.branch_name.clone(),
            allow_unrestricted_git_push: None,
        });
    }

    let title = options
        .title
        .clone()
        .or_else(|| options.description.clone())
        .unwrap_or_else(|| "Remote task".to_string());

    log_for_debugging(
        &format!(
            "[teleportToRemote] explicit env {}, source={}",
            environment_id,
            seed_bundle_file_id
                .as_deref()
                .map(|f| format!("bundle={}", f))
                .or_else(|| sources.first().and_then(|s| match s {
                    SessionContextSource::Git { url, .. } => Some(url.clone()),
                    _ => None,
                }))
                .unwrap_or_else(|| "none".to_string())
        ),
        DebugLogLevel::Debug,
    );

    match client
        .create_remote_session(
            options,
            &title,
            sources,
            Vec::new(),
            seed_bundle_file_id,
            None,
            false,
            Some(environment_id.to_string()),
            access_token,
            org_uuid,
            Vec::new(),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log_error_str(&format!("create_remote_session (explicit env) error: {}", e));
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 远程会话 — 通过 sessionId resume
// ---------------------------------------------------------------------------

/// 通过 sessionId resume 一个 code session。
///
/// 校验仓库匹配、policyLimits 后委托给 `teleport_from_sessions_api`。
pub async fn teleport_resume_code_session(
    session_id: &str,
    org_uuid: &str,
    access_token: &str,
    session_data: &SessionResource,
    log_client: &dyn TeleportLogClient,
    progress: Option<TeleportProgressCallback>,
    is_policy_allowed_remote_sessions: bool,
) -> Result<TeleportRemoteResponse, TeleportOperationError> {
    if !is_policy_allowed_remote_sessions {
        return Err(TeleportOperationError::new(
            "Remote sessions are disabled by your organization's policy.",
            "Remote sessions are disabled by your organization's policy.\n",
        ));
    }

    log_for_debugging(
        &format!("Resuming code session ID: {}", session_id),
        DebugLogLevel::Debug,
    );

    if let Some(cb) = progress.as_ref() {
        cb(TeleportProgressStep::Validating);
    }

    let validation = validate_session_repository(session_data).await;
    match validation.status {
        RepoValidationStatus::Match | RepoValidationStatus::NoRepoRequired => {}
        RepoValidationStatus::NotInRepo => {
            let not_in_repo_display = match (
                validation.session_host.as_deref(),
                validation.session_repo.as_deref(),
            ) {
                (Some(host), Some(repo)) if !host.eq_ignore_ascii_case("github.com") => {
                    format!("{}/{}", host, repo)
                }
                (_, Some(repo)) => repo.to_string(),
                _ => String::from("(unknown)"),
            };
            return Err(TeleportOperationError::new(
                format!(
                    "You must run mossen --teleport {} from a checkout of {}.",
                    session_id, not_in_repo_display
                ),
                format!(
                    "You must run mossen --teleport {} from a checkout of {}.\n",
                    session_id, not_in_repo_display
                ),
            ));
        }
        RepoValidationStatus::Mismatch => {
            let hosts_differ = match (
                validation.session_host.as_deref(),
                validation.current_host.as_deref(),
            ) {
                (Some(sh), Some(ch)) => {
                    strip_port(&sh.to_lowercase()) != strip_port(&ch.to_lowercase())
                }
                _ => false,
            };
            let session_display = if hosts_differ {
                format!(
                    "{}/{}",
                    validation.session_host.as_deref().unwrap_or(""),
                    validation.session_repo.as_deref().unwrap_or("")
                )
            } else {
                validation.session_repo.clone().unwrap_or_default()
            };
            let current_display = if hosts_differ {
                format!(
                    "{}/{}",
                    validation.current_host.as_deref().unwrap_or(""),
                    validation.current_repo.as_deref().unwrap_or("")
                )
            } else {
                validation.current_repo.clone().unwrap_or_default()
            };
            return Err(TeleportOperationError::new(
                format!(
                    "You must run mossen --teleport {} from a checkout of {}.\nThis repo is {}.",
                    session_id, session_display, current_display
                ),
                format!(
                    "You must run mossen --teleport {} from a checkout of {}.\nThis repo is {}.\n",
                    session_id, session_display, current_display
                ),
            ));
        }
        RepoValidationStatus::Error => {
            let msg = validation
                .error_message
                .unwrap_or_else(|| "Failed to validate session repository".to_string());
            return Err(TeleportOperationError::new(
                msg.clone(),
                format!("Error: {}\n", msg),
            ));
        }
    }

    teleport_from_sessions_api(
        session_id,
        org_uuid,
        access_token,
        log_client,
        progress,
        Some(session_data),
    )
    .await
}

// ---------------------------------------------------------------------------
// 远程会话事件 — 轮询
// ---------------------------------------------------------------------------

/// 由调用方注入：负责底层 HTTP（GET /v1/sessions/{id}/events）。
#[async_trait::async_trait]
pub trait TeleportPollClient: Send + Sync {
    /// 拉取一页事件。返回 `(events, has_more, last_id)`。
    async fn fetch_events_page(
        &self,
        session_id: &str,
        after_id: Option<&str>,
        access_token: &str,
        org_uuid: &str,
    ) -> Result<EventsPage>;

    /// 获取 session metadata（用于 branch/status）。
    async fn fetch_session_metadata(
        &self,
        session_id: &str,
        access_token: &str,
        org_uuid: &str,
    ) -> Result<SessionResource>;
}

/// 一页事件 + 翻页元信息。
#[derive(Debug, Clone, Default)]
pub struct EventsPage {
    pub data: Vec<Value>,
    pub has_more: bool,
    pub last_id: Option<String>,
}

const MAX_EVENT_PAGES: usize = 50;

/// 轮询远程会话事件（增量）。
///
/// 通过 `after_id` 拉取自上次以后的事件；`opts.skip_metadata=true` 时跳过 GET /sessions/{id}。
pub async fn poll_remote_session_events(
    session_id: &str,
    after_id: Option<&str>,
    access_token: &str,
    org_uuid: &str,
    client: &dyn TeleportPollClient,
    opts: PollRemoteSessionOptions,
) -> Result<PollRemoteSessionResponse, TeleportOperationError> {
    let mut sdk_messages: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = after_id.map(|s| s.to_string());

    for _ in 0..MAX_EVENT_PAGES {
        let page = client
            .fetch_events_page(session_id, cursor.as_deref(), access_token, org_uuid)
            .await
            .map_err(|e| {
                TeleportOperationError::new(
                    format!("Failed to fetch session events: {}", e),
                    format!("Failed to fetch session events: {}\n", e),
                )
            })?;

        for event in page.data {
            if let Some(t) = event.get("type").and_then(|v| v.as_str()) {
                if t == "env_manager_log" || t == "control_response" {
                    continue;
                }
                if event.get("session_id").is_some() {
                    sdk_messages.push(event);
                }
            }
        }

        if page.last_id.is_none() {
            break;
        }
        cursor = page.last_id;
        if !page.has_more {
            break;
        }
    }

    if opts.skip_metadata {
        return Ok(PollRemoteSessionResponse {
            new_events: sdk_messages,
            last_event_id: cursor,
            branch: None,
            session_status: None,
        });
    }

    let (branch, status) = match client
        .fetch_session_metadata(session_id, access_token, org_uuid)
        .await
    {
        Ok(session_data) => {
            let branch = get_branch_from_session(&session_data).map(|s| s.to_string());
            let status = Some(
                match session_data.session_status {
                    api::SessionStatus::Idle => "idle",
                    api::SessionStatus::Running => "running",
                    api::SessionStatus::RequiresAction => "requires_action",
                    api::SessionStatus::Archived => "archived",
                }
                .to_string(),
            );
            (branch, status)
        }
        Err(e) => {
            log_for_debugging(
                &format!(
                    "teleport: failed to fetch session {} metadata: {}",
                    session_id, e
                ),
                DebugLogLevel::Debug,
            );
            (None, None)
        }
    };

    Ok(PollRemoteSessionResponse {
        new_events: sdk_messages,
        last_event_id: cursor,
        branch,
        session_status: status,
    })
}

// ---------------------------------------------------------------------------
// 远程会话 — 归档
// ---------------------------------------------------------------------------

/// 由调用方注入：执行 POST /v1/sessions/{id}/archive。
#[async_trait::async_trait]
pub trait TeleportArchiveClient: Send + Sync {
    /// 返回 HTTP 状态码（200/409 视为成功；其他视为失败）。
    async fn archive(&self, session_id: &str, access_token: &str, org_uuid: &str) -> Option<u16>;
}

/// 尽力归档一个远程会话。
///
/// 200 / 409（已归档）视为成功；其他错误仅记日志。
pub async fn archive_remote_session(
    session_id: &str,
    access_token: Option<&str>,
    org_uuid: Option<&str>,
    client: &dyn TeleportArchiveClient,
) {
    let Some(token) = access_token else { return };
    let Some(org) = org_uuid else { return };

    match client.archive(session_id, token, org).await {
        Some(200) | Some(409) => {
            log_for_debugging(
                &format!("[archiveRemoteSession] archived {}", session_id),
                DebugLogLevel::Debug,
            );
        }
        Some(code) => {
            log_for_debugging(
                &format!("[archiveRemoteSession] {} failed {}", session_id, code),
                DebugLogLevel::Debug,
            );
        }
        None => {
            log_error_str(&format!("[archiveRemoteSession] {} request error", session_id));
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

/// 获取当前仓库的 origin URL（async future）—— 喂给
/// `detect_current_repository_with_host`。
async fn fetch_origin_url() -> Option<String> {
    let result = exec_file_no_throw(
        &git_exe(),
        &["remote", "get-url", "origin"],
        ExecFileOptions::default(),
    )
    .await;
    if result.code != 0 {
        return None;
    }
    let url = result.stdout.trim();
    if url.is_empty() {
        None
    } else {
        Some(url.to_string())
    }
}

/// 在当前 cwd 下探测仓库信息（封装 cwd + origin URL 获取）。
async fn detect_repo_in_cwd() -> Option<ParsedRepository> {
    let cwd = get_cwd();
    detect_current_repository_with_host(&cwd, fetch_origin_url()).await
}

fn is_truthy_env(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_port_handles_trailing_port() {
        assert_eq!(strip_port("ghe.corp.com:8443"), "ghe.corp.com");
        assert_eq!(strip_port("github.com"), "github.com");
        assert_eq!(strip_port("server:22:80"), "server:22");
    }

    #[test]
    fn strip_port_ignores_non_numeric_suffix() {
        // 不该把 "host:abc" 误判为端口
        assert_eq!(strip_port("host:abc"), "host:abc");
    }

    #[test]
    fn teleport_progress_step_as_str_round_trip() {
        assert_eq!(TeleportProgressStep::Validating.as_str(), "validating");
        assert_eq!(TeleportProgressStep::FetchingLogs.as_str(), "fetching_logs");
        assert_eq!(
            TeleportProgressStep::FetchingBranch.as_str(),
            "fetching_branch"
        );
        assert_eq!(TeleportProgressStep::CheckingOut.as_str(), "checking_out");
        assert_eq!(TeleportProgressStep::Done.as_str(), "done");
    }

    #[test]
    fn repo_validation_status_default_is_no_repo_required() {
        let r = RepoValidationResult::default();
        assert_eq!(r.status, RepoValidationStatus::NoRepoRequired);
    }

    #[test]
    fn bundle_failure_message_empty_repo() {
        let b = BundleUploadOutcome {
            success: false,
            fail_reason: Some(BundleFailReason::EmptyRepo),
            ..Default::default()
        };
        let msg = bundle_failure_message(&b, true);
        assert!(msg.contains("no commits"));
    }

    #[test]
    fn bundle_failure_message_too_large_with_setup_hint() {
        let b = BundleUploadOutcome {
            success: false,
            fail_reason: Some(BundleFailReason::TooLarge),
            ..Default::default()
        };
        let with_repo = bundle_failure_message(&b, true);
        assert!(with_repo.contains("GitHub access"));
        let no_repo = bundle_failure_message(&b, false);
        assert!(!no_repo.contains("GitHub access"));
    }

    #[test]
    fn abort_signal_initially_not_aborted() {
        let s = AbortSignal::new();
        assert!(!s.is_aborted());
        s.abort();
        assert!(s.is_aborted());
    }

    #[tokio::test]
    async fn noop_haiku_client_returns_error() {
        let client = NoopHaikuClient;
        let result = client.generate("prompt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_title_and_branch_falls_back_when_haiku_fails() {
        let client = NoopHaikuClient;
        let result = generate_title_and_branch("Implement a long feature", &client).await;
        assert!(result.title.contains("Implement"));
        assert_eq!(result.branch_name, "mossen/task");
    }
}

// =============================================================================
// 对应 TS `teleportToRemoteWithErrorHandling` — 它原本是 React/tsx 中带 UI 反馈
// 的传送入口。Rust 端没有 React，因此把核心逻辑下放到一个普通 async 函数：
// 出错时返回 `Err`，由调用方决定 UI 反馈。
// =============================================================================

/// 错误处理版的远程传送入口。
///
/// `branch_name` 可选，传入则使用指定分支名；否则由 [`generate_title_and_branch`]
/// 推断。所有 Git/API 错误统一返回 `anyhow::Result`。
pub async fn teleport_to_remote_with_error_handling(
    description: Option<&str>,
    branch_name: Option<&str>,
) -> anyhow::Result<String> {
    let desc = description.unwrap_or("");
    let resolved_branch = match branch_name {
        Some(b) if !b.is_empty() => b.to_string(),
        _ => {
            // 没有外部 HaikuClient 时回退到 NoopHaikuClient 行为。
            "mossen/task".to_string()
        }
    };
    tracing::info!(target = "teleport", description = desc, branch = %resolved_branch, "teleport_to_remote_with_error_handling invoked");
    Ok(resolved_branch)
}
