//! # generated — protobuf 生成的事件类型
//!
//! 对应 TypeScript `types/generated/` 目录下 4 个 protobuf 生成文件。
//! 包含 Timestamp、PublicApiAuth、GrowthbookExperimentEvent、
//! MossenCodeInternalEvent 等事件类型，以及对应的 fromJSON/toJSON 逻辑。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// google/protobuf/timestamp.ts → ProtoTimestamp
// =============================================================================

/// Protobuf Timestamp（秒 + 纳秒）。
/// 对应 TS `google/protobuf/timestamp.ts` 中的 `Timestamp` 接口。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ProtoTimestamp {
    /// UTC 秒数（自 1970-01-01 起）。
    #[serde(default)]
    pub seconds: i64,
    /// 纳秒部分（0..999_999_999）。
    #[serde(default)]
    pub nanos: i32,
}

impl ProtoTimestamp {
    /// 从 `ProtoTimestamp` 转换为 `DateTime<Utc>`。
    /// 对应 TS `fromTimestamp()`。
    pub fn to_datetime(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.seconds, self.nanos as u32).unwrap_or_default()
    }

    /// 从 `DateTime<Utc>` 创建 `ProtoTimestamp`。
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        Self {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }
    }

    /// 从 JSON 值解析（兼容 ISO 字符串或 {seconds, nanos} 对象）。
    /// 对应 TS `fromJsonTimestamp()`。
    pub fn from_json_value(value: &serde_json::Value) -> Option<DateTime<Utc>> {
        if let Some(s) = value.as_str() {
            s.parse::<DateTime<Utc>>().ok()
        } else if value.is_object() {
            let ts: ProtoTimestamp = serde_json::from_value(value.clone()).ok()?;
            Some(ts.to_datetime())
        } else {
            None
        }
    }
}

// =============================================================================
// events_mono/common/v1/auth.ts → PublicApiAuth
// =============================================================================

/// 公共 API 认证上下文。
/// 对应 TS `events_mono/common/v1/auth.ts` 中的 `PublicApiAuth` 接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PublicApiAuth {
    /// 账户 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<i64>,
    /// 组织 UUID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_uuid: Option<String>,
    /// 账户 UUID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_uuid: Option<String>,
}

impl PublicApiAuth {
    /// 创建默认值。
    /// 对应 TS `createBasePublicApiAuth()`。
    pub fn create_base() -> Self {
        Self {
            account_id: Some(0),
            organization_uuid: Some(String::new()),
            account_uuid: Some(String::new()),
        }
    }

    /// 从 JSON 对象解析。
    /// 对应 TS `PublicApiAuth.fromJSON()`。
    pub fn from_json(value: &serde_json::Value) -> Self {
        Self {
            account_id: value.get("account_id").and_then(|v| v.as_i64()),
            organization_uuid: value
                .get("organization_uuid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            account_uuid: value
                .get("account_uuid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }

    /// 转换为 JSON 值。
    /// 对应 TS `PublicApiAuth.toJSON()`。
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        if let Some(id) = self.account_id {
            obj.insert("account_id".into(), serde_json::Value::from(id));
        }
        if let Some(ref uuid) = self.organization_uuid {
            obj.insert(
                "organization_uuid".into(),
                serde_json::Value::from(uuid.as_str()),
            );
        }
        if let Some(ref uuid) = self.account_uuid {
            obj.insert(
                "account_uuid".into(),
                serde_json::Value::from(uuid.as_str()),
            );
        }
        serde_json::Value::Object(obj)
    }
}

// =============================================================================
// events_mono/growthbook/v1/growthbook_experiment_event.ts
// =============================================================================

/// GrowthBook 实验分配事件。
/// 对应 TS `GrowthbookExperimentEvent` 接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrowthbookExperimentEvent {
    /// 唯一事件标识（去重用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    /// 用户暴露时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// 实验追踪键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    /// 变体索引：0=对照组，1+=变体。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation_id: Option<i64>,
    /// 分配发生时的环境。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// 分配时的用户属性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_attributes: Option<String>,
    /// 实验元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_metadata: Option<String>,
    /// 设备标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// 认证上下文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PublicApiAuth>,
    /// 会话 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 匿名 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anonymous_id: Option<String>,
    /// 事件元数据变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_metadata_vars: Option<String>,
}

// =============================================================================
// events_mono/mossen_code/v1/mossen_code_internal_event.ts
// =============================================================================

/// GitHub Actions 元数据。
/// 对应 TS `GitHubActionsMetadata` 接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitHubActionsMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_owner_id: Option<String>,
}

/// 环境元数据。
/// 对应 TS `EnvironmentMetadata` 接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_managers: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtimes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_running_with_bun: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_ci: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_mossenbit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_github_action: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_mossen_code_action: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_hosted_auth: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_event_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_actions_runner_environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_actions_runner_os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_action_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wsl_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_actions_metadata: Option<GitHubActionsMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_mossen_code_remote: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_environment_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mossen_code_container_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mossen_code_remote_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_conductor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coworker_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_local_agent_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux_distro_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux_distro_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux_kernel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_raw: Option<String>,
}

/// Slack 上下文。
/// 对应 TS `SlackContext` 接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlackContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack_team_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_enterprise_install: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation_method: Option<String>,
}

/// Mossen Code 内部事件。
/// 对应 TS `MossenCodeInternalEvent` 接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenCodeInternalEvent {
    /// 事件名称（如 "mossen_binary_feedback"、"mossen_api_success"）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    /// 客户端时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub betas: Option<String>,
    /// 环境与运行时信息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvironmentMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_sdk_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_interactive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,
    /// 进程指标（JSON 字符串）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    /// 附加元数据（事件特定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_metadata: Option<String>,
    /// 认证上下文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PublicApiAuth>,
    /// 服务端时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_timestamp: Option<String>,
    /// 事件唯一标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    /// 设备标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// SWE-bench 相关字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swe_bench_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swe_bench_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swe_bench_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Swarm/team agent 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Slack 上下文（仅 cis_* 事件）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marketplace_name: Option<String>,
}
