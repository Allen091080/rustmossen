//! # auto_mode_denials — 自动模式拒绝记录
//!
//! 对应 TypeScript `utils/autoModeDenials.ts`。

use std::sync::Mutex;

/// 自动模式拒绝记录。
#[derive(Debug, Clone)]
pub struct AutoModeDenial {
    pub tool_name: String,
    /// 被拒绝命令的人类可读描述。
    pub display: String,
    pub reason: String,
    pub timestamp: u64,
}

const MAX_DENIALS: usize = 20;

static DENIALS: Mutex<Vec<AutoModeDenial>> = Mutex::new(Vec::new());

/// 记录一次自动模式拒绝。
pub fn record_auto_mode_denial(denial: AutoModeDenial) {
    let mut denials = DENIALS.lock().unwrap();
    denials.insert(0, denial);
    denials.truncate(MAX_DENIALS);
}

/// 获取所有自动模式拒绝记录。
pub fn get_auto_mode_denials() -> Vec<AutoModeDenial> {
    let denials = DENIALS.lock().unwrap();
    denials.clone()
}
