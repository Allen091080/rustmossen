//! Builtin MCP server templates.
//!
//! Translates `services/mcp/builtinTemplates.ts`.

use serde::{Deserialize, Serialize};

use crate::mcp::types::McpServerConfig;

/// Risk level for a builtin template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltinMcpTemplateRisk {
    Low,
    Medium,
}

/// Parameter type for builtin templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltinMcpTemplateParameter {
    Root,
    Db,
}

/// A builtin MCP server template.
#[derive(Debug, Clone)]
pub struct BuiltinMcpTemplate {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub config: McpServerConfig,
    pub parameters: &'static [BuiltinMcpTemplateParameter],
    pub default_enabled: bool,
    pub read_only: bool,
    pub requires_credentials: bool,
    pub requires_network: bool,
    pub risk: BuiltinMcpTemplateRisk,
    pub notes: &'static [&'static str],
}

fn builtin_mcp_templates() -> Vec<BuiltinMcpTemplate> {
    vec![
        BuiltinMcpTemplate {
            name: "filesystem-readonly",
            title: "Filesystem readonly",
            description: "Template for a local filesystem MCP server scoped to explicit read-only roots.",
            config: McpServerConfig::Stdio {
                command: "mcp-server-filesystem".into(),
                args: vec!["--readonly".into(), "<absolute-project-root>".into()],
                env: None,
                cwd: None,
            },
            parameters: &[BuiltinMcpTemplateParameter::Root],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: &[
                "User must replace <absolute-project-root> before enabling.",
                "Keep writable filesystem tools in a separate explicit server.",
            ],
        },
        BuiltinMcpTemplate {
            name: "git-readonly",
            title: "Git readonly",
            description: "Template for read-only repository inspection: status, branches, history, and metadata.",
            config: McpServerConfig::Stdio {
                command: "mcp-server-git".into(),
                args: vec!["--readonly".into(), "<absolute-repo-root>".into()],
                env: None,
                cwd: None,
            },
            parameters: &[BuiltinMcpTemplateParameter::Root],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: &[
                "Do not expose commit, push, merge, or reset tools in this template.",
                "Use Mossen permission gates for any future mutation-capable git server.",
            ],
        },
        BuiltinMcpTemplate {
            name: "local-docs",
            title: "Local docs",
            description: "Template for searching local documentation folders without network or credential access.",
            config: McpServerConfig::Stdio {
                command: "mcp-server-local-docs".into(),
                args: vec!["--root".into(), "<absolute-docs-root>".into()],
                env: None,
                cwd: None,
            },
            parameters: &[BuiltinMcpTemplateParameter::Root],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: &[
                "Good fit for project docs, API references, and internal runbooks.",
                "Do not point this at secret directories.",
            ],
        },
        BuiltinMcpTemplate {
            name: "playwright-local",
            title: "Playwright local browser",
            description: "Template for local browser automation against localhost or explicit test targets.",
            config: McpServerConfig::Stdio {
                command: "mcp-server-playwright".into(),
                args: vec!["--allow-localhost-only".into()],
                env: None,
                cwd: None,
            },
            parameters: &[],
            default_enabled: false,
            read_only: false,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Medium,
            notes: &[
                "Not read-only: browser actions can click, type, and mutate local apps.",
                "Keep remote browsing and authenticated sites out of the default template.",
            ],
        },
        BuiltinMcpTemplate {
            name: "sqlite-readonly",
            title: "SQLite readonly",
            description: "Template for inspecting a local SQLite database in read-only mode.",
            config: McpServerConfig::Stdio {
                command: "mcp-server-sqlite".into(),
                args: vec!["--readonly".into(), "<absolute-db-path>".into()],
                env: None,
                cwd: None,
            },
            parameters: &[BuiltinMcpTemplateParameter::Db],
            default_enabled: false,
            read_only: true,
            requires_credentials: false,
            requires_network: false,
            risk: BuiltinMcpTemplateRisk::Low,
            notes: &[
                "Use read-only database flags at both MCP server and SQLite connection level.",
                "Do not include production credential paths in templates.",
            ],
        },
    ]
}

/// Get all builtin MCP templates.
pub fn get_builtin_mcp_templates() -> Vec<BuiltinMcpTemplate> {
    builtin_mcp_templates()
}

/// Get a builtin MCP template by name.
pub fn get_builtin_mcp_template(name: &str) -> Option<BuiltinMcpTemplate> {
    builtin_mcp_templates().into_iter().find(|t| t.name == name)
}

/// Localized text for builtin MCP templates.
pub struct LocalizedTemplateText {
    pub title: Option<&'static str>,
    pub description: Option<&'static str>,
    pub notes: Option<&'static [&'static str]>,
}

/// Get localized text for a builtin template.
pub fn get_localized_builtin_mcp_template_text(name: &str) -> LocalizedTemplateText {
    match name {
        "filesystem-readonly" => LocalizedTemplateText {
            title: Some("文件系统只读"),
            description: Some(
                "用于本地 filesystem MCP server 的模板，仅暴露明确指定的只读根目录。",
            ),
            notes: Some(&[
                "启用前必须把 <absolute-project-root> 替换成真实绝对路径。",
                "可写文件系统工具应放在另一个明确声明的 server 中。",
            ]),
        },
        "git-readonly" => LocalizedTemplateText {
            title: Some("Git 只读"),
            description: Some("用于只读仓库检查：状态、分支、历史和元数据。"),
            notes: Some(&[
                "该模板不暴露 commit、push、merge 或 reset 工具。",
                "未来如需可变更的 git server，必须走 Mossen 权限闸。",
            ]),
        },
        "local-docs" => LocalizedTemplateText {
            title: Some("本地文档"),
            description: Some("用于搜索本地文档目录，不需要网络或凭据访问。"),
            notes: Some(&[
                "适合项目文档、API reference 和内部 runbook。",
                "不要把它指向 secret 目录。",
            ]),
        },
        "playwright-local" => LocalizedTemplateText {
            title: Some("本地 Playwright 浏览器"),
            description: Some("用于针对 localhost 或明确测试目标的本地浏览器自动化。"),
            notes: Some(&[
                "这不是只读能力：浏览器动作可以点击、输入并改变本地应用。",
                "默认模板不应包含远程浏览或已登录站点。",
            ]),
        },
        "sqlite-readonly" => LocalizedTemplateText {
            title: Some("SQLite 只读"),
            description: Some("用于以只读模式检查本地 SQLite 数据库。"),
            notes: Some(&[
                "MCP server 与 SQLite connection 两层都应使用只读参数。",
                "不要在模板中包含生产凭据路径。",
            ]),
        },
        _ => LocalizedTemplateText {
            title: None,
            description: None,
            notes: None,
        },
    }
}

/// Parameters for template instantiation.
pub struct TemplateParams {
    pub root: Option<String>,
    pub db: Option<String>,
}

/// Result of instantiating a template.
pub struct InstantiateResult {
    pub config: Option<McpServerConfig>,
    pub missing: Vec<BuiltinMcpTemplateParameter>,
}

/// Instantiate a builtin MCP template with parameters.
pub fn instantiate_builtin_mcp_template(
    template: &BuiltinMcpTemplate,
    params: &TemplateParams,
) -> InstantiateResult {
    let missing: Vec<BuiltinMcpTemplateParameter> = template
        .parameters
        .iter()
        .filter(|p| match p {
            BuiltinMcpTemplateParameter::Root => params.root.is_none(),
            BuiltinMcpTemplateParameter::Db => params.db.is_none(),
        })
        .copied()
        .collect();

    if !missing.is_empty() {
        return InstantiateResult {
            config: None,
            missing,
        };
    }

    let config = match template.name {
        "filesystem-readonly" => McpServerConfig::Stdio {
            command: "mcp-server-filesystem".into(),
            args: vec!["--readonly".into(), params.root.clone().unwrap()],
            env: None,
            cwd: None,
        },
        "git-readonly" => McpServerConfig::Stdio {
            command: "mcp-server-git".into(),
            args: vec!["--readonly".into(), params.root.clone().unwrap()],
            env: None,
            cwd: None,
        },
        "local-docs" => McpServerConfig::Stdio {
            command: "mcp-server-local-docs".into(),
            args: vec!["--root".into(), params.root.clone().unwrap()],
            env: None,
            cwd: None,
        },
        "playwright-local" => McpServerConfig::Stdio {
            command: "mcp-server-playwright".into(),
            args: vec!["--allow-localhost-only".into()],
            env: None,
            cwd: None,
        },
        "sqlite-readonly" => McpServerConfig::Stdio {
            command: "mcp-server-sqlite".into(),
            args: vec!["--readonly".into(), params.db.clone().unwrap()],
            env: None,
            cwd: None,
        },
        _ => {
            return InstantiateResult {
                config: None,
                missing: vec![],
            };
        }
    };

    InstantiateResult {
        config: Some(config),
        missing: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_template_inventory_contains_current_rendered_set() {
        let templates = get_builtin_mcp_templates();
        let names = templates
            .iter()
            .map(|template| template.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "filesystem-readonly",
                "git-readonly",
                "local-docs",
                "playwright-local",
                "sqlite-readonly",
            ]
        );
        assert!(templates.iter().any(|template| !template.read_only));
    }

    #[test]
    fn builtin_template_localization_is_render_time_overlay() {
        let template = get_builtin_mcp_template("filesystem-readonly").expect("template");
        assert_eq!(template.title, "Filesystem readonly");
        let localized = get_localized_builtin_mcp_template_text("filesystem-readonly");
        assert_eq!(localized.title, Some("文件系统只读"));
        assert!(localized
            .description
            .expect("description")
            .contains("只读根目录"));
    }

    #[test]
    fn builtin_template_instantiation_replaces_absolute_parameters() {
        let template = get_builtin_mcp_template("sqlite-readonly").expect("template");
        let result = instantiate_builtin_mcp_template(
            &template,
            &TemplateParams {
                root: None,
                db: Some("/tmp/mossen-test.sqlite".to_string()),
            },
        );
        assert!(result.missing.is_empty());
        match result.config.expect("config") {
            McpServerConfig::Stdio { command, args, .. } => {
                assert_eq!(command, "mcp-server-sqlite");
                assert_eq!(args, vec!["--readonly", "/tmp/mossen-test.sqlite"]);
            }
            other => panic!("expected stdio template, got {other:?}"),
        }
    }
}
