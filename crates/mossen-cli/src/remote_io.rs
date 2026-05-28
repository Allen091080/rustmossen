//! Remote IO — 对应 TS 的 cli/remoteIO.ts。
//!
//! 为 SDK 模式提供双向流式通信，支持会话追踪。

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use url::Url;

use crate::structured_io::StructuredIO;
use crate::transports::{get_transport_for_url, CCRClient, SessionExternalMetadata, Transport};

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
    pub async fn new(stream_url: &str, replay_user_messages: bool) -> Result<Self> {
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

        // 设置数据回调：transport receives NDJSON/JSON chunks, StructuredIO
        // owns protocol parsing and side effects such as control responses.
        let (incoming_tx, mut incoming_rx) = mpsc::channel::<String>(256);
        let incoming_structured_io = structured_io.clone();
        tokio::spawn(async move {
            while let Some(data) = incoming_rx.recv().await {
                process_remote_transport_data(incoming_structured_io.clone(), data).await;
            }
        });

        transport.set_on_data(Box::new(move |data| {
            info!(data_len = data.len(), "RemoteIO: received data");
            if let Err(err) = incoming_tx.try_send(data) {
                warn!(error = %err, "RemoteIO: dropped inbound data");
            }
        }));

        // 设置关闭回调
        let close_structured_io = structured_io.clone();
        transport.set_on_close(Box::new(move || {
            info!("RemoteIO: transport closed");
            let close_structured_io = close_structured_io.clone();
            tokio::spawn(async move {
                close_structured_io.mark_input_closed().await;
            });
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

        // Drain StructuredIO outbound messages to the remote transport. This
        // includes control_request/control_response/cancel messages generated
        // by StructuredIO while processing inbound protocol data.
        if let Some(outbound_rx) = structured_io.take_outbound_rx().await {
            spawn_remote_outbound_drain(outbound_rx, transport.clone(), ccr_client.clone());
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

async fn process_remote_transport_data(structured_io: StructuredIO, data: String) {
    let lines = remote_transport_lines(&data)
        .map(str::to_string)
        .collect::<Vec<_>>();
    for line in lines {
        if let Err(err) = structured_io.process_line(&line).await {
            warn!(
                error = %err,
                data_len = data.len(),
                "RemoteIO: failed to process inbound protocol line"
            );
        }
    }
}

fn remote_transport_lines(data: &str) -> impl Iterator<Item = &str> {
    data.lines().map(str::trim).filter(|line| !line.is_empty())
}

fn spawn_remote_outbound_drain(
    mut outbound_rx: mpsc::Receiver<crate::structured_io::StdoutMessage>,
    transport: Arc<dyn Transport>,
    ccr_client: Option<Arc<CCRClient>>,
) {
    tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            let value = match serde_json::to_value(&message) {
                Ok(value) => value,
                Err(err) => {
                    warn!(error = %err, "RemoteIO: failed to serialize outbound protocol message");
                    continue;
                }
            };
            let result = if let Some(ref client) = ccr_client {
                client.write_event(&value).await
            } else {
                transport.write(&value).await
            };
            if let Err(err) = result {
                warn!(error = %err, "RemoteIO: failed to write outbound protocol message");
                break;
            }
        }
    });
}

/// 获取会话入口认证 token。
fn get_session_ingress_auth_token() -> Option<String> {
    // 优先从环境变量获取
    std::env::var("MOSSEN_CODE_SESSION_ACCESS_TOKEN")
        .ok()
        .or_else(|| std::env::var("MOSSEN_CODE_SESSION_INGRESS_TOKEN").ok())
}

#[cfg(test)]
mod tests {
    use super::remote_transport_lines;

    #[test]
    fn remote_transport_lines_handles_ndjson_and_single_json() {
        let lines = remote_transport_lines(
            "  {\"type\":\"keep_alive\"}\n\n{\"type\":\"system\",\"content\":\"ok\"}\n",
        )
        .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "{\"type\":\"keep_alive\"}",
                "{\"type\":\"system\",\"content\":\"ok\"}"
            ]
        );

        let single = remote_transport_lines("{\"type\":\"keep_alive\"}").collect::<Vec<_>>();
        assert_eq!(single, vec!["{\"type\":\"keep_alive\"}"]);
    }
}
