//! # gh_pr_status — GitHub PR 状态查询
//!
//! 对应 TypeScript `utils/ghPrStatus.ts`。
//! 使用 `gh pr view` 获取当前分支的 PR 状态。

use std::time::Duration;

use crate::exec_file_no_throw::{exec_file_no_throw, ExecFileOptions};

/// PR 审查状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrReviewState {
    Approved,
    Pending,
    ChangesRequested,
    Draft,
    Merged,
    Closed,
}

impl PrReviewState {
    /// 转换为字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Pending => "pending",
            Self::ChangesRequested => "changes_requested",
            Self::Draft => "draft",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }
}

/// PR 状态信息
#[derive(Debug, Clone)]
pub struct PrStatus {
    pub number: u64,
    pub url: String,
    pub review_state: PrReviewState,
}

/// gh 命令超时时间
const GH_TIMEOUT: Duration = Duration::from_secs(5);

/// 从 GitHub API 值推导审查状态。
///
/// Draft PR 始终显示为 'draft'，无论 reviewDecision 如何。
/// reviewDecision 可以是: APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED, 或空字符串。
pub fn derive_review_state(is_draft: bool, review_decision: &str) -> PrReviewState {
    if is_draft {
        return PrReviewState::Draft;
    }
    match review_decision {
        "APPROVED" => PrReviewState::Approved,
        "CHANGES_REQUESTED" => PrReviewState::ChangesRequested,
        _ => PrReviewState::Pending,
    }
}

/// 获取当前 git 分支名
async fn get_current_branch() -> Option<String> {
    let result = exec_file_no_throw(
        "git",
        &["rev-parse", "--abbrev-ref", "HEAD"],
        ExecFileOptions {
            timeout: GH_TIMEOUT,
            ..Default::default()
        },
    )
    .await;
    if result.code == 0 {
        let branch = result.stdout.trim().to_string();
        if branch.is_empty() {
            None
        } else {
            Some(branch)
        }
    } else {
        None
    }
}

/// 获取默认分支名
async fn get_default_branch_name() -> Option<String> {
    // Try git symbolic-ref refs/remotes/origin/HEAD
    let result = exec_file_no_throw(
        "git",
        &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"],
        ExecFileOptions {
            timeout: GH_TIMEOUT,
            ..Default::default()
        },
    )
    .await;
    if result.code == 0 {
        let branch = result.stdout.trim().to_string();
        // Strip "origin/" prefix
        let branch = branch.strip_prefix("origin/").unwrap_or(&branch);
        if !branch.is_empty() {
            return Some(branch.to_string());
        }
    }
    // Fallback: check if main or master exists
    Some("main".to_string())
}

/// 获取当前分支的 PR 状态。
///
/// 在任何失败时返回 None（gh 未安装、无 PR、不在 git 仓库等）。
/// 如果 PR 的 head 分支是默认分支（如 main/master），也返回 None。
pub async fn fetch_pr_status() -> Option<PrStatus> {
    // Check if we're in a git repo
    let check = exec_file_no_throw(
        "git",
        &["rev-parse", "--is-inside-work-tree"],
        ExecFileOptions {
            timeout: GH_TIMEOUT,
            ..Default::default()
        },
    )
    .await;
    if check.code != 0 {
        return None;
    }

    // Skip on the default branch — `gh pr view` returns the most recently
    // merged PR there, which is misleading.
    let (branch, default_branch) = tokio::join!(get_current_branch(), get_default_branch_name());
    let branch = branch?;
    let default_branch = default_branch?;
    if branch == default_branch {
        return None;
    }

    let result = exec_file_no_throw(
        "gh",
        &[
            "pr",
            "view",
            "--json",
            "number,url,reviewDecision,isDraft,headRefName,state",
        ],
        ExecFileOptions {
            timeout: GH_TIMEOUT,
            preserve_output_on_error: false,
            ..Default::default()
        },
    )
    .await;

    if result.code != 0 || result.stdout.trim().is_empty() {
        return None;
    }

    let data: serde_json::Value = serde_json::from_str(&result.stdout).ok()?;

    let number = data.get("number")?.as_u64()?;
    let url = data.get("url")?.as_str()?.to_string();
    let review_decision = data
        .get("reviewDecision")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_draft = data
        .get("isDraft")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let head_ref_name = data
        .get("headRefName")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let state = data.get("state").and_then(|v| v.as_str()).unwrap_or("");

    // Don't show PR status for PRs from the default branch
    if head_ref_name == default_branch || head_ref_name == "main" || head_ref_name == "master" {
        return None;
    }

    // Don't show PR status for merged or closed PRs
    if state == "MERGED" || state == "CLOSED" {
        return None;
    }

    Some(PrStatus {
        number,
        url,
        review_state: derive_review_state(is_draft, review_decision),
    })
}
