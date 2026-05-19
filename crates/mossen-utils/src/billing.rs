//! # billing — 计费访问控制
//!
//! 对应 TypeScript `utils/billing.ts`。

use std::env;
use parking_lot::Mutex;

/// 模拟计费访问覆盖（用于 /mock-limits 测试）
static MOCK_BILLING_ACCESS_OVERRIDE: Mutex<Option<bool>> = Mutex::new(None);

/// 认证 token 源
pub struct AuthTokenSource {
    pub has_token: bool,
}

/// 订阅类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingSubscriptionType {
    Max,
    Pro,
    Enterprise,
    Team,
    Free,
    Other(String),
}

/// 账户角色信息
pub struct OAuthAccount {
    pub organization_role: Option<String>,
    pub workspace_role: Option<String>,
}

/// 计费配置源 trait
pub trait BillingConfigSource: Send + Sync {
    fn is_custom_backend_enabled(&self) -> bool;
    fn is_hosted_subscriber(&self) -> bool;
    fn get_subscription_type(&self) -> BillingSubscriptionType;
    fn get_auth_token_source(&self) -> AuthTokenSource;
    fn get_mossen_api_key(&self) -> Option<String>;
    fn get_oauth_account(&self) -> Option<OAuthAccount>;
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}

/// 检查是否有控制台计费访问权限
pub fn has_console_billing_access(config: &dyn BillingConfigSource) -> bool {
    if config.is_custom_backend_enabled() {
        return false;
    }

    // 检查是否通过环境变量禁用了成本报告
    if let Ok(val) = env::var("DISABLE_COST_WARNINGS") {
        if is_env_truthy(&val) {
            return false;
        }
    }

    let is_subscriber = config.is_hosted_subscriber();
    if is_subscriber {
        return false;
    }

    // 检查用户是否有任何形式的认证
    let auth_source = config.get_auth_token_source();
    let has_api_key = config.get_mossen_api_key().is_some();

    // 如果用户没有任何认证（已登出），不显示成本
    if !auth_source.has_token && !has_api_key {
        return false;
    }

    let account = config.get_oauth_account();
    let (org_role, workspace_role) = match account {
        Some(acc) => (acc.organization_role, acc.workspace_role),
        None => (None, None),
    };

    let org_role = match org_role {
        Some(r) => r,
        None => return false, // 为旧用户隐藏成本（自添加角色后未重新认证）
    };
    let workspace_role = match workspace_role {
        Some(r) => r,
        None => return false,
    };

    // 用户在组织或工作区级别具有 admin 或 billing 角色时有计费访问
    ["admin", "billing"].contains(&org_role.as_str())
        || ["workspace_admin", "workspace_billing"].contains(&workspace_role.as_str())
}

/// 设置模拟计费访问覆盖
pub fn set_mock_billing_access_override(value: Option<bool>) {
    *MOCK_BILLING_ACCESS_OVERRIDE.lock() = value;
}

/// 检查是否有托管计费访问权限
pub fn has_hosted_billing_access(config: &dyn BillingConfigSource) -> bool {
    if config.is_custom_backend_enabled() {
        return false;
    }

    // 首先检查模拟计费访问（用于 /mock-limits 测试）
    if let Some(override_val) = *MOCK_BILLING_ACCESS_OVERRIDE.lock() {
        return override_val;
    }

    if !config.is_hosted_subscriber() {
        return false;
    }

    let subscription_type = config.get_subscription_type();

    // 消费者计划 (Max/Pro) - 个人用户始终有计费访问
    if subscription_type == BillingSubscriptionType::Max
        || subscription_type == BillingSubscriptionType::Pro
    {
        return true;
    }

    // 团队/企业 - 检查 admin 或 billing 角色
    let account = config.get_oauth_account();
    let org_role = match account.and_then(|a| a.organization_role) {
        Some(r) => r,
        None => return false,
    };

    ["admin", "billing", "owner", "primary_owner"].contains(&org_role.as_str())
}
