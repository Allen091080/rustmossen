//! # API 模块
//!
//! 翻译自 `services/api/` 目录，提供：
//! - [`sdk`] — Mossen SDK 类型与错误类型
//! - [`errors`] — API 错误消息生成与分类
//! - [`error_utils`] — 连接错误提取与格式化
//! - [`openai`] — OpenAI 兼容客户端
//! - [`with_retry`] — 重试逻辑与指数退避
//! - [`client`] — Mossen API 客户端创建
//! - [`logging`] — API 日志记录
//! - [`files_api`] — 文件上传/下载 API
//! - [`prompt_cache`] — Prompt Cache Break 检测
//! - [`session_ingress`] — 会话持久化
//! - [`grove`] — Grove 隐私设置
//! - [`referral`] — 推荐系统
//! - [`bootstrap`] — 引导数据获取
//! - [`overage_credit`] — 超额信用额度
//! - [`admin_requests`] — 管理员请求
//! - [`usage`] — 使用量查询
//! - [`ultrareview_quota`] — Ultrareview 配额
//! - [`first_token_date`] — 首次 Token 日期
//! - [`empty_usage`] — 空使用量常量
//! - [`mossen_agent_sdk`] — Agent SDK 权限模式
//! - [`mossen_api`] — 核心 API 客户端（查询模型）

pub mod admin_requests;
pub mod bootstrap;
pub mod client;
pub mod empty_usage;
pub mod error_utils;
pub mod errors;
pub mod files_api;
pub mod first_token_date;
pub mod grove;
pub mod logging;
pub mod mossen_agent_sdk;
pub mod mossen_api;
pub mod openai;
pub mod overage_credit;
pub mod prompt_cache;
pub mod referral;
pub mod sdk;
pub mod session_ingress;
pub mod ultrareview_quota;
pub mod usage;
pub mod with_retry;
