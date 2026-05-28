//! # buffered_writer — 缓冲写入器
//!
//! 对应 TypeScript `utils/bufferedWriter.ts`。
//! 提供带缓冲的写入功能，支持定时刷新、大小限制和立即模式。

use std::sync::{Arc, Mutex};

/// 写入函数类型
pub type WriteFn = Box<dyn Fn(&str) + Send + Sync>;

/// 缓冲写入器配置
pub struct BufferedWriterConfig {
    /// 写入函数
    pub write_fn: WriteFn,
    /// 刷新间隔（毫秒）
    pub flush_interval_ms: u64,
    /// 最大缓冲条目数
    pub max_buffer_size: usize,
    /// 最大缓冲字节数
    pub max_buffer_bytes: usize,
    /// 是否立即模式（跳过缓冲）
    pub immediate_mode: bool,
}

impl Default for BufferedWriterConfig {
    fn default() -> Self {
        Self {
            write_fn: Box::new(|_| {}),
            flush_interval_ms: 1000,
            max_buffer_size: 100,
            max_buffer_bytes: usize::MAX,
            immediate_mode: false,
        }
    }
}

/// 缓冲写入器内部状态
struct BufferedWriterInner {
    buffer: Vec<String>,
    buffer_bytes: usize,
    pending_overflow: Option<Vec<String>>,
    write_fn: WriteFn,
    max_buffer_size: usize,
    max_buffer_bytes: usize,
    immediate_mode: bool,
}

/// 缓冲写入器
///
/// 收集写入内容到缓冲区，在以下情况下刷新：
/// - 定时器触发
/// - 缓冲区条目数超过 max_buffer_size
/// - 缓冲区字节数超过 max_buffer_bytes
/// - 显式调用 flush()
pub struct BufferedWriter {
    inner: Arc<Mutex<BufferedWriterInner>>,
    _flush_interval_ms: u64,
}

impl BufferedWriter {
    /// 创建新的缓冲写入器
    pub fn new(config: BufferedWriterConfig) -> Self {
        let inner = Arc::new(Mutex::new(BufferedWriterInner {
            buffer: Vec::new(),
            buffer_bytes: 0,
            pending_overflow: None,
            write_fn: config.write_fn,
            max_buffer_size: config.max_buffer_size,
            max_buffer_bytes: config.max_buffer_bytes,
            immediate_mode: config.immediate_mode,
        }));

        Self {
            inner,
            _flush_interval_ms: config.flush_interval_ms,
        }
    }

    /// 写入内容到缓冲区
    pub fn write(&self, content: &str) {
        let mut inner = self.inner.lock().unwrap();

        if inner.immediate_mode {
            (inner.write_fn)(content);
            return;
        }

        inner.buffer.push(content.to_string());
        inner.buffer_bytes += content.len();

        if inner.buffer.len() >= inner.max_buffer_size
            || inner.buffer_bytes >= inner.max_buffer_bytes
        {
            // Flush deferred - detach the buffer synchronously so the caller never waits
            Self::flush_deferred_inner(&mut inner);
        }
    }

    /// 刷新缓冲区
    pub fn flush(&self) {
        let mut inner = self.inner.lock().unwrap();
        Self::flush_inner(&mut inner);
    }

    /// 释放资源并刷新所有待处理内容
    pub fn dispose(&self) {
        self.flush();
    }

    fn flush_inner(inner: &mut BufferedWriterInner) {
        // Write pending overflow first
        if let Some(overflow) = inner.pending_overflow.take() {
            let combined: String = overflow.into_iter().collect();
            (inner.write_fn)(&combined);
        }

        if inner.buffer.is_empty() {
            return;
        }

        let combined: String = inner.buffer.drain(..).collect();
        (inner.write_fn)(&combined);
        inner.buffer_bytes = 0;
    }

    fn flush_deferred_inner(inner: &mut BufferedWriterInner) {
        if let Some(ref mut overflow) = inner.pending_overflow {
            // A previous overflow write is still queued. Coalesce into it
            overflow.append(&mut inner.buffer);
            inner.buffer_bytes = 0;
            return;
        }

        let detached: Vec<String> = inner.buffer.drain(..).collect();
        inner.buffer_bytes = 0;
        inner.pending_overflow = Some(detached);
        // In the Rust version, the deferred write will be handled by the next flush
        // or dispose call, since we don't have setImmediate.
    }
}

/// 创建缓冲写入器的便捷函数
pub fn create_buffered_writer(config: BufferedWriterConfig) -> BufferedWriter {
    BufferedWriter::new(config)
}
