//! Builtin template install plan — staged install with confirmation token.
//!
//! Translates `services/mcp/builtinTemplatePlan.ts`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::Rng;

use crate::mcp::builtin_templates::{
    get_builtin_mcp_template, get_builtin_mcp_templates, instantiate_builtin_mcp_template,
    BuiltinMcpTemplateParameter, TemplateParams,
};
use crate::mcp::types::{ConfigScope, McpServerConfig};

pub const MCP_TEMPLATE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

pub type McpTemplateWritableScope = ConfigScope; // local | user | project

#[derive(Debug, Clone)]
pub enum McpTemplatePlanError {
    UnknownTemplate {
        template_name: Option<String>,
        available_templates: Vec<String>,
    },
    MissingParameter {
        template_name: String,
        missing: Vec<BuiltinMcpTemplateParameter>,
    },
    PathNotAbsolute {
        parameter: BuiltinMcpTemplateParameter,
        value: String,
    },
    InvalidScope {
        scope: Option<String>,
    },
    UnknownToken {
        token: String,
    },
    ExpiredToken {
        token: String,
    },
    InstallFailed {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct McpTemplateInstallPlan {
    pub token: String,
    pub created_at: u64,
    pub template_name: String,
    pub title: String,
    pub server_name: String,
    pub scope: ConfigScope,
    pub config: McpServerConfig,
    pub read_only: bool,
    pub risk: String,
    pub notes: Vec<String>,
}

pub enum McpTemplateInstallResult {
    Ok { plan: McpTemplateInstallPlan },
    Err { error: McpTemplatePlanError },
}

lazy_static::lazy_static! {
    static ref PLAN_STORE: Mutex<HashMap<String, McpTemplateInstallPlan>> = Mutex::new(HashMap::new());
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn prune_expired_plans(store: &mut HashMap<String, McpTemplateInstallPlan>) {
    let now = now_ms();
    store.retain(|_, plan| now - plan.created_at <= MCP_TEMPLATE_PLAN_TOKEN_TTL_MS);
}

fn create_token(store: &HashMap<String, McpTemplateInstallPlan>) -> String {
    let mut rng = rand::thread_rng();
    loop {
        let token = format!("{:08x}", rng.gen::<u32>());
        if !store.contains_key(&token) {
            return token;
        }
    }
}

fn normalize_scope(scope: Option<&str>) -> Option<ConfigScope> {
    match scope {
        None | Some("local") => Some(ConfigScope::Local),
        Some("user") => Some(ConfigScope::User),
        Some("project") => Some(ConfigScope::Project),
        _ => None,
    }
}

fn assert_absolute(
    parameter: BuiltinMcpTemplateParameter,
    value: Option<&str>,
) -> Option<McpTemplatePlanError> {
    match value {
        None => None,
        Some(v) => {
            if Path::new(v).is_absolute() {
                None
            } else {
                Some(McpTemplatePlanError::PathNotAbsolute {
                    parameter,
                    value: v.to_string(),
                })
            }
        }
    }
}

/// Create a template install plan.
pub fn get_mcp_template_install_plan(
    template_name: Option<&str>,
    server_name: Option<&str>,
    scope: Option<&str>,
    root: Option<&str>,
    db: Option<&str>,
) -> McpTemplateInstallResult {
    let mut store = PLAN_STORE.lock().unwrap();
    prune_expired_plans(&mut store);

    let template = template_name.and_then(get_builtin_mcp_template);
    if template.is_none() {
        return McpTemplateInstallResult::Err {
            error: McpTemplatePlanError::UnknownTemplate {
                template_name: template_name.map(|s| s.to_string()),
                available_templates: get_builtin_mcp_templates()
                    .iter()
                    .map(|t| t.name.to_string())
                    .collect(),
            },
        };
    }
    let template = template.unwrap();

    let resolved_scope = match normalize_scope(scope) {
        Some(s) => s,
        None => {
            return McpTemplateInstallResult::Err {
                error: McpTemplatePlanError::InvalidScope {
                    scope: scope.map(|s| s.to_string()),
                },
            };
        }
    };

    if let Some(err) = assert_absolute(BuiltinMcpTemplateParameter::Root, root) {
        return McpTemplateInstallResult::Err { error: err };
    }
    if let Some(err) = assert_absolute(BuiltinMcpTemplateParameter::Db, db) {
        return McpTemplateInstallResult::Err { error: err };
    }

    let params = TemplateParams {
        root: root.map(|s| s.to_string()),
        db: db.map(|s| s.to_string()),
    };
    let instantiated = instantiate_builtin_mcp_template(&template, &params);

    if instantiated.config.is_none() {
        return McpTemplateInstallResult::Err {
            error: McpTemplatePlanError::MissingParameter {
                template_name: template.name.to_string(),
                missing: instantiated.missing,
            },
        };
    }

    let token = create_token(&store);
    let plan = McpTemplateInstallPlan {
        token: token.clone(),
        created_at: now_ms(),
        template_name: template.name.to_string(),
        title: template.title.to_string(),
        server_name: server_name.unwrap_or(template.name).to_string(),
        scope: resolved_scope,
        config: instantiated.config.unwrap(),
        read_only: template.read_only,
        risk: format!("{:?}", template.risk).to_lowercase(),
        notes: template.notes.iter().map(|s| s.to_string()).collect(),
    };
    store.insert(token, plan.clone());
    McpTemplateInstallResult::Ok { plan }
}

/// Execute a previously-created template install plan.
pub async fn execute_mcp_template_install_plan(
    token: &str,
    add_mcp_config: impl AsyncAddMcpConfig,
) -> McpTemplateInstallResult {
    let plan = {
        let mut store = PLAN_STORE.lock().unwrap();
        prune_expired_plans(&mut store);
        store.remove(token)
    };

    let plan = match plan {
        Some(p) => p,
        None => {
            return McpTemplateInstallResult::Err {
                error: McpTemplatePlanError::UnknownToken {
                    token: token.to_string(),
                },
            };
        }
    };

    if now_ms() - plan.created_at > MCP_TEMPLATE_PLAN_TOKEN_TTL_MS {
        return McpTemplateInstallResult::Err {
            error: McpTemplatePlanError::ExpiredToken {
                token: token.to_string(),
            },
        };
    }

    match add_mcp_config
        .add_config(&plan.server_name, &plan.config, plan.scope)
        .await
    {
        Ok(()) => McpTemplateInstallResult::Ok { plan },
        Err(e) => McpTemplateInstallResult::Err {
            error: McpTemplatePlanError::InstallFailed {
                message: e.to_string(),
            },
        },
    }
}

/// Trait for async MCP config addition.
#[async_trait::async_trait]
pub trait AsyncAddMcpConfig {
    async fn add_config(
        &self,
        name: &str,
        config: &McpServerConfig,
        scope: ConfigScope,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Reset plan store for testing.
pub fn reset_mcp_template_plan_store_for_testing() {
    PLAN_STORE.lock().unwrap().clear();
}
