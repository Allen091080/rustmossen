//! Internal logging — Kubernetes namespace and container detection for internal users

use once_cell::sync::Lazy;
use tokio::sync::OnceCell;
use tracing::debug;

static K8S_NAMESPACE: OnceCell<Option<String>> = OnceCell::const_new();
static CONTAINER_ID: OnceCell<Option<String>> = OnceCell::const_new();

/// Get the current Kubernetes namespace (internal users only)
pub async fn get_kubernetes_namespace() -> Option<String> {
    if std::env::var("USER_TYPE").as_deref() != Ok("internal") {
        return None;
    }

    K8S_NAMESPACE
        .get_or_init(|| async {
            match tokio::fs::read_to_string(
                "/var/run/secrets/kubernetes.io/serviceaccount/namespace",
            )
            .await
            {
                Ok(content) => Some(content.trim().to_string()),
                Err(_) => Some("namespace not found".to_string()),
            }
        })
        .await
        .clone()
}

/// Get the OCI container ID from within a running container
pub async fn get_container_id() -> Option<String> {
    if std::env::var("USER_TYPE").as_deref() != Ok("internal") {
        return None;
    }

    CONTAINER_ID
        .get_or_init(|| async {
            match tokio::fs::read_to_string("/proc/self/mountinfo").await {
                Ok(content) => {
                    // Match Docker or containerd container IDs
                    let pattern =
                        regex::Regex::new(r"(?:/docker/containers/|/sandboxes/)([0-9a-f]{64})")
                            .ok()?;

                    for line in content.lines() {
                        if let Some(captures) = pattern.captures(line) {
                            if let Some(id) = captures.get(1) {
                                return Some(id.as_str().to_string());
                            }
                        }
                    }
                    Some("container ID not found in mountinfo".to_string())
                }
                Err(_) => Some("container ID not found".to_string()),
            }
        })
        .await
        .clone()
}

/// Log permission context for internal users
pub async fn log_permission_context_for_ants(tool_permission_context: Option<&str>, moment: &str) {
    if std::env::var("USER_TYPE").as_deref() != Ok("internal") {
        return;
    }

    let namespace = get_kubernetes_namespace().await;
    let container_id = get_container_id().await;

    debug!(
        moment = moment,
        namespace = namespace.as_deref().unwrap_or("unknown"),
        container_id = container_id.as_deref().unwrap_or("unknown"),
        permission_context = tool_permission_context.unwrap_or("null"),
        "Internal permission context logged"
    );
}
