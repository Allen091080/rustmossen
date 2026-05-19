//! FPS metrics context — frame rate tracking.
//!
//! Translates: context/fpsMetrics.tsx
//! React context → struct.

/// FPS metrics data.
#[derive(Debug, Clone, Default)]
pub struct FpsMetrics {
    pub fps: f64,
    pub frame_count: u64,
    pub dropped_frames: u64,
}

/// FPS metrics getter — provides access to current FPS metrics.
pub type FpsMetricsGetter = Box<dyn Fn() -> Option<FpsMetrics> + Send + Sync>;

/// FPS metrics holder (stores the getter for on-demand access).
pub struct FpsMetricsContext {
    getter: Option<FpsMetricsGetter>,
}

impl FpsMetricsContext {
    pub fn new(getter: Option<FpsMetricsGetter>) -> Self {
        Self { getter }
    }

    /// Get the current FPS metrics, if available.
    pub fn get_fps_metrics(&self) -> Option<FpsMetrics> {
        self.getter.as_ref().and_then(|f| f())
    }
}

/// Snapshot of the FPS-metrics context. Mirrors React `useFpsMetrics()`.
pub fn use_fps_metrics(ctx: &FpsMetricsContext) -> Option<FpsMetrics> {
    ctx.get_fps_metrics()
}

/// Provider entry-point. Mirrors React `FpsMetricsProvider`.
pub fn fps_metrics_provider(getter: FpsMetricsGetter) -> FpsMetricsContext {
    FpsMetricsContext::new(Some(getter))
}
