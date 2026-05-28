//! # internal_logging — 内部日志与环境诊断
//!
//! 对应 TS `services/internalLogging.ts`。提供 Kubernetes 命名空间
//! 检测、容器 ID 获取等内部环境诊断功能。

use std::sync::OnceLock;

use tokio::fs;
use tracing::debug;

// ---------------------------------------------------------------------------
// Kubernetes 命名空间检测
// ---------------------------------------------------------------------------

/// 缓存的 Kubernetes 命名空间。
static K8S_NAMESPACE: OnceLock<Option<String>> = OnceLock::new();

/// 获取 Kubernetes 命名空间。
///
/// 对应 TS `getKubernetesNamespace()`。
/// 在非内部环境返回 `None`，devbox 返回命名空间名称。
pub async fn get_kubernetes_namespace() -> Option<String> {
    if let Some(cached) = K8S_NAMESPACE.get() {
        return cached.clone();
    }

    let result = detect_k8s_namespace().await;
    // OnceLock::get_or_init 不支持 async，手动处理
    let _ = K8S_NAMESPACE.set(result.clone());
    result
}

async fn detect_k8s_namespace() -> Option<String> {
    // 仅在内部环境检测
    if std::env::var("USER_TYPE").ok().as_deref() != Some("internal") {
        return None;
    }

    let path = "/var/run/secrets/kubernetes.io/serviceaccount/namespace";
    match fs::read_to_string(path).await {
        Ok(content) => Some(content.trim().to_string()),
        Err(_) => Some("namespace not found".to_string()),
    }
}

// ---------------------------------------------------------------------------
// 容器 ID 检测
// ---------------------------------------------------------------------------

/// 缓存的容器 ID。
static CONTAINER_ID: OnceLock<Option<String>> = OnceLock::new();

/// 获取 OCI 容器 ID。
///
/// 对应 TS `getContainerId()`。
pub async fn get_container_id() -> Option<String> {
    if let Some(cached) = CONTAINER_ID.get() {
        return cached.clone();
    }

    let result = detect_container_id().await;
    let _ = CONTAINER_ID.set(result.clone());
    result
}

async fn detect_container_id() -> Option<String> {
    if std::env::var("USER_TYPE").ok().as_deref() != Some("internal") {
        return None;
    }

    let path = "/proc/self/mountinfo";
    match fs::read_to_string(path).await {
        Ok(content) => {
            let pattern =
                regex::Regex::new(r"(?:/docker/containers/|/sandboxes/)([0-9a-f]{64})").ok()?;

            for line in content.lines() {
                if let Some(caps) = pattern.captures(line) {
                    if let Some(id) = caps.get(1) {
                        return Some(id.as_str().to_string());
                    }
                }
            }
            Some("container ID not found in mountinfo".to_string())
        }
        Err(_) => Some("container ID not found".to_string()),
    }
}

// ---------------------------------------------------------------------------
// 权限上下文日志
// ---------------------------------------------------------------------------

/// 记录权限上下文（仅内部环境）。
///
/// 对应 TS `logPermissionContextForAnts()`。
pub async fn log_permission_context(
    _tool_permission_context: Option<&serde_json::Value>,
    moment: &str,
) {
    if std::env::var("USER_TYPE").ok().as_deref() != Some("internal") {
        return;
    }

    let namespace = get_kubernetes_namespace().await;
    let container_id = get_container_id().await;

    debug!(
        moment = moment,
        namespace = namespace.as_deref().unwrap_or("unknown"),
        container_id = container_id.as_deref().unwrap_or("unknown"),
        "internal permission context log"
    );
}
