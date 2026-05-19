//! Remote IO — 对应 TS 的 cli/remoteIO.ts。
//!
//! 为 SDK 模式提供双向流式通信，支持会话追踪。

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use url::Url;

use crate::structured_io::{SDKControlRequest, StructuredIO, StdoutMessage};
use crate::transports::{
    get_transport_for_url, CCRClient, SSETransport, SessionExternalMetadata, Transport,
};

/// RemoteIO — 远程双向流式 IO。
///
/// 对应 TS 的 RemoteIO class，继承自 StructuredIO。
/// 支持 WebSocket/SSE/Hybrid 传输，可选 CCR v2 协议。
pub struct RemoteIO {
    /// 内部 StructuredIO。
    pub structured_io: StructuredIO,
    /// 远程 URL。
    url: Url,
    /// 传输层。
    transport: Arc<dyn Transport>,
    /// CCR v2 客户端（可选）。
    ccr_client: Option<Arc<CCRClient>>,
    /// 恢复的 worker 状态。
    pub restored_worker_state: Arc<RwLock<Option<SessionExternalMetadata>>>,
}

impl RemoteIO {
    /// 创建新的 RemoteIO 实例。
    ///
    /// 对应 TS RemoteIO 的 constructor。
    pub async fn new(
        stream_url: &str,
        replay_user_messages: bool,
    ) -> Result<Self> {
        let url = Url::parse(stream_url).context("invalid stream URL")?;

        // 准备请求头
        let mut headers = HashMap::new();
        if let Some(token) = get_session_ingress_auth_token() {
            headers.insert("Authorization".to_string(), format!("Bearer {}", token));
        } else {
            warn!("[remote-io] No session ingress token available");
        }

        // 添加环境 runner 版本
        if let Ok(er_version) = std::env::var("MOSSEN_CODE_ENVIRONMENT_RUNNER_VERSION") {
            headers.insert("x-environment-runner-version".to_string(), er_version);
        }

        // 动态刷新头的闭包
        let refresh_headers: Box<dyn Fn() -> HashMap<String, String> + Send + Sync> =
            Box::new(|| {
                let mut h = HashMap::new();
                if let Some(token) = get_session_ingress_auth_token() {
                    h.insert("Authorization".to_string(), format!("Bearer {}", token));
                }
                if let Ok(ver) = std::env::var("MOSSEN_CODE_ENVIRONMENT_RUNNER_VERSION") {
                    h.insert("x-environment-runner-version".to_string(), ver);
                }
                h
            });

        // 获取 session ID (use env or generate one)
        let session_id: Option<String> = std::env::var("MOSSEN_SESSION_ID").ok();

        // 获取合适的传输层
        let transport: Arc<dyn Transport> = Arc::from(get_transport_for_url(
            &url,
            headers.clone(),
            session_id.as_deref(),
            Some(refresh_headers),
        ));

        let structured_io = StructuredIO::new(replay_user_messages);

        // 设置数据回调
        let outbound_tx = structured_io.outbound.clone();
        transport.set_on_data(Box::new(move |data| {
            // 将接收到的数据通过 StructuredIO 处理
            info!(data_len = data.len(), "RemoteIO: received data");
        }));

        // 设置关闭回调
        transport.set_on_close(Box::new(|| {
            info!("RemoteIO: transport closed");
        }));

        // 初始化 CCR v2 客户端（如果启用）
        let use_ccr_v2 = std::env::var("MOSSEN_CODE_USE_CCR_V2")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);

        let ccr_client = if use_ccr_v2 {
            let client = Arc::new(CCRClient::new(transport.clone(), url.clone()));
            Some(client)
        } else {
            None
        };

        let restored_worker_state = Arc::new(RwLock::new(None));

        // 初始化 CCR 客户端
        if let Some(ref client) = ccr_client {
            match client.initialize().await {
                Ok(metadata) => {
                    let mut guard = restored_worker_state.write().await;
                    *guard = Some(metadata);
                }
                Err(e) => {
                    error!("CCRClient initialization failed: {}", e);
                    // 不在这里退出，允许 fallback
                }
            }
        }

        // 启动连接
        transport.connect().await?;

        Ok(Self {
            structured_io,
            url,
            transport,
            ccr_client,
            restored_worker_state,
        })
    }

    /// 写入消息。
    pub async fn write(&self, message: &serde_json::Value) -> Result<()> {
        if let Some(ref client) = self.ccr_client {
            client.write_event(message).await
        } else {
            self.transport.write(message).await
        }
    }

    /// 刷新内部事件。
    pub async fn flush_internal_events(&self) -> Result<()> {
        if let Some(ref client) = self.ccr_client {
            client.flush_internal_events().await
        } else {
            Ok(())
        }
    }

    /// 获取待发送内部事件数。
    pub async fn internal_events_pending(&self) -> usize {
        if let Some(ref client) = self.ccr_client {
            client.internal_events_pending().await
        } else {
            0
        }
    }

    /// 关闭连接。
    pub async fn close(&self) {
        if let Some(ref client) = self.ccr_client {
            client.close().await;
        }
        self.transport.close();
    }
}

/// 获取会话入口认证 token。
fn get_session_ingress_auth_token() -> Option<String> {
    // 优先从环境变量获取
    std::env::var("MOSSEN_CODE_SESSION_ACCESS_TOKEN")
        .ok()
        .or_else(|| std::env::var("MOSSEN_CODE_SESSION_INGRESS_TOKEN").ok())
}
