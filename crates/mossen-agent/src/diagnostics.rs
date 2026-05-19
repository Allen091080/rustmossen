//! # diagnostics — IDE 诊断跟踪服务
//!
//! 对应 TS `services/diagnosticTracking.ts`。跟踪文件编辑前后的
//! IDE 诊断变化（Error/Warning/Info/Hint），用于向模型反馈
//! 编辑引入的新问题。

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::error;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 诊断摘要最大字符数。
const MAX_DIAGNOSTICS_SUMMARY_CHARS: usize = 4000;

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

/// 单个诊断条目。
///
/// 对应 TS `Diagnostic`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub range: DiagnosticRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// 诊断严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// 诊断范围。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticRange {
    pub start: DiagnosticPosition,
    pub end: DiagnosticPosition,
}

/// 诊断位置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticPosition {
    pub line: u32,
    pub character: u32,
}

/// 文件诊断信息。
///
/// 对应 TS `DiagnosticFile`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticFile {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------
// DiagnosticTrackingService
// ---------------------------------------------------------------------------

/// 诊断跟踪服务（单例）。
///
/// 对应 TS `DiagnosticTrackingService` class。
pub struct DiagnosticTrackingService {
    /// 文件基线诊断。
    baseline: HashMap<String, Vec<Diagnostic>>,
    /// 已初始化标志。
    initialized: bool,
    /// right-file 诊断状态跟踪。
    right_file_diagnostics_state: HashMap<String, Vec<Diagnostic>>,
    /// 最后处理时间戳。
    last_processed_timestamps: HashMap<String, u64>,
}

/// 全局单例。
static INSTANCE: OnceLock<Mutex<DiagnosticTrackingService>> = OnceLock::new();

impl DiagnosticTrackingService {
    /// 获取全局单例。
    pub fn instance() -> &'static Mutex<DiagnosticTrackingService> {
        INSTANCE.get_or_init(|| {
            Mutex::new(DiagnosticTrackingService {
                baseline: HashMap::new(),
                initialized: false,
                right_file_diagnostics_state: HashMap::new(),
                last_processed_timestamps: HashMap::new(),
            })
        })
    }

    /// 初始化服务。
    pub fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
    }

    /// 关闭服务。
    pub fn shutdown(&mut self) {
        self.initialized = false;
        self.baseline.clear();
        self.right_file_diagnostics_state.clear();
        self.last_processed_timestamps.clear();
    }

    /// 重置跟踪状态（保持初始化）。
    pub fn reset(&mut self) {
        self.baseline.clear();
        self.right_file_diagnostics_state.clear();
        self.last_processed_timestamps.clear();
    }

    /// 规范化文件 URI。
    fn normalize_file_uri(file_uri: &str) -> String {
        let prefixes = ["file://", "_mossen_fs_right:", "_mossen_fs_left:"];
        let mut normalized = file_uri;
        for prefix in &prefixes {
            if let Some(stripped) = file_uri.strip_prefix(prefix) {
                normalized = stripped;
                break;
            }
        }
        // 平台感知路径规范化
        #[cfg(target_os = "windows")]
        {
            normalized.to_lowercase().replace('\\', "/")
        }
        #[cfg(not(target_os = "windows"))]
        {
            normalized.to_string()
        }
    }

    /// 记录文件编辑前的基线诊断。
    ///
    /// 对应 TS `beforeFileEdited()`。
    pub fn before_file_edited(&mut self, file_path: &str, diagnostics: Vec<Diagnostic>) {
        if !self.initialized {
            return;
        }
        let normalized = Self::normalize_file_uri(file_path);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.baseline.insert(normalized.clone(), diagnostics);
        self.last_processed_timestamps.insert(normalized, now);
    }

    /// 获取新诊断（与基线对比）。
    ///
    /// 对应 TS `getNewDiagnostics()`。
    pub fn get_new_diagnostics(
        &mut self,
        all_diagnostic_files: &[DiagnosticFile],
    ) -> Vec<DiagnosticFile> {
        if !self.initialized {
            return Vec::new();
        }

        let mut result = Vec::new();

        // 按 file:// 过滤有基线的文件
        let files_with_baselines: Vec<&DiagnosticFile> = all_diagnostic_files
            .iter()
            .filter(|f| {
                self.baseline
                    .contains_key(&Self::normalize_file_uri(&f.uri))
            })
            .filter(|f| f.uri.starts_with("file://"))
            .collect();

        // 收集 _mossen_fs_right 诊断
        let mut right_map: HashMap<String, &DiagnosticFile> = HashMap::new();
        for f in all_diagnostic_files {
            if f.uri.starts_with("_mossen_fs_right:")
                && self
                    .baseline
                    .contains_key(&Self::normalize_file_uri(&f.uri))
            {
                right_map.insert(Self::normalize_file_uri(&f.uri), f);
            }
        }

        for file in &files_with_baselines {
            let normalized = Self::normalize_file_uri(&file.uri);
            let baseline_diags = self.baseline.get(&normalized).cloned().unwrap_or_default();

            // 确定使用哪个诊断文件
            let file_to_use: &DiagnosticFile = if let Some(right_file) = right_map.get(&normalized)
            {
                let prev_right = self.right_file_diagnostics_state.get(&normalized);
                let use_right = match prev_right {
                    None => true,
                    Some(prev) => !Self::are_diagnostic_arrays_equal(prev, &right_file.diagnostics),
                };
                self.right_file_diagnostics_state
                    .insert(normalized.clone(), right_file.diagnostics.clone());
                if use_right {
                    right_file
                } else {
                    file
                }
            } else {
                file
            };

            // 找出新诊断
            let new_diags: Vec<Diagnostic> = file_to_use
                .diagnostics
                .iter()
                .filter(|d| {
                    !baseline_diags
                        .iter()
                        .any(|b| Self::are_diagnostics_equal(d, b))
                })
                .cloned()
                .collect();

            if !new_diags.is_empty() {
                result.push(DiagnosticFile {
                    uri: file.uri.clone(),
                    diagnostics: new_diags,
                });
            }

            // 更新基线
            self.baseline
                .insert(normalized, file_to_use.diagnostics.clone());
        }

        result
    }

    /// 比较两个诊断是否相等。
    fn are_diagnostics_equal(a: &Diagnostic, b: &Diagnostic) -> bool {
        a.message == b.message
            && a.severity == b.severity
            && a.source == b.source
            && a.code == b.code
            && a.range == b.range
    }

    /// 比较两个诊断数组是否相等（无序）。
    fn are_diagnostic_arrays_equal(a: &[Diagnostic], b: &[Diagnostic]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .all(|da| b.iter().any(|db| Self::are_diagnostics_equal(da, db)))
            && b.iter()
                .all(|db| a.iter().any(|da| Self::are_diagnostics_equal(da, db)))
    }

    /// 格式化诊断摘要。
    ///
    /// 对应 TS `formatDiagnosticsSummary()`。
    pub fn format_diagnostics_summary(files: &[DiagnosticFile]) -> String {
        let truncation_marker = "…[truncated]";

        let result: String = files
            .iter()
            .map(|file| {
                let filename = file.uri.rsplit('/').next().unwrap_or(&file.uri);

                let diags: String = file
                    .diagnostics
                    .iter()
                    .map(|d| {
                        let symbol = Self::get_severity_symbol(d.severity);
                        let code_part = d
                            .code
                            .as_ref()
                            .map(|c| format!(" [{}]", c))
                            .unwrap_or_default();
                        let source_part = d
                            .source
                            .as_ref()
                            .map(|s| format!(" ({})", s))
                            .unwrap_or_default();
                        format!(
                            "  {} [Line {}:{}] {}{}{}",
                            symbol,
                            d.range.start.line + 1,
                            d.range.start.character + 1,
                            d.message,
                            code_part,
                            source_part,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                format!("{}:\n{}", filename, diags)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        if result.len() > MAX_DIAGNOSTICS_SUMMARY_CHARS {
            let cut = MAX_DIAGNOSTICS_SUMMARY_CHARS - truncation_marker.len();
            format!("{}{}", &result[..cut], truncation_marker)
        } else {
            result
        }
    }

    /// 获取严重级别符号。
    pub fn get_severity_symbol(severity: DiagnosticSeverity) -> &'static str {
        match severity {
            DiagnosticSeverity::Error => "✖",
            DiagnosticSeverity::Warning => "⚠",
            DiagnosticSeverity::Info => "ℹ",
            DiagnosticSeverity::Hint => "★",
        }
    }
}
