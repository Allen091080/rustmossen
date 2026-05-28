//! # profiler_base — Profiler 共享基础设施
//!
//! 对应 TypeScript `utils/profilerBase.ts`。

use std::time::Instant;

/// Profiler 计时基准 — 模拟 TS `getPerformance()`，返回一个共享 `Instant`。
///
/// 在 Rust 中 `std::time::Instant` 已是 monotonic clock，无需懒加载 `perf_hooks`。
/// 该函数提供与 TS 类似的入口，方便其他 profiler 模块基于共享起点计算偏移。
pub fn get_performance() -> Instant {
    use once_cell::sync::Lazy;
    static START: Lazy<Instant> = Lazy::new(Instant::now);
    *START
}

/// 格式化毫秒数为字符串（3位小数）。
pub fn format_ms(ms: f64) -> String {
    format!("{:.3}", ms)
}

/// 格式化文件大小为人类可读形式。
fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 内存使用信息。
#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub rss: usize,
    pub heap_used: usize,
}

/// 渲染单条时间线行，格式：
/// `[+  total.ms] (+  delta.ms) name [extra] [| RSS: .., Heap: ..]`
pub fn format_timeline_line(
    total_ms: f64,
    delta_ms: f64,
    name: &str,
    memory: Option<&MemoryUsage>,
    total_pad: usize,
    delta_pad: usize,
    extra: &str,
) -> String {
    let mem_info = match memory {
        Some(m) => format!(
            " | RSS: {}, Heap: {}",
            format_file_size(m.rss),
            format_file_size(m.heap_used)
        ),
        None => String::new(),
    };
    format!(
        "[+{}ms] (+{}ms) {}{}{}",
        pad_start(&format_ms(total_ms), total_pad),
        pad_start(&format_ms(delta_ms), delta_pad),
        name,
        extra,
        mem_info
    )
}

/// 左填充字符串。
fn pad_start(s: &str, width: usize) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        format!("{:>width$}", s, width = width)
    }
}
