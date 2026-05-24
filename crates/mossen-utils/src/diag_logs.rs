//! # diag_logs — 诊断日志
//!
//! 对应 TypeScript `utils/diagLogs.ts`。

use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

/// 诊断日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// 诊断日志条目
#[derive(Debug, Clone, Serialize)]
struct DiagnosticLogEntry {
    timestamp: String,
    level: DiagnosticLogLevel,
    event: String,
    data: HashMap<String, serde_json::Value>,
}

/// 获取诊断日志文件路径
fn get_diagnostic_log_file() -> Option<String> {
    env::var("MOSSEN_CODE_DIAGNOSTICS_FILE").ok()
}

/// 记录诊断信息到日志文件。此信息通过环境管理器发送到 session-ingress
/// 以监控容器内的问题。
///
/// *重要* - 此函数不得使用任何 PII 调用，包括文件路径、项目名称、仓库名称、提示等。
pub fn log_for_diagnostics_no_pii(
    level: DiagnosticLogLevel,
    event: &str,
    data: Option<HashMap<String, serde_json::Value>>,
) {
    let log_file = match get_diagnostic_log_file() {
        Some(f) => f,
        None => return,
    };

    let entry = DiagnosticLogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level,
        event: event.to_string(),
        data: data.unwrap_or_default(),
    };

    let line = match serde_json::to_string(&entry) {
        Ok(s) => format!("{}\n", s),
        Err(_) => return,
    };

    // 尝试追加写入
    if append_to_file(&log_file, &line).is_err() {
        // 如果追加失败，尝试先创建目录
        if let Some(parent) = Path::new(&log_file).parent() {
            let _ = fs::create_dir_all(parent);
            let _ = append_to_file(&log_file, &line);
        }
    }
}

fn append_to_file(path: &str, content: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(content.as_bytes())
}

/// 包装异步函数，添加诊断计时日志。
/// 在执行前记录 `{event}_started`，执行后记录 `{event}_completed`（含 duration_ms）。
pub async fn with_diagnostics_timing<T, F, Fut, G>(
    event: &str,
    f: F,
    get_data: Option<G>,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    G: FnOnce(&T) -> HashMap<String, serde_json::Value>,
{
    let start_time = Instant::now();
    log_for_diagnostics_no_pii(
        DiagnosticLogLevel::Info,
        &format!("{}_started", event),
        None,
    );

    match f().await {
        Ok(result) => {
            let mut additional_data = if let Some(get_data_fn) = get_data {
                get_data_fn(&result)
            } else {
                HashMap::new()
            };
            additional_data.insert(
                "duration_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(
                    start_time.elapsed().as_millis() as u64,
                )),
            );
            log_for_diagnostics_no_pii(
                DiagnosticLogLevel::Info,
                &format!("{}_completed", event),
                Some(additional_data),
            );
            Ok(result)
        }
        Err(error) => {
            let mut data = HashMap::new();
            data.insert(
                "duration_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(
                    start_time.elapsed().as_millis() as u64,
                )),
            );
            log_for_diagnostics_no_pii(
                DiagnosticLogLevel::Error,
                &format!("{}_failed", event),
                Some(data),
            );
            Err(error)
        }
    }
}
