//! # dynamic — 动态技能发现与条件激活
//!
//! 对应 TypeScript `skills/loadSkillsDir.ts` 中后半部分的动态技能 / 条件
//! 技能逻辑。提供：
//! - `get_skills_path` — 由设置作用域计算技能目录位置；
//! - `create_skill_command` — 从结构化字段构造一个 `CraftCommand`；
//! - `clear_skill_caches` / `clear_dynamic_skills` — 测试用清理；
//! - `on_dynamic_skills_loaded` — 订阅动态技能加载完成事件；
//! - `discover_skill_dirs_for_paths` — 从文件路径向上探索技能目录；
//! - `add_skill_directories` — 从给定目录加载技能并合并到动态注册表；
//! - `activate_conditional_skills_for_paths` — 根据路径模式激活条件技能。
//!
//! Rust 端把 TS 的 `dynamicSkills: Map`、`conditionalSkills: Map`、
//! `activatedConditionalSkillNames: Set` 用全局 `RwLock` 包裹的容器实现。
//! 监听器列表由 `Mutex<Vec<Arc<dyn Fn>>>` 持有，与 TS `createSignal()` 行为
//! 一致：emit 时遍历调用、subscribe 返回反注册闭包。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use mossen_types::command::{
    CommandBase, CommandLoadedFrom, EffortValue, ExecutionContext, PromptCommandData,
    PromptCommandSource,
};

use crate::config::FrontmatterShell;
use crate::loader::load_skills_from_dir;
use crate::skill::CraftCommand;

// ---------------------------------------------------------------------------
// SettingSource — 对应 TS `utils/settings/constants.ts` 的 SettingSource。
// 在 Rust 端用一个 `&str` 枚举，避免引入 cli/utils 的循环依赖。
// ---------------------------------------------------------------------------

/// `loadSkillsDir.ts` `LoadedFrom` —— 一个技能的加载来源标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadedFrom {
    CommandsDeprecated,
    Skills,
    Plugin,
    Managed,
    Bundled,
    Mcp,
}

impl LoadedFrom {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CommandsDeprecated => "commands_DEPRECATED",
            Self::Skills => "skills",
            Self::Plugin => "plugin",
            Self::Managed => "managed",
            Self::Bundled => "bundled",
            Self::Mcp => "mcp",
        }
    }
}

/// `loadSkillsDir.ts` `getSkillDirCommands` 的入口。
///
/// 在 TS 中这是一个 memoized 函数；Rust 端我们直接委托给 [`load_skills_from_dir`]，
/// 调用方应用自己的缓存（避免 `lodash-es/memoize` 的隐式全局状态）。
pub async fn get_skill_dir_commands(
    base_path: &std::path::Path,
    source: PromptCommandSource,
) -> Vec<(CraftCommand, std::path::PathBuf)> {
    crate::loader::load_skills_from_dir(base_path, source).await
}

/// 设置作用域 — 对应 TS `SettingSource`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSettingSource {
    PolicySettings,
    UserSettings,
    ProjectSettings,
    Plugin,
}

impl SkillSettingSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PolicySettings => "policySettings",
            Self::UserSettings => "userSettings",
            Self::ProjectSettings => "projectSettings",
            Self::Plugin => "plugin",
        }
    }
}

/// `loadSkillsDir.ts` `getSkillsPath`。
///
/// 计算指定作用域下技能/命令目录的路径。`policy_dir` 与 `user_home` 由调用方
/// 提供（在 Rust 中我们避免直接调用 utils 层以防循环依赖）。
///
/// 对应 TS：
/// - policySettings → `<policy_dir>/<canonical>/<dir>`
/// - userSettings → `<user_home>/<dir>`
/// - projectSettings → `.mossen/<dir>` (相对路径)
/// - plugin → `plugin` 字符串
pub fn get_skills_path(
    source: SkillSettingSource,
    dir: &str,
    policy_dir: &Path,
    user_home: &Path,
    canonical_config_dir_name: &str,
) -> String {
    match source {
        SkillSettingSource::PolicySettings => policy_dir
            .join(canonical_config_dir_name)
            .join(dir)
            .to_string_lossy()
            .into_owned(),
        SkillSettingSource::UserSettings => user_home.join(dir).to_string_lossy().into_owned(),
        SkillSettingSource::ProjectSettings => format!("{}/{}", canonical_config_dir_name, dir),
        SkillSettingSource::Plugin => "plugin".to_string(),
    }
}

// ---------------------------------------------------------------------------
// createSkillCommand — 构造 CraftCommand
// ---------------------------------------------------------------------------

/// `loadSkillsDir.ts` `createSkillCommand` 的输入字段。
#[derive(Debug, Clone)]
pub struct CreateSkillCommandInput {
    pub skill_name: String,
    pub display_name: Option<String>,
    pub description: String,
    pub has_user_specified_description: bool,
    pub markdown_content: String,
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub argument_names: Vec<String>,
    pub when_to_use: Option<String>,
    pub version: Option<String>,
    pub model: Option<String>,
    pub disable_model_invocation: bool,
    pub user_invocable: bool,
    pub source: PromptCommandSource,
    pub base_dir: Option<PathBuf>,
    pub loaded_from: CommandLoadedFrom,
    pub hooks: Option<serde_json::Value>,
    pub execution_context: Option<ExecutionContext>,
    pub agent: Option<String>,
    pub paths: Option<Vec<String>>,
    pub effort: Option<EffortValue>,
    pub shell: Option<FrontmatterShell>,
}

/// `loadSkillsDir.ts` `createSkillCommand`。
///
/// 把结构化字段聚合为一个 `CraftCommand`。Rust 端不需要 TS 中的
/// `getPromptForCommand` 闭包：调用方在执行技能时直接读取 `markdown_content`
/// 并按需通过 [`crate::skill::prepend_base_dir`] / 参数替换工具完成 prompt
/// 渲染。
pub fn create_skill_command(input: CreateSkillCommandInput) -> CraftCommand {
    let content_length = input.markdown_content.len();
    let is_hidden = !input.user_invocable;
    let base_dir_str = input.base_dir.as_ref().map(|p| p.to_string_lossy().into_owned());
    // `effort` and `shell` are accepted in the input for parity with TS but the
    // current `CraftCommand` does not have a dedicated `shell` field — shell is
    // tracked at the executor layer when the skill runs. We forward `effort`
    // through `prompt_data.effort` and keep `shell` in the markdown frontmatter
    // pipeline upstream.
    let _ = input.shell;

    CraftCommand {
        base: CommandBase {
            name: input.skill_name.clone(),
            description: input.description,
            aliases: None,
            argument_hint: input.argument_hint,
            when_to_use: input.when_to_use,
            user_invocable: Some(input.user_invocable),
            disable_model_invocation: Some(input.disable_model_invocation),
            is_hidden: Some(is_hidden),
            has_user_specified_description: Some(input.has_user_specified_description),
            loaded_from: Some(input.loaded_from),
            availability: None,
            version: input.version,
            is_mcp: None,
            kind: None,
            immediate: None,
            is_sensitive: None,
            extra: HashMap::new(),
        },
        prompt_data: PromptCommandData {
            progress_message: "running".to_string(),
            content_length,
            arg_names: if input.argument_names.is_empty() {
                None
            } else {
                Some(input.argument_names)
            },
            allowed_tools: Some(input.allowed_tools),
            model: input.model,
            source: input.source,
            plugin_info: None,
            disable_non_interactive: None,
            hooks: input.hooks,
            skill_root: base_dir_str.clone(),
            context: input.execution_context,
            agent: input.agent,
            effort: input.effort,
            paths: input.paths,
        },
        loaded_from: input.loaded_from,
        markdown_content: Some(input.markdown_content),
        skill_root: base_dir_str,
        display_name: input.display_name,
    }
}

// ---------------------------------------------------------------------------
// dynamicSkills / conditionalSkills — 全局状态
// ---------------------------------------------------------------------------

struct DynamicState {
    /// Already-checked skill dirs (used to suppress repeat stat()).
    checked_dirs: HashSet<PathBuf>,
    /// Loaded dynamic skills (name -> CraftCommand).
    skills: HashMap<String, CraftCommand>,
    /// Skills with `paths` frontmatter still waiting for a path match.
    conditional_skills: HashMap<String, CraftCommand>,
    /// Names of conditional skills that have already been activated.
    activated_conditional_names: HashSet<String>,
}

impl DynamicState {
    fn new() -> Self {
        Self {
            checked_dirs: HashSet::new(),
            skills: HashMap::new(),
            conditional_skills: HashMap::new(),
            activated_conditional_names: HashSet::new(),
        }
    }
}

static STATE: OnceLock<RwLock<DynamicState>> = OnceLock::new();
static LISTENERS: OnceLock<Mutex<Vec<(usize, Arc<dyn Fn() + Send + Sync>)>>> = OnceLock::new();
static LISTENER_SEQ: OnceLock<Mutex<usize>> = OnceLock::new();

fn state() -> &'static RwLock<DynamicState> {
    STATE.get_or_init(|| RwLock::new(DynamicState::new()))
}

fn listeners() -> &'static Mutex<Vec<(usize, Arc<dyn Fn() + Send + Sync>)>> {
    LISTENERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn listener_seq() -> &'static Mutex<usize> {
    LISTENER_SEQ.get_or_init(|| Mutex::new(0))
}

fn emit_skills_loaded() {
    let snapshot: Vec<Arc<dyn Fn() + Send + Sync>> = {
        listeners()
            .lock()
            .unwrap()
            .iter()
            .map(|(_, cb)| cb.clone())
            .collect()
    };
    for cb in snapshot {
        // call_safe equivalent — swallow any panics to keep parity with TS
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb()));
    }
}

/// `loadSkillsDir.ts` `onDynamicSkillsLoaded`。
///
/// 注册一个动态技能加载完成的回调，返回反注册闭包（调用一次后再次调用
/// 等价 no-op，对应 TS `signal.subscribe` 返回的取消订阅函数）。
pub fn on_dynamic_skills_loaded<F>(cb: F) -> Box<dyn Fn() + Send + Sync>
where
    F: Fn() + Send + Sync + 'static,
{
    let mut seq = listener_seq().lock().unwrap();
    *seq += 1;
    let id = *seq;
    drop(seq);

    listeners().lock().unwrap().push((id, Arc::new(cb)));

    Box::new(move || {
        let mut list = listeners().lock().unwrap();
        list.retain(|(lid, _)| *lid != id);
    })
}

/// `loadSkillsDir.ts` `clearSkillCaches`。
///
/// 清除条件技能等内部缓存。dynamic skills 本身保留 — 与 TS 一致。
pub fn clear_skill_caches() {
    let mut s = state().write().unwrap();
    s.conditional_skills.clear();
    s.activated_conditional_names.clear();
}

/// `loadSkillsDir.ts` `clearDynamicSkills`。
pub fn clear_dynamic_skills() {
    let mut s = state().write().unwrap();
    s.checked_dirs.clear();
    s.skills.clear();
    s.conditional_skills.clear();
    s.activated_conditional_names.clear();
}

/// `loadSkillsDir.ts` `getDynamicSkills`。
pub fn get_dynamic_skills() -> Vec<CraftCommand> {
    state().read().unwrap().skills.values().cloned().collect()
}

/// `loadSkillsDir.ts` `getConditionalSkillsCount`。
pub fn get_conditional_skills_count() -> usize {
    state().read().unwrap().conditional_skills.len()
}

/// `loadSkillsDir.ts` `getConditionalSkillCount` — 同名别名（TS 中两个都存在）。
pub fn get_conditional_skill_count() -> usize {
    get_conditional_skills_count()
}

/// `loadSkillsDir.ts` `discoverSkillDirsForPaths`。
///
/// 从每个 file_path 的父目录向 cwd 方向回溯（不包含 cwd 自身），寻找
/// `<canonical>/skills` 目录。已检查过的目录不会再次 stat。
pub async fn discover_skill_dirs_for_paths(
    file_paths: &[PathBuf],
    cwd: &Path,
    canonical_config_dir_name: &str,
) -> Vec<PathBuf> {
    let resolved_cwd = strip_trailing_sep(cwd);
    let mut new_dirs: Vec<PathBuf> = Vec::new();

    for file_path in file_paths {
        let mut current_dir = file_path.parent().map(Path::to_path_buf);

        while let Some(dir) = current_dir.clone() {
            // continue only while dir is strictly under cwd
            if !is_strictly_under(&dir, &resolved_cwd) {
                break;
            }

            let skill_dir = dir.join(canonical_config_dir_name).join("skills");

            let already_checked = {
                let mut s = state().write().unwrap();
                if s.checked_dirs.contains(&skill_dir) {
                    true
                } else {
                    s.checked_dirs.insert(skill_dir.clone());
                    false
                }
            };

            if !already_checked && tokio::fs::metadata(&skill_dir).await.is_ok() {
                new_dirs.push(skill_dir);
            }

            let parent = dir.parent().map(Path::to_path_buf);
            if parent.as_ref() == Some(&dir) || parent.is_none() {
                break;
            }
            current_dir = parent;
        }
    }

    // Sort by path depth (deepest first).
    new_dirs.sort_by(|a, b| {
        let da = path_depth(a);
        let db = path_depth(b);
        db.cmp(&da)
    });
    new_dirs
}

fn strip_trailing_sep(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s.ends_with(MAIN_SEPARATOR) {
        PathBuf::from(&s[..s.len() - 1])
    } else {
        p.to_path_buf()
    }
}

fn is_strictly_under(child: &Path, parent: &Path) -> bool {
    let cs = child.to_string_lossy();
    let ps = parent.to_string_lossy();
    let needle = format!("{}{}", ps, MAIN_SEPARATOR);
    cs.starts_with(needle.as_str())
}

fn path_depth(p: &Path) -> usize {
    p.components().count()
}

/// `loadSkillsDir.ts` `addSkillDirectories`。
///
/// 从目录列表加载技能并合并到 dynamic skills 中（深目录优先覆盖）。
/// 完成后触发 [`on_dynamic_skills_loaded`] 监听器。
pub async fn add_skill_directories(dirs: &[PathBuf]) -> usize {
    if dirs.is_empty() {
        return 0;
    }

    // Load all directories concurrently.
    let mut all_loaded: Vec<Vec<CraftCommand>> = Vec::with_capacity(dirs.len());
    for dir in dirs {
        let loaded = load_skills_from_dir(dir, PromptCommandSource::ProjectSettings).await;
        all_loaded.push(loaded.into_iter().map(|(cmd, _)| cmd).collect());
    }

    // shallower first, so deeper overrides
    let mut added: usize = 0;
    {
        let mut s = state().write().unwrap();
        for skills in all_loaded.iter().rev() {
            for skill in skills {
                let name = skill.base.name.clone();
                if s.skills.insert(name.clone(), skill.clone()).is_none() {
                    added += 1;
                }
            }
        }
    }

    if added > 0 {
        emit_skills_loaded();
    }
    added
}

/// `loadSkillsDir.ts` `activateConditionalSkillsForPaths`。
///
/// 根据路径模式激活待定的条件技能（带 `paths` frontmatter）。返回新激活
/// 的技能名列表。Rust 端用简化的 glob 匹配：把每个模式当作 `glob::Pattern`
/// 处理；若不能编译则跳过该模式（fail-soft，与 TS 的 `ignore()` 健壮性
/// 一致）。
pub fn activate_conditional_skills_for_paths(file_paths: &[String], cwd: &Path) -> Vec<String> {
    let mut activated = Vec::new();

    // Snapshot patterns under read lock first.
    let candidate_patterns: Vec<(String, Vec<String>, CraftCommand)> = {
        let s = state().read().unwrap();
        if s.conditional_skills.is_empty() {
            return activated;
        }
        s.conditional_skills
            .iter()
            .filter_map(|(name, skill)| {
                skill
                    .prompt_data
                    .paths
                    .as_ref()
                    .filter(|p| !p.is_empty())
                    .map(|p| (name.clone(), p.clone(), skill.clone()))
            })
            .collect()
    };

    for (name, patterns, skill) in candidate_patterns {
        for fp in file_paths {
            let rel: String = if Path::new(fp).is_absolute() {
                match pathdiff_relative(Path::new(fp), cwd) {
                    Some(r) => r.to_string_lossy().into_owned(),
                    None => continue,
                }
            } else {
                fp.clone()
            };
            if rel.is_empty() || rel.starts_with("..") || Path::new(&rel).is_absolute() {
                continue;
            }
            if matches_patterns_with_negation(&patterns, &rel) {
                let mut s = state().write().unwrap();
                s.skills.insert(name.clone(), skill.clone());
                s.conditional_skills.remove(&name);
                s.activated_conditional_names.insert(name.clone());
                activated.push(name.clone());
                break;
            }
        }
    }

    if !activated.is_empty() {
        emit_skills_loaded();
    }
    activated
}

fn pathdiff_relative(target: &Path, base: &Path) -> Option<PathBuf> {
    let target_str = target.to_string_lossy().into_owned();
    let base_str = base.to_string_lossy().into_owned();
    let base_with_sep = if base_str.ends_with(MAIN_SEPARATOR) {
        base_str.clone()
    } else {
        format!("{}{}", base_str, MAIN_SEPARATOR)
    };
    target_str
        .strip_prefix(base_with_sep.as_str())
        .map(PathBuf::from)
}

/// Gitignore-style pattern list matcher — supports `**`, `*`, `?`, and the
/// leading `!` negation operator from TS `ignore` semantics:
///
///   * Patterns are evaluated top-to-bottom; later matches override earlier
///     ones (same as `.gitignore`).
///   * A non-negated pattern that matches → flips state to `included`.
///   * A `!`-prefixed pattern that matches → flips state to `excluded`.
///   * Final state is returned. Initial state is `excluded`.
///
/// Implemented via `globset::Glob` so the actual matching honours the same
/// glob grammar the TS port relies on (no hand-rolled regex translation).
fn matches_patterns_with_negation(patterns: &[String], candidate: &str) -> bool {
    let mut included = false;
    for raw in patterns {
        let (negate, pat) = match raw.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, raw.as_str()),
        };
        let Ok(glob) = globset::GlobBuilder::new(pat)
            .literal_separator(true)
            .build()
        else { continue };
        if glob.compile_matcher().is_match(candidate) {
            included = !negate;
        }
    }
    included
}

/// Single-pattern matcher used by the existing tests. Wraps the negation-
/// aware helper so legacy call sites keep working.
fn simple_glob_match(pattern: &str, candidate: &str) -> bool {
    matches_patterns_with_negation(&[pattern.to_string()], candidate)
}

/// 给测试与高层注入：把一个条件技能（带 paths）添加到候选池中。
pub fn add_conditional_skill(skill: CraftCommand) {
    let mut s = state().write().unwrap();
    s.conditional_skills.insert(skill.base.name.clone(), skill);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_basic_and_double_star() {
        assert!(simple_glob_match("src/*.ts", "src/foo.ts"));
        assert!(!simple_glob_match("src/*.ts", "src/sub/foo.ts"));
        assert!(simple_glob_match("src/**", "src/sub/foo.ts"));
        assert!(simple_glob_match("**/foo.ts", "deep/path/foo.ts"));
    }

    #[test]
    fn pathdiff_resolves_prefix() {
        let r = pathdiff_relative(Path::new("/repo/src/foo.ts"), Path::new("/repo"));
        assert_eq!(r, Some(PathBuf::from("src/foo.ts")));
    }
}
