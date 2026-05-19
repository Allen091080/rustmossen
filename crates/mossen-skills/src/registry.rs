//! # registry — 技能注册表
//!
//! 对应 TypeScript `skills/loadSkillsDir.ts` 中的全局 skill 状态管理。
//! 提供 `CraftRegistry`：统一管理捆绑技能、磁盘技能、动态发现技能和条件技能。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use tracing::debug;

use crate::skill::CraftCommand;

// ---------------------------------------------------------------------------
// Signal — 简单的订阅/发射机制
// ---------------------------------------------------------------------------

type Callback = Box<dyn Fn() + Send + Sync>;

/// 简易信号系统，对应 TS `createSignal()`。
struct Signal {
    listeners: Vec<Callback>,
}

impl Signal {
    fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    fn subscribe(&mut self, callback: Callback) {
        self.listeners.push(callback);
    }

    fn emit(&self) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| listener())) {
                tracing::error!("Signal listener panicked: {:?}", e);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CraftRegistry
// ---------------------------------------------------------------------------

/// 技能注册表 — 统一管理所有技能来源。
///
/// 对应 TS 中 `loadSkillsDir.ts` 里散落的全局状态：
/// - `bundledSkills` → `self.bundled`
/// - `dynamicSkills` → `self.dynamic`
/// - `conditionalSkills` → `self.conditional`
/// - `activatedConditionalSkillNames` → `self.activated_conditional_names`
/// - `dynamicSkillDirs` → `self.discovered_dirs`
pub struct CraftRegistry {
    /// 磁盘加载的技能（启动时加载的无条件技能）。
    disk_crafts: Vec<CraftCommand>,
    /// 动态发现的技能（通过文件路径发现）。
    dynamic: HashMap<String, CraftCommand>,
    /// 条件技能（带路径模式，尚未激活）。
    conditional: HashMap<String, CraftCommand>,
    /// 已激活的条件技能名称（跨缓存清除保留）。
    activated_conditional_names: HashSet<String>,
    /// 已探测的技能目录。
    discovered_dirs: HashSet<String>,
    /// 技能加载信号。
    skills_loaded: Signal,
}

impl CraftRegistry {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self {
            disk_crafts: Vec::new(),
            dynamic: HashMap::new(),
            conditional: HashMap::new(),
            activated_conditional_names: HashSet::new(),
            discovered_dirs: HashSet::new(),
            skills_loaded: Signal::new(),
        }
    }

    /// 设置磁盘加载的技能（替换现有）。
    pub fn set_disk_crafts(&mut self, crafts: Vec<CraftCommand>) {
        // 分离条件技能和无条件技能
        let mut unconditional = Vec::new();
        let mut new_conditional = Vec::new();

        for craft in crafts {
            let has_paths = craft
                .prompt_data
                .paths
                .as_ref()
                .map_or(false, |p| !p.is_empty());
            let already_activated = self.activated_conditional_names.contains(craft.name());

            if has_paths && !already_activated {
                new_conditional.push(craft);
            } else {
                unconditional.push(craft);
            }
        }

        // 存储条件技能
        for craft in new_conditional {
            self.conditional.insert(craft.name().to_string(), craft);
        }

        if !self.conditional.is_empty() {
            debug!(
                "[skills] {} conditional skills stored (activated when matching files are touched)",
                self.conditional.len()
            );
        }

        self.disk_crafts = unconditional;
    }

    /// 添加动态发现的技能。
    pub fn add_dynamic_crafts(&mut self, crafts: Vec<CraftCommand>) {
        for craft in crafts {
            self.dynamic.insert(craft.name().to_string(), craft);
        }
        self.skills_loaded.emit();
    }

    /// 标记技能目录已探测。
    pub fn mark_dir_discovered(&mut self, dir: &str) -> bool {
        self.discovered_dirs.insert(dir.to_string())
    }

    /// 检查目录是否已探测。
    pub fn is_dir_discovered(&self, dir: &str) -> bool {
        self.discovered_dirs.contains(dir)
    }

    /// 激活匹配文件路径的条件技能。
    ///
    /// 对应 TS `activateConditionalSkillsForPaths()`。
    /// 返回新激活的技能名称列表。
    pub fn activate_conditional_for_paths(
        &mut self,
        file_paths: &[&str],
        cwd: &str,
    ) -> Vec<String> {
        if self.conditional.is_empty() {
            return vec![];
        }

        let mut activated = Vec::new();
        let mut to_activate = Vec::new();

        for (name, craft) in &self.conditional {
            let patterns = match &craft.prompt_data.paths {
                Some(p) if !p.is_empty() => p,
                _ => continue,
            };

            for file_path in file_paths {
                let relative = make_relative(file_path, cwd);
                if let Some(rel) = &relative {
                    if rel.starts_with("..") || std::path::Path::new(rel).is_absolute() {
                        continue;
                    }
                    if matches_any_pattern(rel, patterns) {
                        to_activate.push(name.clone());
                        activated.push(name.clone());
                        debug!(
                            "[skills] Activated conditional skill '{}' (matched path: {})",
                            name, rel
                        );
                        break;
                    }
                }
            }
        }

        // 将激活的技能移到动态列表
        for name in &to_activate {
            if let Some(craft) = self.conditional.remove(name) {
                self.dynamic.insert(name.clone(), craft);
                self.activated_conditional_names.insert(name.clone());
            }
        }

        if !activated.is_empty() {
            self.skills_loaded.emit();
        }

        activated
    }

    /// 获取所有可用技能（磁盘 + 动态）。
    pub fn all_crafts(&self) -> Vec<&CraftCommand> {
        let mut result: Vec<&CraftCommand> = self.disk_crafts.iter().collect();
        result.extend(self.dynamic.values());
        result
    }

    /// 获取动态发现的技能。
    pub fn dynamic_crafts(&self) -> Vec<&CraftCommand> {
        self.dynamic.values().collect()
    }

    /// 获取条件技能数量。
    pub fn conditional_count(&self) -> usize {
        self.conditional.len()
    }

    /// 注册技能加载回调。
    pub fn on_skills_loaded(&mut self, callback: impl Fn() + Send + Sync + 'static) {
        self.skills_loaded.subscribe(Box::new(callback));
    }

    /// 清除缓存（保留 activated_conditional_names）。
    pub fn clear_caches(&mut self) {
        self.disk_crafts.clear();
        self.conditional.clear();
    }

    /// 清除所有动态状态（测试用）。
    pub fn clear_dynamic(&mut self) {
        self.discovered_dirs.clear();
        self.dynamic.clear();
        self.conditional.clear();
        self.activated_conditional_names.clear();
    }
}

impl Default for CraftRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 线程安全的共享注册表
// ---------------------------------------------------------------------------

/// 线程安全的共享技能注册表。
pub type SharedCraftRegistry = Arc<RwLock<CraftRegistry>>;

/// 创建新的共享注册表。
pub fn new_shared_registry() -> SharedCraftRegistry {
    Arc::new(RwLock::new(CraftRegistry::new()))
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 将绝对路径转为相对路径。
fn make_relative(file_path: &str, cwd: &str) -> Option<String> {
    if std::path::Path::new(file_path).is_absolute() {
        let fp = std::path::Path::new(file_path);
        let base = std::path::Path::new(cwd);
        pathdiff_relative(fp, base)
    } else {
        Some(file_path.to_string())
    }
}

/// 简易的 path diff（不依赖 pathdiff crate）。
fn pathdiff_relative(path: &std::path::Path, base: &std::path::Path) -> Option<String> {
    let path_str = path.to_str()?;
    let base_str = base.to_str()?;
    let base_with_sep = if base_str.ends_with(std::path::MAIN_SEPARATOR) {
        base_str.to_string()
    } else {
        format!("{}{}", base_str, std::path::MAIN_SEPARATOR)
    };
    if path_str.starts_with(&base_with_sep) {
        Some(path_str[base_with_sep.len()..].to_string())
    } else {
        None
    }
}

/// 简易 glob 匹配（支持 * 和 ** 模式）。
fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if glob_matches(pattern, path) {
            return true;
        }
    }
    false
}

/// 简易 glob 匹配。
fn glob_matches(pattern: &str, path: &str) -> bool {
    if pattern == "**" || pattern == "*" {
        return true;
    }

    // 精确匹配
    if pattern == path {
        return true;
    }

    // 前缀匹配（目录模式）
    if path.starts_with(pattern) && path[pattern.len()..].starts_with('/') {
        return true;
    }

    // 尝试用 regex 做简单的 glob
    let regex_pattern = pattern
        .replace('.', "\\.")
        .replace("**", "§§")
        .replace('*', "[^/]*")
        .replace("§§", ".*");
    regex::Regex::new(&format!("^{}$", regex_pattern))
        .ok()
        .map_or(false, |re| re.is_match(path))
}
