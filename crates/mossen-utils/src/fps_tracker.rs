//! # fps_tracker — FPS 追踪器
//!
//! 对应 TypeScript `utils/fpsTracker.ts`。

/// FPS 指标。
#[derive(Debug, Clone)]
pub struct FpsMetrics {
    pub average_fps: f64,
    pub low_1_pct_fps: f64,
}

/// FPS 追踪器。
pub struct FpsTracker {
    frame_durations: Vec<f64>,
    first_render_time: Option<f64>,
    last_render_time: Option<f64>,
}

impl FpsTracker {
    pub fn new() -> Self {
        Self {
            frame_durations: Vec::new(),
            first_render_time: None,
            last_render_time: None,
        }
    }

    /// 记录一帧。
    pub fn record(&mut self, duration_ms: f64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0;
        if self.first_render_time.is_none() {
            self.first_render_time = Some(now);
        }
        self.last_render_time = Some(now);
        self.frame_durations.push(duration_ms);
    }

    /// 获取 FPS 指标。
    pub fn get_metrics(&self) -> Option<FpsMetrics> {
        if self.frame_durations.is_empty() {
            return None;
        }
        let (first, last) = match (self.first_render_time, self.last_render_time) {
            (Some(f), Some(l)) => (f, l),
            _ => return None,
        };

        let total_time_ms = last - first;
        if total_time_ms <= 0.0 {
            return None;
        }

        let total_frames = self.frame_durations.len() as f64;
        let average_fps = total_frames / (total_time_ms / 1000.0);

        let mut sorted = self.frame_durations.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let p99_index = ((sorted.len() as f64 * 0.01).ceil() as usize).max(1) - 1;
        let p99_frame_time_ms = sorted[p99_index];
        let low_1_pct_fps = if p99_frame_time_ms > 0.0 {
            1000.0 / p99_frame_time_ms
        } else {
            0.0
        };

        Some(FpsMetrics {
            average_fps: (average_fps * 100.0).round() / 100.0,
            low_1_pct_fps: (low_1_pct_fps * 100.0).round() / 100.0,
        })
    }
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self::new()
    }
}
