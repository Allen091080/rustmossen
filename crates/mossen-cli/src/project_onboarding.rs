//! 项目引导状态 — 对应 TS 的 projectOnboardingState.ts。
//!
//! 管理用户首次使用项目时的引导流程步骤和完成状态。

use std::path::Path;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// 引导步骤。
#[derive(Debug, Clone)]
pub struct Step {
    pub key: String,
    pub text: String,
    pub is_complete: bool,
    pub is_completable: bool,
    pub is_enabled: bool,
}

/// 项目引导配置。
#[derive(Debug, Clone, Default)]
pub struct ProjectOnboardingConfig {
    pub has_completed_project_onboarding: bool,
    pub project_onboarding_seen_count: u32,
}

/// 产品名称信息。
#[derive(Debug, Clone)]
pub struct ProductNames {
    pub assistant_name: String,
    pub cli_name: String,
    pub instructions_display_name: String,
}

impl Default for ProductNames {
    fn default() -> Self {
        Self {
            assistant_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            instructions_display_name: "MOSSEN.md".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// 获取项目指令文件的候选路径列表。
fn get_project_instructions_read_candidates(cwd: &Path) -> Vec<std::path::PathBuf> {
    vec![
        cwd.join("MOSSEN.md"),
        cwd.join(".mossen").join("MOSSEN.md"),
        cwd.join("mossen.md"),
    ]
}

/// 检查目录是否为空。
fn is_dir_empty(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

/// 获取引导步骤列表。
pub fn get_steps(cwd: &Path, names: &ProductNames) -> Vec<Step> {
    let has_instructions_file = get_project_instructions_read_candidates(cwd)
        .iter()
        .any(|candidate| candidate.exists());
    let is_workspace_dir_empty = is_dir_empty(cwd);

    vec![
        Step {
            key: "workspace".to_string(),
            text: format!(
                "Ask {} to create a new app or clone a repository",
                names.assistant_name
            ),
            is_complete: false,
            is_completable: true,
            is_enabled: is_workspace_dir_empty,
        },
        Step {
            key: "mossenmd".to_string(),
            text: format!(
                "Run /init to create a {} project instructions file for {} ({})",
                names.instructions_display_name, names.assistant_name, names.cli_name
            ),
            is_complete: has_instructions_file,
            is_completable: true,
            is_enabled: !is_workspace_dir_empty,
        },
    ]
}

/// 检查项目引导是否已完成。
pub fn is_project_onboarding_complete(cwd: &Path, names: &ProductNames) -> bool {
    get_steps(cwd, names)
        .iter()
        .filter(|s| s.is_completable && s.is_enabled)
        .all(|s| s.is_complete)
}

/// 如果引导已完成，标记为完成。
pub fn maybe_mark_project_onboarding_complete(
    cwd: &Path,
    names: &ProductNames,
    config: &mut ProjectOnboardingConfig,
) {
    if config.has_completed_project_onboarding {
        return;
    }
    if is_project_onboarding_complete(cwd, names) {
        config.has_completed_project_onboarding = true;
    }
}

/// 判断是否应显示项目引导。
pub fn should_show_project_onboarding(
    cwd: &Path,
    names: &ProductNames,
    config: &ProjectOnboardingConfig,
    is_demo: bool,
) -> bool {
    if config.has_completed_project_onboarding
        || config.project_onboarding_seen_count >= 4
        || is_demo
    {
        return false;
    }
    !is_project_onboarding_complete(cwd, names)
}

/// 递增项目引导已查看计数。
pub fn increment_project_onboarding_seen_count(config: &mut ProjectOnboardingConfig) {
    config.project_onboarding_seen_count += 1;
}
