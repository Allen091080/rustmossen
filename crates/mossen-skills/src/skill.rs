//! # skill — 技能定义与注册
//!
//! 对应 TypeScript `skills/bundledSkills.ts`。
//! 定义 `BundledCraftDefinition`、bundled craft 注册/获取，
//! 以及文件提取（extractBundledSkillFiles）逻辑。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tracing::warn;

use mossen_types::command::{
    CommandBase, CommandLoadedFrom, CommandType, EffortValue, ExecutionContext, PromptCommandData,
    PromptCommandSource,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// 内容块参数 — 简化版，对应 TS 的 ContentBlockParam。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// 纯文本块。
    Text { text: String },
    /// 图像块。
    Image { source: String },
}

/// 捆绑技能定义 — 对应 TS `BundledSkillDefinition`。
#[derive(Debug, Clone)]
pub struct BundledCraftDefinition {
    /// 技能名称。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 别名列表。
    pub aliases: Option<Vec<String>>,
    /// 使用场景。
    pub when_to_use: Option<String>,
    /// 参数提示。
    pub argument_hint: Option<String>,
    /// 允许的工具列表。
    pub allowed_tools: Option<Vec<String>>,
    /// 指定模型。
    pub model: Option<String>,
    /// 是否禁用模型调用。
    pub disable_model_invocation: bool,
    /// 用户是否可调用。
    pub user_invocable: bool,
    /// Hooks 配置（JSON 值）。
    pub hooks: Option<serde_json::Value>,
    /// 执行上下文：内联或分叉。
    pub context: Option<ExecutionContext>,
    /// 关联的 agent 名称。
    pub agent: Option<String>,
    /// 额外引用文件（相对路径 → 内容）。
    pub files: Option<HashMap<String, String>>,
}

/// 已构建的技能命令 — 对应 TS `Command` (type: 'prompt')。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftCommand {
    /// 命令基础属性。
    pub base: CommandBase,
    /// Prompt 命令数据。
    pub prompt_data: PromptCommandData,
    /// 加载来源。
    pub loaded_from: CommandLoadedFrom,
    /// Markdown 内容（延迟加载时保存原始内容）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown_content: Option<String>,
    /// 技能根目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_root: Option<String>,
    /// 用户可见名称（displayName）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl CraftCommand {
    /// 获取用户可见名称。
    pub fn user_facing_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.base.name)
    }

    /// 获取技能名称。
    pub fn name(&self) -> &str {
        &self.base.name
    }

    /// 是否可被用户调用。
    pub fn is_user_invocable(&self) -> bool {
        self.base.user_invocable.unwrap_or(true)
    }
}

// ---------------------------------------------------------------------------
// Bundled craft registry (global, process-lifetime)
// ---------------------------------------------------------------------------

static BUNDLED_CRAFTS: RwLock<Vec<CraftCommand>> = RwLock::new(Vec::new());

/// 注册一个捆绑技能。启动时调用。
///
/// 对应 TS `registerBundledSkill(definition)`。
pub fn register_bundled_craft(definition: BundledCraftDefinition) {
    let skill_root = definition
        .files
        .as_ref()
        .filter(|f| !f.is_empty())
        .map(|_| {
            bundled_craft_extract_dir(&definition.name)
                .to_string_lossy()
                .into_owned()
        });

    let command = CraftCommand {
        base: CommandBase {
            name: definition.name.clone(),
            description: definition.description.clone(),
            aliases: definition.aliases.clone(),
            argument_hint: definition.argument_hint.clone(),
            when_to_use: definition.when_to_use.clone(),
            user_invocable: Some(definition.user_invocable),
            disable_model_invocation: Some(definition.disable_model_invocation),
            is_hidden: Some(!definition.user_invocable),
            has_user_specified_description: Some(true),
            loaded_from: Some(CommandLoadedFrom::Bundled),
            availability: None,
            is_mcp: None,
            version: None,
            kind: None,
            immediate: None,
            is_sensitive: None,
            extra: HashMap::new(),
        },
        prompt_data: PromptCommandData {
            progress_message: "running".to_string(),
            content_length: 0,
            arg_names: None,
            allowed_tools: definition.allowed_tools.clone().or_else(|| Some(vec![])),
            model: definition.model.clone(),
            source: PromptCommandSource::Bundled,
            plugin_info: None,
            disable_non_interactive: None,
            hooks: definition.hooks.clone(),
            skill_root: skill_root.clone(),
            context: definition.context,
            agent: definition.agent.clone(),
            effort: None,
            paths: None,
        },
        loaded_from: CommandLoadedFrom::Bundled,
        markdown_content: None,
        skill_root,
        display_name: None,
    };

    let mut registry = BUNDLED_CRAFTS
        .write()
        .expect("bundled crafts lock poisoned");
    if let Some(existing) = registry
        .iter_mut()
        .find(|craft| craft.base.name == command.base.name)
    {
        *existing = command;
    } else {
        registry.push(command);
    }
}

/// 获取所有已注册的捆绑技能（返回副本）。
pub fn get_bundled_crafts() -> Vec<CraftCommand> {
    let registry = BUNDLED_CRAFTS.read().expect("bundled crafts lock poisoned");
    registry.clone()
}

/// 清除捆绑技能注册表（测试用）。
pub fn clear_bundled_crafts() {
    let mut registry = BUNDLED_CRAFTS
        .write()
        .expect("bundled crafts lock poisoned");
    registry.clear();
}

// ---------------------------------------------------------------------------
// 兼容别名（与 TS `bundledSkills.ts` 函数名一一对应）
// ---------------------------------------------------------------------------

/// `bundledSkills.ts` `registerBundledSkill` 的别名。
pub fn register_bundled_skill(definition: BundledCraftDefinition) {
    register_bundled_craft(definition)
}

/// `bundledSkills.ts` `getBundledSkills` 的别名。
pub fn get_bundled_skills() -> Vec<CraftCommand> {
    get_bundled_crafts()
}

/// `bundledSkills.ts` `clearBundledSkills` 的别名。
pub fn clear_bundled_skills() {
    clear_bundled_crafts();
}

/// `bundledSkills.ts` `getBundledSkillExtractDir` 的别名。
pub fn get_bundled_skill_extract_dir(skill_name: &str) -> PathBuf {
    bundled_craft_extract_dir(skill_name)
}

/// `bundledSkills.ts` `BundledSkillDefinition` 的别名。
pub type BundledSkillDefinition = BundledCraftDefinition;

// ---------------------------------------------------------------------------
// Bundled craft file extraction helpers
// ---------------------------------------------------------------------------

/// 确定性的捆绑技能文件提取目录。
///
/// 对应 TS `getBundledSkillExtractDir(skillName)`。
pub fn bundled_craft_extract_dir(skill_name: &str) -> PathBuf {
    // 使用临时目录下的 mossen-bundled-skills/<name>
    let base = std::env::temp_dir().join("mossen-bundled-skills");
    base.join(skill_name)
}

/// 提取捆绑技能的引用文件到磁盘。
///
/// 对应 TS `extractBundledSkillFiles()`。
/// 返回写入的目录路径，失败时返回 None（技能继续工作，只是缺少 base-directory 前缀）。
pub async fn extract_bundled_craft_files(
    skill_name: &str,
    files: &HashMap<String, String>,
) -> Option<PathBuf> {
    let dir = bundled_craft_extract_dir(skill_name);
    match write_craft_files(&dir, files).await {
        Ok(()) => Some(dir),
        Err(e) => {
            warn!(
                "Failed to extract bundled craft '{}' to {}: {}",
                skill_name,
                dir.display(),
                e
            );
            None
        }
    }
}

/// 写入技能文件到指定目录。
async fn write_craft_files(dir: &Path, files: &HashMap<String, String>) -> anyhow::Result<()> {
    use std::collections::HashMap as StdMap;

    // 按父目录分组，先创建目录再写入文件
    let mut by_parent: StdMap<PathBuf, Vec<(PathBuf, &str)>> = StdMap::new();
    for (rel_path, content) in files {
        let target = resolve_craft_file_path(dir, rel_path)?;
        let parent = target
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent dir for {}", target.display()))?
            .to_path_buf();
        by_parent
            .entry(parent)
            .or_default()
            .push((target, content.as_str()));
    }

    for (parent, entries) in &by_parent {
        tokio::fs::create_dir_all(parent).await?;
        for (path, content) in entries {
            // 仅在文件不存在时写入（O_EXCL 语义）
            if !path.exists() {
                tokio::fs::write(path, content).await?;
            }
        }
    }

    Ok(())
}

/// 解析并验证技能相对路径，防止路径遍历。
fn resolve_craft_file_path(base_dir: &Path, rel_path: &str) -> anyhow::Result<PathBuf> {
    let normalized = Path::new(rel_path);

    // 拒绝绝对路径
    if normalized.is_absolute() {
        anyhow::bail!("bundled craft file path is absolute: {}", rel_path);
    }

    // 拒绝含 ".." 的路径
    for component in normalized.components() {
        if let std::path::Component::ParentDir = component {
            anyhow::bail!("bundled craft file path escapes skill dir: {}", rel_path);
        }
    }

    Ok(base_dir.join(normalized))
}

/// 在内容块前添加 base directory 前缀。
pub fn prepend_base_dir(blocks: &mut Vec<ContentBlock>, base_dir: &str) {
    let prefix = format!("Base directory for this skill: {}\n\n", base_dir);
    if let Some(ContentBlock::Text { text }) = blocks.first_mut() {
        *text = format!("{}{}", prefix, text);
    } else {
        blocks.insert(0, ContentBlock::Text { text: prefix });
    }
}
