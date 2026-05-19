//! # aws_auth_status_manager — 云提供商认证状态管理
//!
//! 对应 TypeScript `utils/awsAuthStatusManager.ts`。
//! 管理 AWS Bedrock/GCP Vertex 认证刷新状态。

use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast;

/// 认证状态
#[derive(Debug, Clone)]
pub struct AwsAuthStatus {
    pub is_authenticating: bool,
    pub output: Vec<String>,
    pub error: Option<String>,
}

impl Default for AwsAuthStatus {
    fn default() -> Self {
        Self {
            is_authenticating: false,
            output: Vec::new(),
            error: None,
        }
    }
}

/// 认证状态管理器（单例）
pub struct AwsAuthStatusManager {
    status: Mutex<AwsAuthStatus>,
    sender: broadcast::Sender<AwsAuthStatus>,
}

static INSTANCE: OnceLock<Arc<AwsAuthStatusManager>> = OnceLock::new();

impl AwsAuthStatusManager {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(16);
        Self {
            status: Mutex::new(AwsAuthStatus::default()),
            sender,
        }
    }

    /// 获取单例实例
    pub fn get_instance() -> Arc<AwsAuthStatusManager> {
        INSTANCE
            .get_or_init(|| Arc::new(AwsAuthStatusManager::new()))
            .clone()
    }

    /// 获取当前状态
    pub fn get_status(&self) -> AwsAuthStatus {
        self.status.lock().unwrap().clone()
    }

    /// 开始认证
    pub fn start_authentication(&self) {
        let mut status = self.status.lock().unwrap();
        *status = AwsAuthStatus {
            is_authenticating: true,
            output: Vec::new(),
            error: None,
        };
        let _ = self.sender.send(status.clone());
    }

    /// 添加输出行
    pub fn add_output(&self, line: String) {
        let mut status = self.status.lock().unwrap();
        status.output.push(line);
        let _ = self.sender.send(status.clone());
    }

    /// 设置错误
    pub fn set_error(&self, error: String) {
        let mut status = self.status.lock().unwrap();
        status.error = Some(error);
        let _ = self.sender.send(status.clone());
    }

    /// 结束认证
    pub fn end_authentication(&self, success: bool) {
        let mut status = self.status.lock().unwrap();
        if success {
            *status = AwsAuthStatus::default();
        } else {
            status.is_authenticating = false;
        }
        let _ = self.sender.send(status.clone());
    }

    /// 订阅状态变更
    pub fn subscribe(&self) -> broadcast::Receiver<AwsAuthStatus> {
        self.sender.subscribe()
    }

    /// 重置（用于测试）
    pub fn reset() {
        // OnceLock 不支持重置，在测试中使用独立实例
    }
}
