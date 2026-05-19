//! # undercover — 卧底模式安全工具
//!
//! 对应 TypeScript `utils/undercover.ts`。
//!
//! 当激活时，Mossen 在提交/PR 提示中添加安全指令并剥离所有归属信息，
//! 以避免泄露内部模型代号、项目名称或其他内部信息。

use std::env;
use std::sync::OnceLock;

/// 仓库分类（缓存）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoClass {
    Internal,
    External,
    None,
}

impl From<crate::commit_attribution::RepoClass> for RepoClass {
    fn from(c: crate::commit_attribution::RepoClass) -> Self {
        match c {
            crate::commit_attribution::RepoClass::Internal => RepoClass::Internal,
            crate::commit_attribution::RepoClass::External => RepoClass::External,
            crate::commit_attribution::RepoClass::None => RepoClass::None,
        }
    }
}

/// 进程内仓库分类缓存。TS 端在 `commitAttribution.ts` 中通过
/// `repoClassCache` 单例缓存；Rust 用 `OnceLock` 等价。
static REPO_CLASS_CACHE: OnceLock<RepoClass> = OnceLock::new();

/// 测试/手动 prime（与 TS `primeRepoClassCache` 等价）；同样允许通过
/// `MOSSEN_REPO_CLASS` 环境变量覆盖（保留旧行为，便于测试和 CI）。
pub fn prime_repo_class_cache(remote_url: Option<&str>) {
    let _ = REPO_CLASS_CACHE.set(
        crate::commit_attribution::classify_repo(remote_url).into(),
    );
}

/// 获取缓存的仓库分类；缺省按 `MOSSEN_REPO_CLASS` env 回落到 `None`。
fn get_repo_class_cached() -> RepoClass {
    if let Some(cached) = REPO_CLASS_CACHE.get() {
        return cached.clone();
    }
    match env::var("MOSSEN_REPO_CLASS").as_deref() {
        Ok("internal") => RepoClass::Internal,
        Ok("external") => RepoClass::External,
        _ => RepoClass::None,
    }
}

/// 获取全局配置中 `hasSeenUndercoverAutoNotice` 标志
fn get_has_seen_undercover_auto_notice() -> bool {
    env::var("MOSSEN_SEEN_UNDERCOVER_NOTICE").as_deref() == Ok("1")
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}

/// 检查是否处于卧底模式
pub fn is_undercover() -> bool {
    if env::var("USER_TYPE").as_deref() == Ok("ant") {
        if let Ok(val) = env::var("MOSSEN_CODE_UNDERCOVER") {
            if is_env_truthy(&val) {
                return true;
            }
        }
        // 自动模式：除非已确认在允许列表的内部仓库中，否则激活
        // 'external'、'none' 和 null（检查尚未运行）都解析为 ON
        return get_repo_class_cached() != RepoClass::Internal;
    }
    false
}

/// 获取卧底模式指令
pub fn get_undercover_instructions() -> &'static str {
    if env::var("USER_TYPE").as_deref() == Ok("ant") {
        return r#"## UNDERCOVER MODE — CRITICAL

You are operating UNDERCOVER in a PUBLIC/OPEN-SOURCE repository. Your commit
messages, PR titles, and PR bodies MUST NOT contain ANY Mossen-internal
information. Do not blow your cover.

NEVER include in commit messages or PR descriptions:
- Internal model codenames (animal names like Capybara, Tengu, etc.)
- Unreleased model version numbers (e.g., opus-4-7, sonnet-4-8)
- Internal repo or project names (e.g., mossen-cli-internal, mossen/…)
- Internal tooling, Slack channels, or short links (e.g., go/cc, #mossen-code-…)
- The phrase "Mossen" or any mention that you are an AI
- Any hint of what model or version you are
- Co-Authored-By lines or any other attribution

Write commit messages as a human developer would — describe only what the code
change does.

GOOD:
- "Fix race condition in file watcher initialization"
- "Add support for custom key bindings"
- "Refactor parser for better error messages"

BAD (never write these):
- "Fix bug found while testing with internal Capybara build"
- "1-shotted by internal-opus-4-6"
- "Generated with Mossen"
- "Co-Authored-By: internal agent <…>"
"#;
    }
    ""
}

/// 检查是否应显示一次性卧底模式自动通知对话框。
/// 当以下条件满足时返回 true：卧底模式通过自动检测激活（非通过 env 强制），
/// 且用户之前未看过通知。
pub fn should_show_undercover_auto_notice() -> bool {
    if env::var("USER_TYPE").as_deref() == Ok("ant") {
        // 如果通过 env 强制，用户已经知道；不要打扰
        if let Ok(val) = env::var("MOSSEN_CODE_UNDERCOVER") {
            if is_env_truthy(&val) {
                return false;
            }
        }
        if !is_undercover() {
            return false;
        }
        if get_has_seen_undercover_auto_notice() {
            return false;
        }
        return true;
    }
    false
}
