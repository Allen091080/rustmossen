//! # config — 技能与插件配置
//!
//! 对应 TypeScript `skills/loadSkillsDir.ts` 中的 frontmatter 解析部分。
//! 提供技能前言（frontmatter）解析、路径模式解析、配置合并逻辑。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use mossen_types::command::{EffortValue, ExecutionContext};

// ---------------------------------------------------------------------------
// Frontmatter 数据
// ---------------------------------------------------------------------------

/// 从 SKILL.md 中解析出的 frontmatter 数据。
///
/// 对应 TS `FrontmatterData`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrontmatterData {
    /// 技能显示名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
    /// 使用场景。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    /// 模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 允许的工具。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<serde_json::Value>,
    /// 参数提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    /// 参数名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// 版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 是否可由用户调用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_invocable: Option<serde_json::Value>,
    /// 是否禁用模型调用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_model_invocation: Option<serde_json::Value>,
    /// Hooks 配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
    /// 执行上下文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Agent 名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Effort 级别。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<serde_json::Value>,
    /// 路径模式（条件技能）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<serde_json::Value>,
    /// Shell 配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<serde_json::Value>,
    /// 额外字段。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Shell 配置
// ---------------------------------------------------------------------------

/// Shell 前言配置 — 对应 TS `FrontmatterShell`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmatterShell {
    /// Shell 命令。
    pub command: String,
    /// 工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

// ---------------------------------------------------------------------------
// 已解析的技能前言字段
// ---------------------------------------------------------------------------

/// 从 frontmatter 解析出的所有技能字段。
///
/// 对应 TS `parseSkillFrontmatterFields()` 的返回值。
#[derive(Debug, Clone)]
pub struct ParsedSkillFields {
    /// 显示名称。
    pub display_name: Option<String>,
    /// 描述。
    pub description: String,
    /// 是否为用户指定的描述。
    pub has_user_specified_description: bool,
    /// 允许的工具列表。
    pub allowed_tools: Vec<String>,
    /// 参数提示。
    pub argument_hint: Option<String>,
    /// 参数名称列表。
    pub argument_names: Vec<String>,
    /// 使用场景。
    pub when_to_use: Option<String>,
    /// 版本。
    pub version: Option<String>,
    /// 指定模型。
    pub model: Option<String>,
    /// 是否禁用模型调用。
    pub disable_model_invocation: bool,
    /// 用户是否可调用。
    pub user_invocable: bool,
    /// Hooks 配置。
    pub hooks: Option<serde_json::Value>,
    /// 执行上下文。
    pub execution_context: Option<ExecutionContext>,
    /// 关联的 agent。
    pub agent: Option<String>,
    /// Effort 级别。
    pub effort: Option<EffortValue>,
    /// Shell 配置。
    pub shell: Option<FrontmatterShell>,
}

// ---------------------------------------------------------------------------
// 解析函数
// ---------------------------------------------------------------------------

/// 从 frontmatter 解析技能字段。
///
/// 对应 TS `parseSkillFrontmatterFields()`。
pub fn parse_skill_frontmatter_fields(
    frontmatter: &FrontmatterData,
    markdown_content: &str,
    resolved_name: &str,
) -> ParsedSkillFields {
    let description = coerce_description_to_string(frontmatter.description.as_ref(), resolved_name)
        .unwrap_or_else(|| extract_description_from_markdown(markdown_content, "Skill"));

    let has_user_specified_description = frontmatter
        .description
        .as_ref()
        .and_then(|v| v.as_str())
        .is_some();

    let user_invocable = frontmatter
        .user_invocable
        .as_ref()
        .map(parse_boolean_frontmatter)
        .unwrap_or(true);

    let model = match frontmatter.model.as_deref() {
        Some("inherit") | None => None,
        Some(m) => Some(m.to_string()),
    };

    let disable_model_invocation = frontmatter
        .disable_model_invocation
        .as_ref()
        .map(parse_boolean_frontmatter)
        .unwrap_or(false);

    let execution_context = match frontmatter.context.as_deref() {
        Some("fork") => Some(ExecutionContext::Fork),
        _ => None,
    };

    let effort = frontmatter.effort.as_ref().and_then(parse_effort_value);

    let allowed_tools = parse_allowed_tools(frontmatter.allowed_tools.as_ref());

    let argument_names = parse_argument_names(frontmatter.arguments.as_ref());

    let shell = parse_shell_frontmatter(frontmatter.shell.as_ref());

    ParsedSkillFields {
        display_name: frontmatter.name.clone(),
        description,
        has_user_specified_description,
        allowed_tools,
        argument_hint: frontmatter.argument_hint.clone(),
        argument_names,
        when_to_use: frontmatter.when_to_use.clone(),
        version: frontmatter.version.clone(),
        model,
        disable_model_invocation,
        user_invocable,
        hooks: frontmatter.hooks.clone(),
        execution_context,
        agent: frontmatter.agent.clone(),
        effort,
        shell,
    }
}

/// 解析路径模式。
///
/// 对应 TS `parseSkillPaths()`。
pub fn parse_skill_paths(frontmatter: &FrontmatterData) -> Option<Vec<String>> {
    let paths_val = frontmatter.paths.as_ref()?;

    let patterns: Vec<String> = match paths_val {
        serde_json::Value::String(s) => s
            .split('\n')
            .flat_map(|line| line.split(','))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => return None,
    };

    // 移除 /** 后缀
    let patterns: Vec<String> = patterns
        .into_iter()
        .map(|p| {
            if p.ends_with("/**") {
                p[..p.len() - 3].to_string()
            } else {
                p
            }
        })
        .filter(|p| !p.is_empty())
        .collect();

    // 全是 ** 则视为无路径
    if patterns.is_empty() || patterns.iter().all(|p| p == "**") {
        return None;
    }

    Some(patterns)
}

/// 估算技能前言的 token 数。
///
/// 对应 TS `estimateSkillFrontmatterTokens()`。
pub fn estimate_skill_frontmatter_tokens(
    name: &str,
    description: &str,
    when_to_use: Option<&str>,
) -> usize {
    let text = [Some(name), Some(description), when_to_use]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    // 粗略估算：每 4 个字符约 1 个 token
    text.len() / 4
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 将描述值强制转换为字符串。
fn coerce_description_to_string(
    value: Option<&serde_json::Value>,
    _skill_name: &str,
) -> Option<String> {
    match value? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// 从 Markdown 内容提取描述（第一段非空行）。
fn extract_description_from_markdown(content: &str, fallback_label: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return trimmed.to_string();
    }
    format!("{} command", fallback_label)
}

/// 解析布尔型 frontmatter 值。
fn parse_boolean_frontmatter(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => matches!(s.to_lowercase().as_str(), "true" | "yes" | "1"),
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/// 解析 effort 值。
fn parse_effort_value(value: &serde_json::Value) -> Option<EffortValue> {
    match value {
        serde_json::Value::String(s) => match s.to_lowercase().as_str() {
            "low" => Some(EffortValue::Low),
            "medium" | "med" => Some(EffortValue::Medium),
            "high" => Some(EffortValue::High),
            "max" => Some(EffortValue::Max),
            _ => None,
        },
        serde_json::Value::Number(n) => {
            let v = n.as_i64()?;
            match v {
                0..=25 => Some(EffortValue::Low),
                26..=50 => Some(EffortValue::Medium),
                51..=75 => Some(EffortValue::High),
                _ => Some(EffortValue::Max),
            }
        }
        _ => None,
    }
}

/// 解析允许的工具列表。
fn parse_allowed_tools(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    }
}

/// 解析参数名称列表。
fn parse_argument_names(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}

/// 解析 shell 前言配置。
fn parse_shell_frontmatter(value: Option<&serde_json::Value>) -> Option<FrontmatterShell> {
    let val = value?;
    match val {
        serde_json::Value::String(s) => Some(FrontmatterShell {
            command: s.clone(),
            cwd: None,
        }),
        serde_json::Value::Object(obj) => {
            let command = obj.get("command")?.as_str()?.to_string();
            let cwd = obj
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(FrontmatterShell { command, cwd })
        }
        _ => None,
    }
}

/// 解析 frontmatter（YAML 头部）与 markdown 正文。
///
/// 对应 TS `parseFrontmatter(content, filePath)`。
pub fn parse_frontmatter(content: &str) -> (FrontmatterData, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (FrontmatterData::default(), content.to_string());
    }

    // 查找结束的 ---
    if let Some(end_idx) = trimmed[3..].find("\n---") {
        let yaml_section = &trimmed[3..3 + end_idx];
        let markdown = &trimmed[3 + end_idx + 4..]; // skip \n---

        if let Ok(fm) = serde_json::from_value(serde_yaml_to_json(yaml_section)) {
            return (fm, markdown.trim_start_matches('\n').to_string());
        }
    }

    (FrontmatterData::default(), content.to_string())
}

/// YAML → JSON conversion via `serde_yaml`. Handles nested mappings,
/// sequences, multi-line scalars, anchors, and explicit-typed scalars
/// (everything the previous line-by-line parser silently dropped). Falls
/// back to an empty object if the input isn't valid YAML — same shape as
/// the prior implementation's defensive default so callers don't need to
/// change.
fn serde_yaml_to_json(yaml: &str) -> serde_json::Value {
    serde_yaml::from_str::<serde_json::Value>(yaml)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()))
}
