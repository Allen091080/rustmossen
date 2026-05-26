//! Stream-json terminal patch renderer.
//!
//! This consumes `render_frame` values and produces line-oriented patch
//! operations that a terminal client can apply without repainting the whole
//! screen. It is intentionally viewport-light: region placement, scroll, and
//! cursor hints stay explicit so a real terminal scheduler can preserve prompt
//! position and scrollback.

use crossterm::{
    cursor, queue, style,
    terminal::{self, ClearType},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, Write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION: u32 = 1;
pub const STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION: u32 = 1;
pub const STREAM_JSON_RENDER_PATCH_TYPE: &str = "render_patch";
pub const STREAM_JSON_RENDER_DRAW_PLAN_TYPE: &str = "render_draw_plan";
pub const STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS: usize = 240;
pub const STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES: usize = 200;
pub const STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES: usize = 120;
pub const STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES: usize = 160;
pub const STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES: usize = 240;
pub const STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS: usize = 8;
pub const STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES: usize = 65_536;
pub const STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS: usize = 2_048;
const STREAM_JSON_TERMINAL_RENDER_COLOR_ENV: &str = "MOSSEN_TERMINAL_RENDER_COLOR";
const STREAM_JSON_TERMINAL_RENDER_UNICODE_ENV: &str = "MOSSEN_TERMINAL_RENDER_UNICODE";

#[derive(Debug, Clone, Default)]
pub struct StreamJsonTerminalPatchRenderer {
    last_applied_frame_hash: Option<String>,
    previous_top_region_layouts: BTreeMap<String, StreamJsonTerminalRegionLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamJsonTerminalRegionLayout {
    start_row: usize,
    line_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RenderPatchLineSet {
    lines: Vec<String>,
    max_width_cells: usize,
    any_truncated: bool,
    any_stripped: bool,
    source_line_count: usize,
    max_line_count: usize,
    omitted_line_count: usize,
}

impl StreamJsonTerminalPatchRenderer {
    pub fn new() -> Self {
        Self {
            last_applied_frame_hash: None,
            previous_top_region_layouts: BTreeMap::new(),
        }
    }

    pub fn render_frame_value(&mut self, frame: &Value) -> Value {
        self.render_frame_value_with_options(frame, None)
    }

    pub fn render_frame_value_forced(&mut self, frame: &Value, reason: &str) -> Value {
        self.render_frame_value_with_options(frame, Some(reason))
    }

    fn render_frame_value_with_options(
        &mut self,
        frame: &Value,
        force_redraw_reason: Option<&str>,
    ) -> Value {
        let frame_hash = string_field(frame, "frameHash");
        let frame_id = string_field(frame, "frameId");
        let sequence = frame.get("sequence").and_then(Value::as_u64).unwrap_or(0);
        let previous_applied_frame_hash = self.last_applied_frame_hash.clone().unwrap_or_default();
        let force_redraw = force_redraw_reason.is_some();
        let frame_hash_matches = !frame_hash.is_empty()
            && self.last_applied_frame_hash.as_deref() == Some(frame_hash.as_str());
        let frame_hash_unchanged = frame_hash_matches && !force_redraw;
        let first_frame = self.last_applied_frame_hash.is_none()
            || bool_path(frame, &["changes", "firstFrame"]).unwrap_or(false);
        let frame_dirty = bool_path(frame, &["draw", "dirty"]).unwrap_or(false);
        let current_top_region_layouts = render_frame_top_region_layouts(frame);
        let mut region_ids = render_patch_region_ids(frame, frame_hash_unchanged);
        let scrollback_append_once_suppressed = force_redraw
            && frame_hash_matches
            && render_patch_suppress_forced_scrollback_reappend(frame, &mut region_ids);
        if !frame_hash_unchanged {
            merge_render_patch_region_ids(
                &mut region_ids,
                render_patch_top_layout_changed_region_ids(
                    &current_top_region_layouts,
                    &self.previous_top_region_layouts,
                ),
            );
        }
        let operations = if frame_hash_unchanged {
            Vec::new()
        } else {
            render_patch_operations(
                frame,
                &region_ids,
                &self.previous_top_region_layouts,
                &current_top_region_layouts,
            )
        };
        let skipped = operations.is_empty();
        let operation_count = operations.len();
        let skip_reason = if frame_hash_unchanged {
            Value::String("frame_hash_unchanged".to_string())
        } else if skipped && force_redraw {
            Value::String("forced_redraw_no_regions".to_string())
        } else if skipped {
            Value::String("no_changed_regions".to_string())
        } else {
            Value::Null
        };
        let throttle_ms = value_path(frame, &["refresh", "throttleMs"])
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let flush_policy = if skipped {
            "skip"
        } else if force_redraw || frame_dirty || first_frame {
            "immediate"
        } else {
            "coalesced"
        };

        if !frame_hash.is_empty() {
            self.last_applied_frame_hash = Some(frame_hash.clone());
            self.previous_top_region_layouts = current_top_region_layouts;
        }

        json!({
            "type": STREAM_JSON_RENDER_PATCH_TYPE,
            "subtype": STREAM_JSON_RENDER_PATCH_TYPE,
            "schemaVersion": STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION,
            "frameSchemaVersion": frame.get("schemaVersion").cloned().unwrap_or(Value::Null),
            "sequence": sequence,
            "sourceFrame": {
                "frameId": frame_id,
                "frameHash": frame_hash,
                "previousAppliedFrameHash": previous_applied_frame_hash,
            },
            "draw": {
                "dirty": frame_dirty,
                "skipped": skipped,
                "skipReason": skip_reason,
                "preferredStrategy": "patch_regions",
                "replaceWholeScreen": false,
                "operationCount": operation_count,
                "changedRegionIds": region_ids,
                "skipIfFrameHashUnchanged": !force_redraw,
                "frameHashUnchanged": frame_hash_matches,
                "forcedRedraw": force_redraw,
                "forceRedrawReason": force_redraw_reason.unwrap_or_default(),
                "scrollbackAppendOnceSuppressed": scrollback_append_once_suppressed,
            },
            "operations": operations,
            "flush": {
                "shouldFlush": !skipped,
                "policy": flush_policy,
                "throttleMs": throttle_ms,
                "coalesceSafe": !first_frame,
            },
            "cursor": {
                "preservePrompt": true,
                "restoreAfterDraw": true,
                "avoidScrollbackJump": bool_path(frame, &["scroll", "stable"]).unwrap_or(true),
            },
            "scroll": render_patch_scroll_contract(frame, &operations),
            "style": {
                "semanticColors": true,
                "plainTextFallback": true,
                "resetAfterLine": true,
            },
            "terminal": frame.get("terminal").cloned().unwrap_or(Value::Null),
            "safety": {
                "ansiSafeLines": true,
                "controlCharsStripped": true,
                "c0ControlCharsNormalized": true,
                "tabsNormalizedToSpaces": true,
                "newlineWritesSuppressed": true,
                "bidiControlsStripped": true,
                "unsafeFormatControlsStripped": true,
                "unicodeBidiSpoofGuard": true,
                "maxPatchLineCells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
            },
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamJsonTerminalDrawScheduler {
    previous_region_line_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamJsonTerminalDrawLineBudget {
    remaining_noncritical_top_lines: usize,
}

impl Default for StreamJsonTerminalDrawLineBudget {
    fn default() -> Self {
        Self {
            remaining_noncritical_top_lines:
                STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES,
        }
    }
}

impl StreamJsonTerminalDrawLineBudget {
    fn available_noncritical_top_lines(&self) -> usize {
        self.remaining_noncritical_top_lines
    }

    fn consume_noncritical_top_lines(&mut self, count: usize) {
        self.remaining_noncritical_top_lines =
            self.remaining_noncritical_top_lines.saturating_sub(count);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamJsonTerminalTextByteBudget {
    max_bytes: usize,
    remaining_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamJsonTerminalTextByteBudgetResult {
    text: String,
    truncated: bool,
    omitted: bool,
}

impl StreamJsonTerminalTextByteBudget {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            remaining_bytes: max_bytes,
        }
    }

    fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    fn written_bytes(&self) -> usize {
        self.max_bytes.saturating_sub(self.remaining_bytes)
    }

    fn cap_text(&mut self, text: &str) -> StreamJsonTerminalTextByteBudgetResult {
        if text.is_empty() {
            return StreamJsonTerminalTextByteBudgetResult {
                text: String::new(),
                truncated: false,
                omitted: false,
            };
        }

        if self.remaining_bytes == 0 {
            return StreamJsonTerminalTextByteBudgetResult {
                text: String::new(),
                truncated: false,
                omitted: true,
            };
        }

        if text.len() <= self.remaining_bytes {
            self.remaining_bytes = self.remaining_bytes.saturating_sub(text.len());
            return StreamJsonTerminalTextByteBudgetResult {
                text: text.to_string(),
                truncated: false,
                omitted: false,
            };
        }

        let capped = terminal_truncate_text_to_byte_budget(text, self.remaining_bytes);
        self.remaining_bytes = 0;
        if capped.is_empty() {
            return StreamJsonTerminalTextByteBudgetResult {
                text: String::new(),
                truncated: false,
                omitted: true,
            };
        }

        StreamJsonTerminalTextByteBudgetResult {
            text: capped,
            truncated: true,
            omitted: false,
        }
    }
}

impl StreamJsonTerminalDrawScheduler {
    pub fn new() -> Self {
        Self {
            previous_region_line_counts: BTreeMap::new(),
        }
    }

    pub fn render_patch_value(&mut self, patch: &Value) -> Value {
        let skipped = bool_path(patch, &["draw", "skipped"]).unwrap_or(false);
        let should_flush = bool_path(patch, &["flush", "shouldFlush"]).unwrap_or(false);
        let forced_redraw = bool_path(patch, &["draw", "forcedRedraw"]).unwrap_or(false);
        let patch_operations = patch.get("operations").and_then(Value::as_array);
        let patch_operation_count = patch_operations.map_or(0, Vec::len);
        let mut region_plans = Vec::new();
        let mut terminal_ops = Vec::new();

        if !skipped && should_flush && patch_operation_count > 0 {
            let mut anchored_terminal_ops = Vec::new();
            let mut scrollback_terminal_ops = Vec::new();
            let mut draw_line_budget = StreamJsonTerminalDrawLineBudget::default();

            for operation in patch_operations.into_iter().flatten() {
                let appends_scrollback = patch_operation_appends_scrollback(operation);
                let target_ops = if appends_scrollback {
                    &mut scrollback_terminal_ops
                } else {
                    &mut anchored_terminal_ops
                };
                let region_plan = render_draw_region_plan(
                    operation,
                    &self.previous_region_line_counts,
                    &mut draw_line_budget,
                    target_ops,
                );
                if let Some(region_id) = region_plan.get("regionId").and_then(Value::as_str) {
                    let line_count = region_plan
                        .get("lineCount")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    self.previous_region_line_counts
                        .insert(region_id.to_string(), line_count);
                }
                region_plans.push(region_plan);
            }

            if !anchored_terminal_ops.is_empty() {
                terminal_ops.push(json!({
                    "op": "save_cursor",
                    "reason": "preserve_prompt",
                }));
                terminal_ops.push(json!({
                    "op": "begin_batch",
                    "replaceWholeScreen": false,
                    "mode": "anchored_region_patch",
                }));
                terminal_ops.extend(anchored_terminal_ops);
                terminal_ops.push(json!({
                    "op": "end_batch",
                }));
                terminal_ops.push(json!({
                    "op": "restore_cursor",
                    "reason": "preserve_prompt",
                }));
            }
            terminal_ops.extend(scrollback_terminal_ops);
        }
        let commits_scrollback = terminal_ops.iter().any(|operation| {
            operation.get("op").and_then(Value::as_str) == Some("append_scrollback_block")
        });
        let saves_cursor = terminal_ops
            .iter()
            .any(|operation| operation.get("op").and_then(Value::as_str) == Some("save_cursor"));
        let restores_cursor = terminal_ops
            .iter()
            .any(|operation| operation.get("op").and_then(Value::as_str) == Some("restore_cursor"));
        let blocking_region_ids = region_plans
            .iter()
            .filter(|plan| render_region_plan_is_blocking(plan))
            .filter_map(|plan| plan.get("regionId").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        let uses_dynamic_top_stack = region_plans
            .iter()
            .any(|plan| string_field(plan, "layoutMode") == "dynamic_top_stack");
        let top_region_count = region_plans
            .iter()
            .filter(|plan| string_field(plan, "anchor") == "top")
            .count();
        let top_region_line_count = region_plans
            .iter()
            .filter(|plan| string_field(plan, "anchor") == "top")
            .map(|plan| plan.get("lineCount").and_then(Value::as_u64).unwrap_or(0))
            .sum::<u64>();
        let region_line_budgeted_region_count = patch_operations
            .into_iter()
            .flatten()
            .filter(|operation| bool_path(operation, &["lineBudget", "exceeded"]).unwrap_or(false))
            .count();
        let region_line_budget_omitted_line_count = patch_operations
            .into_iter()
            .flatten()
            .filter_map(|operation| value_path(operation, &["lineBudget", "omittedLineCount"]))
            .filter_map(Value::as_u64)
            .sum::<u64>();
        let draw_line_budgeted_region_count = region_plans
            .iter()
            .filter(|plan| bool_path(plan, &["drawLineBudget", "exceeded"]).unwrap_or(false))
            .count();
        let draw_line_budget_omitted_line_count = region_plans
            .iter()
            .filter_map(|plan| value_path(plan, &["drawLineBudget", "omittedLineCount"]))
            .filter_map(Value::as_u64)
            .sum::<u64>();
        let mut plan = json!({
            "type": STREAM_JSON_RENDER_DRAW_PLAN_TYPE,
            "subtype": STREAM_JSON_RENDER_DRAW_PLAN_TYPE,
            "schemaVersion": STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION,
            "patchSchemaVersion": patch.get("schemaVersion").cloned().unwrap_or(Value::Null),
            "sequence": terminal_patch_sequence_value(patch),
            "sourceFrame": patch.get("sourceFrame").cloned().unwrap_or(Value::Null),
            "draw": {
                "skipped": skipped || terminal_ops.is_empty(),
                "skipReason": value_path(patch, &["draw", "skipReason"]).cloned().unwrap_or(Value::Null),
                "replaceWholeScreen": false,
                "viewportAdaptive": true,
                "operationCount": terminal_ops.len(),
                "regionPlanCount": region_plans.len(),
                "strategy": if commits_scrollback {
                    "anchored_region_patch_with_scrollback_commit"
                } else {
                    "anchored_region_patch"
                },
                "clearStaleRegionLines": true,
                "commitsScrollback": commits_scrollback,
                "hasBlockingRegion": !blocking_region_ids.is_empty(),
                "blockingRegionIds": blocking_region_ids,
                "topLayoutMode": if uses_dynamic_top_stack {
                    "dynamic_stack"
                } else {
                    "anchored_fallback"
                },
                "topLayoutCompactsGaps": uses_dynamic_top_stack,
                "topRegionCount": top_region_count,
                "topRegionLineCount": top_region_line_count,
                "topRegionOverflowPolicy": "clip_before_bottom_regions",
                "topRegionClipDiagnostics": true,
                "regionLineBudgeted": true,
                "regionLineBudgetPolicy": "cap_region_lines_before_terminal_ops",
                "regionLineBudgetMaxLines": STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES,
                "regionLineBudgetedRegionCount": region_line_budgeted_region_count,
                "regionLineBudgetOmittedLineCount": region_line_budget_omitted_line_count,
                "terminalOpsPrebudgetedLines": true,
                "terminalOpsLineBudgeted": true,
                "drawLineBudgetPolicy": "cap_noncritical_top_lines_before_terminal_ops",
                "drawLineBudgetScope": "cumulative_noncritical_top_widgets",
                "drawLineBudgetMaxNoncriticalTopLines": STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES,
                "drawLineBudgetMaxNoncriticalTopTotalLines": STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES,
                "drawLineBudgetedRegionCount": draw_line_budgeted_region_count,
                "drawLineBudgetOmittedLineCount": draw_line_budget_omitted_line_count,
                "scrollbackPhysicalLineBudgeted": true,
                "scrollbackPhysicalLineBudgetPolicy": "cap_scrollback_physical_lines_before_terminal_writes",
                "scrollbackPhysicalLineBudgetMaxLines": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES,
                "failSafeEndBatchAfterBudgetTruncation": true,
            },
            "schedule": {
                "shouldFlush": should_flush && !terminal_ops.is_empty(),
                "flushPolicy": string_path(patch, &["flush", "policy"]),
                "throttleMs": value_path(patch, &["flush", "throttleMs"]).cloned().unwrap_or(Value::Null),
                "coalesceSafe": bool_path(patch, &["flush", "coalesceSafe"]).unwrap_or(false),
                "dropWhenSuperseded": true,
                "minimumFrameIntervalMs": value_path(patch, &["flush", "throttleMs"]).cloned().unwrap_or(Value::Null),
            },
            "cursor": {
                "saveBeforeDraw": saves_cursor,
                "restoreAfterDraw": restores_cursor && !commits_scrollback,
                "restoreAfterAnchoredPatch": restores_cursor,
                "preservePrompt": bool_path(patch, &["cursor", "preservePrompt"]).unwrap_or(true),
                "failSafeRestoreAfterBudgetTruncation": true,
                "failSafeRestoreOnWriteError": true,
            },
            "scroll": patch.get("scroll").cloned().unwrap_or(Value::Null),
            "viewportAdaptation": terminal_draw_viewport_adaptation_contract_value(),
            "style": {
                "semanticColors": bool_path(patch, &["style", "semanticColors"]).unwrap_or(true),
                "plainTextFallback": bool_path(patch, &["style", "plainTextFallback"]).unwrap_or(true),
                "resetAfterLine": bool_path(patch, &["style", "resetAfterLine"]).unwrap_or(true),
                "failSafeResetOnWriteError": true,
                "graphemeClusterSafe": true,
                "asciiFallback": true,
            },
            "regionPlans": region_plans,
            "terminalOps": terminal_ops,
            "safety": {
                "ansiSafeLines": true,
                "terminalControlSequencesStripped": true,
                "oscControlSequencesStripped": true,
                "inlineControlsNormalized": true,
                "carriageReturnProgressNormalized": true,
                "backspaceProgressNormalized": true,
                "c0ControlCharsNormalized": true,
                "tabsNormalizedToSpaces": true,
                "newlineWritesSuppressed": true,
                "bidiControlsStripped": true,
                "unsafeFormatControlsStripped": true,
                "unicodeBidiSpoofGuard": true,
                "controlCharsStripped": true,
                "boundedLineWidth": true,
                "graphemeClusterWidthGuard": true,
                "asciiGlyphFallback": true,
                "semanticStyleResets": true,
                "styleResetFailSafe": true,
                "styleWriteErrorReset": true,
                "cursorRestoreOnWriteError": true,
                "noWholeScreenClear": true,
            },
        });

        if let Some(draw) = plan.get_mut("draw").and_then(Value::as_object_mut) {
            draw.insert("forcedRedraw".to_string(), Value::Bool(forced_redraw));
            draw.insert(
                "forceRedrawReason".to_string(),
                value_path(patch, &["draw", "forceRedrawReason"])
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            draw.insert(
                "scrollbackAppendOnceSuppressed".to_string(),
                Value::Bool(
                    bool_path(patch, &["draw", "scrollbackAppendOnceSuppressed"]).unwrap_or(false),
                ),
            );
            draw.insert("terminalOpBudgeted".to_string(), Value::Bool(true));
            draw.insert(
                "terminalOpBudgetPolicy".to_string(),
                Value::String("cap_terminal_ops_before_execution".to_string()),
            );
            draw.insert(
                "terminalOpBudgetMaxOps".to_string(),
                json!(STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS),
            );
            draw.insert("terminalTextByteBudgeted".to_string(), Value::Bool(true));
            draw.insert(
                "terminalTextByteBudgetPolicy".to_string(),
                Value::String("cap_terminal_text_bytes_before_terminal_writes".to_string()),
            );
            draw.insert(
                "terminalTextByteBudgetMaxBytes".to_string(),
                json!(STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES),
            );
            draw.insert(
                "scrollbackClearVisibleRowsBudgeted".to_string(),
                Value::Bool(true),
            );
            draw.insert(
                "scrollbackClearVisibleRowsBudgetPolicy".to_string(),
                Value::String(
                    "cap_scrollback_clear_visible_rows_before_terminal_writes".to_string(),
                ),
            );
            draw.insert(
                "scrollbackClearVisibleRowsBudgetMaxRows".to_string(),
                json!(STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS),
            );
            draw.insert(
                "terminalExecutorBudgetHardCaps".to_string(),
                Value::Bool(true),
            );
            draw.insert(
                "terminalExecutorBudgetHardCapPolicy".to_string(),
                Value::String("min_declared_budget_with_renderer_hard_cap".to_string()),
            );
            draw.insert(
                "terminalExecutorZeroCopyBudgeting".to_string(),
                Value::Bool(true),
            );
            draw.insert(
                "terminalExecutorZeroCopyBudgetingPolicy".to_string(),
                Value::String("borrow_terminal_ops_and_scrollback_lines_before_budget".to_string()),
            );
            draw.insert("drawPlanBorrowedPatchInputs".to_string(), Value::Bool(true));
            draw.insert(
                "drawPlanBorrowedPatchInputPolicy".to_string(),
                Value::String("borrow_patch_operations_and_lines_until_draw_json_emit".to_string()),
            );
            draw.insert(
                "drawRuntimeOwnedPendingSubmit".to_string(),
                Value::Bool(true),
            );
            draw.insert(
                "drawRuntimeOwnedPendingPolicy".to_string(),
                Value::String("move_owned_draw_plan_into_pending_queue".to_string()),
            );
            draw.insert(
                "scrollbackSoftWrapMaterializationBudgeted".to_string(),
                Value::Bool(true),
            );
            draw.insert(
                "scrollbackSoftWrapMaterializationBudgetPolicy".to_string(),
                Value::String(
                    "cap_soft_wrap_materialization_before_scrollback_allocation".to_string(),
                ),
            );
            draw.insert(
                "scrollbackSoftWrapStreamingSanitizer".to_string(),
                Value::Bool(true),
            );
            draw.insert(
                "scrollbackSoftWrapStreamingSanitizerPolicy".to_string(),
                Value::String("strip_terminal_controls_while_budgeting_soft_wrap".to_string()),
            );
        }
        if let Some(schedule) = plan.get_mut("schedule").and_then(Value::as_object_mut) {
            schedule.insert(
                "dropWhenSuperseded".to_string(),
                Value::Bool(!forced_redraw),
            );
            schedule.insert(
                "supersededSequenceBypass".to_string(),
                Value::Bool(forced_redraw),
            );
        }
        if let Some(safety) = plan.get_mut("safety").and_then(Value::as_object_mut) {
            safety.insert("boundedTerminalOps".to_string(), Value::Bool(true));
            safety.insert(
                "terminalOpBudget".to_string(),
                terminal_draw_terminal_op_budget_value(),
            );
            safety.insert("cursorRestoreFailSafe".to_string(), Value::Bool(true));
            safety.insert(
                "budgetTruncatedCursorRestore".to_string(),
                Value::Bool(true),
            );
            safety.insert("writeErrorCursorRestore".to_string(), Value::Bool(true));
            safety.insert("synchronizedUpdateFailSafe".to_string(), Value::Bool(true));
            safety.insert(
                "budgetTruncatedSynchronizedUpdateClose".to_string(),
                Value::Bool(true),
            );
            safety.insert("boundedTextBytes".to_string(), Value::Bool(true));
            safety.insert(
                "terminalTextByteBudget".to_string(),
                terminal_draw_text_byte_budget_value(),
            );
            safety.insert(
                "boundedScrollbackClearVisibleRows".to_string(),
                Value::Bool(true),
            );
            safety.insert(
                "scrollbackClearVisibleRowsBudget".to_string(),
                terminal_draw_scrollback_clear_visible_rows_budget_value(),
            );
            safety.insert(
                "executorBudgetHardCaps".to_string(),
                terminal_draw_executor_budget_hard_caps_value(),
            );
            safety.insert(
                "executorZeroCopyBudgeting".to_string(),
                terminal_draw_executor_zero_copy_budgeting_value(),
            );
            safety.insert(
                "drawPlanBorrowedPatchInputs".to_string(),
                terminal_draw_plan_borrowed_patch_inputs_value(),
            );
            safety.insert(
                "drawRuntimeOwnedPendingSubmit".to_string(),
                terminal_draw_runtime_owned_pending_submit_value(),
            );
            safety.insert(
                "scrollbackSoftWrapMaterializationBudget".to_string(),
                terminal_draw_scrollback_soft_wrap_materialization_budget_value(),
            );
            safety.insert(
                "scrollbackSoftWrapStreamingSanitizer".to_string(),
                terminal_draw_scrollback_soft_wrap_streaming_sanitizer_value(),
            );
        }

        plan
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamJsonTerminalViewport {
    pub rows: u16,
    pub columns: u16,
}

impl StreamJsonTerminalViewport {
    pub fn new(rows: u16, columns: u16) -> Self {
        Self {
            rows: rows.max(1),
            columns: columns.max(1),
        }
    }

    pub fn current() -> Self {
        let (columns, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        Self::new(rows, columns)
    }
}

impl Default for StreamJsonTerminalViewport {
    fn default() -> Self {
        Self::new(24, 80)
    }
}

fn terminal_viewport_width_profile(viewport: StreamJsonTerminalViewport) -> &'static str {
    terminal_viewport_width_profile_for_columns(viewport.columns)
}

fn terminal_viewport_width_profile_for_columns(columns: u16) -> &'static str {
    if columns >= 120 {
        "full"
    } else if columns >= 80 {
        "compact"
    } else {
        "minimal"
    }
}

fn terminal_viewport_status_line_policy(viewport: StreamJsonTerminalViewport) -> &'static str {
    match terminal_viewport_width_profile(viewport) {
        "full" => "full_status",
        "compact" => "compact_status",
        _ => "minimal_status",
    }
}

fn terminal_viewport_secondary_field_policy(viewport: StreamJsonTerminalViewport) -> &'static str {
    match terminal_viewport_width_profile(viewport) {
        "full" => "show_all",
        "compact" => "collapse_secondary",
        _ => "hide_secondary",
    }
}

fn terminal_draw_viewport_adaptation_contract_value() -> Value {
    json!({
        "viewportAdaptive": true,
        "widthProfiles": ["full", "compact", "minimal"],
        "fullColumnsFrom": 120,
        "compactColumnsFrom": 80,
        "minimalColumnsBelow": 80,
        "statusLinePolicy": "choose_shortest_fitting_variant",
        "secondaryFieldPolicy": "drop_secondary_before_truncation",
        "overflowPolicy": "clip_top_stack_before_bottom_regions",
        "lineWrapPolicy": "soft_wrap_scrollback_bound_anchored_lines",
        "resizePolicy": "recompute_profile_before_pending_flush",
        "immediateResizeRedrawPolicy": "force_redraw_current_frame_with_latest_viewport",
    })
}

fn terminal_viewport_variant_key_order(viewport: StreamJsonTerminalViewport) -> [&'static str; 3] {
    match terminal_viewport_width_profile(viewport) {
        "full" => ["full", "compact", "minimal"],
        "compact" => ["compact", "minimal", "full"],
        _ => ["minimal", "compact", "full"],
    }
}

fn terminal_draw_text_for_viewport(
    operation: &Value,
    viewport: StreamJsonTerminalViewport,
    fallback: &str,
) -> (String, bool, bool) {
    let Some(variants) = operation.get("textVariants").and_then(Value::as_object) else {
        return (fallback.to_string(), false, false);
    };

    let mut first_candidate = None;
    for key in terminal_viewport_variant_key_order(viewport) {
        let Some(candidate) = terminal_text_variant_text(variants, key) else {
            continue;
        };
        first_candidate.get_or_insert_with(|| candidate.clone());
        if UnicodeWidthStr::width(candidate.as_str()) <= usize::from(viewport.columns) {
            return (candidate, true, false);
        }
    }

    if let Some(candidate) = first_candidate {
        return (candidate, true, true);
    }

    (fallback.to_string(), false, true)
}

fn terminal_text_variant_text(
    variants: &serde_json::Map<String, Value>,
    key: &str,
) -> Option<String> {
    variants
        .get(key)
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("text").and_then(Value::as_str))
        })
        .map(str::to_string)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamJsonTerminalDrawExecutionReport {
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub viewport_rows: u16,
    pub viewport_columns: u16,
    pub viewport_width_profile: String,
    pub viewport_status_line_policy: String,
    pub viewport_secondary_field_policy: String,
    pub viewport_variant_line_count: usize,
    pub viewport_variant_selection_count: usize,
    pub viewport_variant_fallback_count: usize,
    pub terminal_op_count: usize,
    pub terminal_op_budgeted: bool,
    pub terminal_op_budget_max: usize,
    pub terminal_op_budget_exceeded: bool,
    pub terminal_op_budget_omitted_count: usize,
    pub executed_terminal_op_count: usize,
    pub written_line_count: usize,
    pub cleared_line_count: usize,
    pub bounded_line_count: usize,
    pub clipped_row_count: usize,
    pub top_clipped_row_count: usize,
    pub reserved_bottom_rows: u16,
    pub visible_top_row_budget: u16,
    pub invalid_terminal_op_count: usize,
    pub scrollback_append_count: usize,
    pub scrollback_line_count: usize,
    pub scrollback_wrapped_line_count: usize,
    pub scrollback_clear_visible_rows_budgeted: bool,
    pub scrollback_clear_visible_rows_budget_max: usize,
    pub scrollback_clear_visible_rows_budget_exceeded: bool,
    pub scrollback_clear_visible_rows_budget_omitted_count: usize,
    pub scrollback_physical_line_budgeted: bool,
    pub scrollback_physical_line_budget_max: usize,
    pub scrollback_physical_line_budget_exceeded: bool,
    pub scrollback_physical_line_budget_omitted_source_line_count: usize,
    pub scrollback_physical_line_budget_omitted_wrapped_line_count: usize,
    pub terminal_text_byte_budgeted: bool,
    pub terminal_text_byte_budget_max: usize,
    pub terminal_text_byte_budget_written_bytes: usize,
    pub terminal_text_byte_budget_exceeded: bool,
    pub terminal_text_byte_budget_truncated_write_count: usize,
    pub terminal_text_byte_budget_omitted_write_count: usize,
    pub synchronized_update: bool,
    pub synchronized_update_fail_safe_count: usize,
    pub saved_cursor: bool,
    pub restored_cursor: bool,
    pub cursor_restore_fail_safe_count: usize,
    pub styled_line_count: usize,
    pub style_reset_count: usize,
    pub ascii_fallback_count: usize,
    pub control_sequence_stripped_count: usize,
    pub inline_control_normalized_count: usize,
    pub control_char_normalized_count: usize,
    pub format_control_stripped_count: usize,
    pub flushed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamJsonTerminalDrawRuntimeReport {
    pub accepted: bool,
    pub queued: bool,
    pub queued_owned_draw_plan: bool,
    pub queued_cloned_draw_plan: bool,
    pub applied: bool,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub pending_sequence: Option<u64>,
    pub next_flush_due_ms: Option<u64>,
    pub dropped_pending_count: usize,
    pub execution: Option<StreamJsonTerminalDrawExecutionReport>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamJsonTerminalDrawRuntimeSnapshot {
    pub has_pending_draw: bool,
    pub manual_scroll_active: bool,
    pub last_flush_at_ms: Option<u64>,
    pub next_flush_due_ms: Option<u64>,
    pub runtime_report_count: u64,
    pub runtime_applied_report_count: u64,
    pub runtime_queued_report_count: u64,
    pub runtime_skipped_report_count: u64,
    pub runtime_manual_scroll_preserved_report_count: u64,
    pub runtime_manual_scroll_teardown_release_count: u64,
    pub runtime_dropped_pending_count: usize,
    pub last_runtime_report: Option<StreamJsonTerminalDrawRuntimeReport>,
}

#[derive(Debug, Clone)]
pub struct StreamJsonTerminalDrawExecutor {
    viewport: StreamJsonTerminalViewport,
    last_applied_sequence: Option<u64>,
    semantic_colors_enabled: bool,
    unicode_enabled: bool,
}

impl StreamJsonTerminalDrawExecutor {
    pub fn new(viewport: StreamJsonTerminalViewport) -> Self {
        Self {
            viewport,
            last_applied_sequence: None,
            semantic_colors_enabled: terminal_render_semantic_colors_enabled(),
            unicode_enabled: terminal_render_unicode_enabled(),
        }
    }

    pub fn with_semantic_colors(
        viewport: StreamJsonTerminalViewport,
        semantic_colors_enabled: bool,
    ) -> Self {
        Self::with_terminal_capabilities(viewport, semantic_colors_enabled, true)
    }

    pub fn with_terminal_capabilities(
        viewport: StreamJsonTerminalViewport,
        semantic_colors_enabled: bool,
        unicode_enabled: bool,
    ) -> Self {
        Self {
            viewport,
            last_applied_sequence: None,
            semantic_colors_enabled,
            unicode_enabled,
        }
    }

    pub fn for_current_terminal() -> Self {
        Self::new(StreamJsonTerminalViewport::current())
    }

    pub fn viewport(&self) -> StreamJsonTerminalViewport {
        self.viewport
    }

    pub fn set_viewport(&mut self, viewport: StreamJsonTerminalViewport) {
        self.viewport = viewport;
    }

    pub fn apply_draw_plan<W: Write>(
        &mut self,
        draw_plan: &Value,
        writer: &mut W,
    ) -> io::Result<StreamJsonTerminalDrawExecutionReport> {
        let mut report = StreamJsonTerminalDrawExecutionReport::default();
        report.viewport_rows = self.viewport.rows;
        report.viewport_columns = self.viewport.columns;
        report.viewport_width_profile = terminal_viewport_width_profile(self.viewport).to_string();
        report.viewport_status_line_policy =
            terminal_viewport_status_line_policy(self.viewport).to_string();
        report.viewport_secondary_field_policy =
            terminal_viewport_secondary_field_policy(self.viewport).to_string();
        let mut text_byte_budget =
            StreamJsonTerminalTextByteBudget::new(terminal_draw_text_byte_budget(draw_plan));
        report.terminal_text_byte_budgeted = true;
        report.terminal_text_byte_budget_max = text_byte_budget.max_bytes();
        let terminal_ops = draw_plan.get("terminalOps").and_then(Value::as_array);
        let terminal_op_count = terminal_ops.map_or(0, Vec::len);
        report.terminal_op_count = terminal_op_count;
        let terminal_op_budget = terminal_draw_terminal_op_budget(draw_plan);
        report.terminal_op_budgeted = true;
        report.terminal_op_budget_max = terminal_op_budget;
        if terminal_op_count > terminal_op_budget {
            report.terminal_op_budget_exceeded = true;
            report.terminal_op_budget_omitted_count =
                terminal_op_count.saturating_sub(terminal_op_budget);
        }
        let reserved_bottom_rows =
            terminal_draw_reserved_bottom_rows(draw_plan, self.viewport.rows);
        report.reserved_bottom_rows = reserved_bottom_rows;
        report.visible_top_row_budget = self.viewport.rows.saturating_sub(reserved_bottom_rows);

        let plan_skipped = bool_path(draw_plan, &["draw", "skipped"]).unwrap_or(false);
        let should_flush = bool_path(draw_plan, &["schedule", "shouldFlush"]).unwrap_or(false);
        if plan_skipped || !should_flush || terminal_op_count == 0 {
            report.skipped = true;
            report.skip_reason = value_path(draw_plan, &["draw", "skipReason"])
                .and_then(Value::as_str)
                .filter(|reason| !reason.is_empty())
                .map(str::to_string)
                .or_else(|| Some("draw_plan_not_flushable".to_string()));
            return Ok(report);
        }

        let sequence = draw_plan.get("sequence").and_then(Value::as_u64);
        let drop_when_superseded =
            bool_path(draw_plan, &["schedule", "dropWhenSuperseded"]).unwrap_or(true);
        if drop_when_superseded {
            if let (Some(sequence), Some(last_applied)) = (sequence, self.last_applied_sequence) {
                if sequence <= last_applied {
                    report.skipped = true;
                    report.skip_reason = Some("superseded_sequence".to_string());
                    return Ok(report);
                }
            }
        }

        let mut synchronized_update_open = false;
        let mut saved_cursor_open = false;
        let mut current_row_visible = false;
        let mut current_row_clipped_top = false;
        let terminal_ops = terminal_ops.expect("nonempty terminal ops were checked");
        for operation in terminal_ops.iter().take(terminal_op_budget) {
            let op = operation
                .get("op")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match op {
                "save_cursor" => {
                    terminal_draw_queue_or_fail_closed(
                        writer,
                        &mut synchronized_update_open,
                        &mut saved_cursor_open,
                        |writer| queue!(writer, cursor::SavePosition),
                    )?;
                    saved_cursor_open = true;
                    report.saved_cursor = true;
                    report.executed_terminal_op_count += 1;
                }
                "restore_cursor" => {
                    terminal_draw_queue_or_fail_closed(
                        writer,
                        &mut synchronized_update_open,
                        &mut saved_cursor_open,
                        |writer| queue!(writer, cursor::RestorePosition),
                    )?;
                    saved_cursor_open = false;
                    report.restored_cursor = true;
                    report.executed_terminal_op_count += 1;
                }
                "begin_batch" => {
                    terminal_draw_queue_or_fail_closed(
                        writer,
                        &mut synchronized_update_open,
                        &mut saved_cursor_open,
                        |writer| queue!(writer, terminal::BeginSynchronizedUpdate),
                    )?;
                    synchronized_update_open = true;
                    report.synchronized_update = true;
                    report.executed_terminal_op_count += 1;
                }
                "end_batch" if synchronized_update_open => {
                    terminal_draw_queue_or_fail_closed(
                        writer,
                        &mut synchronized_update_open,
                        &mut saved_cursor_open,
                        |writer| queue!(writer, terminal::EndSynchronizedUpdate),
                    )?;
                    synchronized_update_open = false;
                    report.executed_terminal_op_count += 1;
                }
                "move_to_row" => {
                    let row = operation
                        .get("row")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let is_top_row = row.trim().starts_with("top+");
                    if let Some(row) =
                        resolve_terminal_draw_row(row, self.viewport, reserved_bottom_rows)
                    {
                        terminal_draw_queue_or_fail_closed(
                            writer,
                            &mut synchronized_update_open,
                            &mut saved_cursor_open,
                            |writer| queue!(writer, cursor::MoveTo(0, row)),
                        )?;
                        current_row_visible = true;
                        current_row_clipped_top = false;
                        report.executed_terminal_op_count += 1;
                    } else {
                        current_row_visible = false;
                        current_row_clipped_top = is_top_row;
                        report.clipped_row_count += 1;
                        if is_top_row {
                            report.top_clipped_row_count += 1;
                        }
                    }
                }
                "clear_line" => {
                    if current_row_visible {
                        terminal_draw_queue_or_fail_closed(
                            writer,
                            &mut synchronized_update_open,
                            &mut saved_cursor_open,
                            |writer| queue!(writer, terminal::Clear(ClearType::CurrentLine)),
                        )?;
                        report.cleared_line_count += 1;
                        report.executed_terminal_op_count += 1;
                    } else {
                        report.clipped_row_count += 1;
                        if current_row_clipped_top {
                            report.top_clipped_row_count += 1;
                        }
                    }
                }
                "write_line" => {
                    if current_row_visible {
                        let text = operation
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let (text, variant_selected, variant_fallback) =
                            terminal_draw_text_for_viewport(operation, self.viewport, text);
                        if variant_selected {
                            report.viewport_variant_line_count =
                                report.viewport_variant_line_count.saturating_add(1);
                            report.viewport_variant_selection_count =
                                report.viewport_variant_selection_count.saturating_add(1);
                        }
                        if variant_fallback {
                            report.viewport_variant_fallback_count =
                                report.viewport_variant_fallback_count.saturating_add(1);
                        }
                        let semantic_style = operation
                            .get("semanticStyle")
                            .and_then(Value::as_str)
                            .unwrap_or("plain");
                        let (
                            bounded,
                            truncated,
                            stripped,
                            ascii_fallback_count,
                            control_sequence_stripped_count,
                            inline_control_normalized_count,
                            control_char_normalized_count,
                            format_control_stripped_count,
                        ) = terminal_draw_bounded_line_with_unicode(
                            &text,
                            self.viewport.columns as usize,
                            self.unicode_enabled,
                        );
                        if truncated || stripped || ascii_fallback_count > 0 {
                            report.bounded_line_count += 1;
                        }
                        report.ascii_fallback_count = report
                            .ascii_fallback_count
                            .saturating_add(ascii_fallback_count);
                        report.control_sequence_stripped_count = report
                            .control_sequence_stripped_count
                            .saturating_add(control_sequence_stripped_count);
                        report.inline_control_normalized_count = report
                            .inline_control_normalized_count
                            .saturating_add(inline_control_normalized_count);
                        report.control_char_normalized_count = report
                            .control_char_normalized_count
                            .saturating_add(control_char_normalized_count);
                        report.format_control_stripped_count = report
                            .format_control_stripped_count
                            .saturating_add(format_control_stripped_count);
                        let semantic_color = if self.semantic_colors_enabled {
                            terminal_semantic_color(semantic_style)
                        } else {
                            None
                        };
                        let Some(bounded) = terminal_draw_budget_text_for_write(
                            &bounded,
                            &mut text_byte_budget,
                            &mut report,
                        ) else {
                            continue;
                        };
                        let mut semantic_style_open = false;
                        if let Some(color) = semantic_color {
                            terminal_draw_queue_or_fail_closed(
                                writer,
                                &mut synchronized_update_open,
                                &mut saved_cursor_open,
                                |writer| queue!(writer, style::SetForegroundColor(color)),
                            )?;
                            semantic_style_open = true;
                            report.styled_line_count += 1;
                        }
                        if let Err(err) = queue!(writer, style::Print(bounded)) {
                            if semantic_style_open {
                                terminal_draw_fail_reset_style(
                                    writer,
                                    &mut synchronized_update_open,
                                    &mut saved_cursor_open,
                                );
                            } else {
                                terminal_draw_fail_cleanup(
                                    writer,
                                    &mut synchronized_update_open,
                                    &mut saved_cursor_open,
                                    false,
                                );
                            }
                            return Err(err);
                        }
                        if semantic_style_open {
                            terminal_draw_queue_or_fail_closed(
                                writer,
                                &mut synchronized_update_open,
                                &mut saved_cursor_open,
                                |writer| queue!(writer, style::ResetColor),
                            )?;
                            report.style_reset_count += 1;
                        }
                        report.written_line_count += 1;
                        report.executed_terminal_op_count += 1;
                    } else {
                        report.clipped_row_count += 1;
                        if current_row_clipped_top {
                            report.top_clipped_row_count += 1;
                        }
                    }
                }
                "append_scrollback_block" => {
                    let requested_clear_visible_rows = operation
                        .get("clearVisibleBottomRows")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .min(u64::from(self.viewport.rows))
                        as usize;
                    let clear_visible_rows_budget =
                        terminal_draw_scrollback_clear_visible_rows_budget(operation);
                    report.scrollback_clear_visible_rows_budgeted = true;
                    report.scrollback_clear_visible_rows_budget_max = report
                        .scrollback_clear_visible_rows_budget_max
                        .max(clear_visible_rows_budget);
                    if requested_clear_visible_rows > clear_visible_rows_budget {
                        report.scrollback_clear_visible_rows_budget_exceeded = true;
                        report.scrollback_clear_visible_rows_budget_omitted_count = report
                            .scrollback_clear_visible_rows_budget_omitted_count
                            .saturating_add(
                                requested_clear_visible_rows
                                    .saturating_sub(clear_visible_rows_budget),
                            );
                    }
                    let clear_visible_rows =
                        requested_clear_visible_rows.min(clear_visible_rows_budget) as u16;
                    for offset in (0..clear_visible_rows).rev() {
                        let row = self.viewport.rows - 1 - offset;
                        terminal_draw_queue_or_fail_closed(
                            writer,
                            &mut synchronized_update_open,
                            &mut saved_cursor_open,
                            |writer| queue!(writer, cursor::MoveTo(0, row)),
                        )?;
                        terminal_draw_queue_or_fail_closed(
                            writer,
                            &mut synchronized_update_open,
                            &mut saved_cursor_open,
                            |writer| queue!(writer, terminal::Clear(ClearType::CurrentLine)),
                        )?;
                        report.cleared_line_count += 1;
                    }
                    terminal_draw_queue_or_fail_closed(
                        writer,
                        &mut synchronized_update_open,
                        &mut saved_cursor_open,
                        |writer| queue!(writer, cursor::MoveTo(0, self.viewport.rows - 1)),
                    )?;
                    let lines = operation.get("lines").and_then(Value::as_array);
                    let line_count = lines.map_or(0, Vec::len);
                    let physical_line_budget =
                        terminal_draw_scrollback_physical_line_budget(operation);
                    report.scrollback_physical_line_budgeted = true;
                    report.scrollback_physical_line_budget_max = report
                        .scrollback_physical_line_budget_max
                        .max(physical_line_budget);
                    let mut remaining_physical_lines = physical_line_budget;
                    let mut text_byte_budget_exhausted = false;
                    for (line_index, line) in lines.into_iter().flatten().enumerate() {
                        let text = line.as_str().unwrap_or_default();
                        let (
                            wrapped_lines,
                            wrapped_count,
                            omitted_wrapped_line_count,
                            stripped,
                            ascii_fallback_count,
                            control_sequence_stripped_count,
                            inline_control_normalized_count,
                            control_char_normalized_count,
                            format_control_stripped_count,
                        ) = terminal_draw_soft_wrapped_lines_with_unicode_budget(
                            text,
                            self.viewport.columns as usize,
                            self.unicode_enabled,
                            remaining_physical_lines,
                        );
                        if wrapped_count > 0 {
                            report.scrollback_wrapped_line_count = report
                                .scrollback_wrapped_line_count
                                .saturating_add(wrapped_count);
                        }
                        if stripped || ascii_fallback_count > 0 {
                            report.bounded_line_count += 1;
                        }
                        report.ascii_fallback_count = report
                            .ascii_fallback_count
                            .saturating_add(ascii_fallback_count);
                        report.control_sequence_stripped_count = report
                            .control_sequence_stripped_count
                            .saturating_add(control_sequence_stripped_count);
                        report.inline_control_normalized_count = report
                            .inline_control_normalized_count
                            .saturating_add(inline_control_normalized_count);
                        report.control_char_normalized_count = report
                            .control_char_normalized_count
                            .saturating_add(control_char_normalized_count);
                        report.format_control_stripped_count = report
                            .format_control_stripped_count
                            .saturating_add(format_control_stripped_count);
                        if remaining_physical_lines == 0 {
                            report.scrollback_physical_line_budget_exceeded = true;
                            report.scrollback_physical_line_budget_omitted_source_line_count =
                                report
                                    .scrollback_physical_line_budget_omitted_source_line_count
                                    .saturating_add(line_count.saturating_sub(line_index));
                            break;
                        }
                        let wrapped_line_count = wrapped_lines.len();
                        let writable_line_count = wrapped_line_count.min(remaining_physical_lines);
                        if writable_line_count < wrapped_line_count
                            || omitted_wrapped_line_count > 0
                        {
                            report.scrollback_physical_line_budget_exceeded = true;
                            report.scrollback_physical_line_budget_omitted_wrapped_line_count =
                                report
                                    .scrollback_physical_line_budget_omitted_wrapped_line_count
                                    .saturating_add(
                                        wrapped_line_count.saturating_sub(writable_line_count),
                                    )
                                    .saturating_add(omitted_wrapped_line_count);
                            report.scrollback_physical_line_budget_omitted_source_line_count =
                                report
                                    .scrollback_physical_line_budget_omitted_source_line_count
                                    .saturating_add(line_count.saturating_sub(line_index + 1));
                        }
                        for bounded in wrapped_lines.into_iter().take(writable_line_count) {
                            let Some(bounded) = terminal_draw_budget_text_for_write(
                                &bounded,
                                &mut text_byte_budget,
                                &mut report,
                            ) else {
                                text_byte_budget_exhausted = true;
                                break;
                            };
                            terminal_draw_queue_or_fail_closed(
                                writer,
                                &mut synchronized_update_open,
                                &mut saved_cursor_open,
                                |writer| queue!(writer, terminal::Clear(ClearType::CurrentLine)),
                            )?;
                            terminal_draw_queue_or_fail_closed(
                                writer,
                                &mut synchronized_update_open,
                                &mut saved_cursor_open,
                                |writer| queue!(writer, style::Print(bounded)),
                            )?;
                            terminal_draw_queue_or_fail_closed(
                                writer,
                                &mut synchronized_update_open,
                                &mut saved_cursor_open,
                                |writer| queue!(writer, terminal::Clear(ClearType::UntilNewLine)),
                            )?;
                            terminal_draw_queue_or_fail_closed(
                                writer,
                                &mut synchronized_update_open,
                                &mut saved_cursor_open,
                                |writer| queue!(writer, style::Print("\r\n")),
                            )?;
                            report.written_line_count += 1;
                            report.cleared_line_count += 1;
                        }
                        if text_byte_budget_exhausted {
                            break;
                        }
                        remaining_physical_lines =
                            remaining_physical_lines.saturating_sub(writable_line_count);
                        if writable_line_count < wrapped_line_count {
                            break;
                        }
                        if omitted_wrapped_line_count > 0 {
                            break;
                        }
                    }
                    current_row_visible = true;
                    report.scrollback_append_count += 1;
                    report.scrollback_line_count =
                        report.scrollback_line_count.saturating_add(line_count);
                    report.executed_terminal_op_count += 1;
                }
                _ => {
                    report.invalid_terminal_op_count += 1;
                }
            }
        }

        if synchronized_update_open {
            terminal_draw_queue_or_fail_closed(
                writer,
                &mut synchronized_update_open,
                &mut saved_cursor_open,
                |writer| queue!(writer, terminal::EndSynchronizedUpdate),
            )?;
            report.synchronized_update = true;
            report.synchronized_update_fail_safe_count =
                report.synchronized_update_fail_safe_count.saturating_add(1);
            report.executed_terminal_op_count = report.executed_terminal_op_count.saturating_add(1);
        }

        if saved_cursor_open {
            terminal_draw_queue_or_fail_closed(
                writer,
                &mut synchronized_update_open,
                &mut saved_cursor_open,
                |writer| queue!(writer, cursor::RestorePosition),
            )?;
            report.restored_cursor = true;
            report.cursor_restore_fail_safe_count =
                report.cursor_restore_fail_safe_count.saturating_add(1);
            report.executed_terminal_op_count = report.executed_terminal_op_count.saturating_add(1);
        }

        writer.flush()?;
        report.flushed = true;
        if let Some(sequence) = sequence {
            self.last_applied_sequence = Some(sequence);
        }
        Ok(report)
    }
}

pub fn render_draw_plan_to_terminal_bytes(
    draw_plan: &Value,
    viewport: StreamJsonTerminalViewport,
) -> io::Result<(Vec<u8>, StreamJsonTerminalDrawExecutionReport)> {
    let mut executor = StreamJsonTerminalDrawExecutor::new(viewport);
    let mut bytes = Vec::new();
    let report = executor.apply_draw_plan(draw_plan, &mut bytes)?;
    Ok((bytes, report))
}

fn terminal_draw_queue_or_fail_closed<W: Write>(
    writer: &mut W,
    synchronized_update_open: &mut bool,
    saved_cursor_open: &mut bool,
    op: impl FnOnce(&mut W) -> io::Result<()>,
) -> io::Result<()> {
    match op(writer) {
        Ok(()) => Ok(()),
        Err(err) => {
            terminal_draw_fail_cleanup(writer, synchronized_update_open, saved_cursor_open, false);
            Err(err)
        }
    }
}

fn terminal_draw_fail_close_synchronized_update<W: Write>(
    writer: &mut W,
    synchronized_update_open: &mut bool,
) {
    if *synchronized_update_open {
        let _ = queue!(writer, terminal::EndSynchronizedUpdate);
        *synchronized_update_open = false;
    }
}

fn terminal_draw_fail_cleanup<W: Write>(
    writer: &mut W,
    synchronized_update_open: &mut bool,
    saved_cursor_open: &mut bool,
    reset_style: bool,
) {
    if reset_style {
        let _ = queue!(writer, style::ResetColor);
    }
    terminal_draw_fail_close_synchronized_update(writer, synchronized_update_open);
    if *saved_cursor_open {
        let _ = queue!(writer, cursor::RestorePosition);
        *saved_cursor_open = false;
    }
    let _ = writer.flush();
}

fn terminal_draw_fail_reset_style<W: Write>(
    writer: &mut W,
    synchronized_update_open: &mut bool,
    saved_cursor_open: &mut bool,
) {
    terminal_draw_fail_cleanup(writer, synchronized_update_open, saved_cursor_open, true);
}

#[derive(Debug, Clone)]
pub struct StreamJsonTerminalDrawRuntime {
    executor: StreamJsonTerminalDrawExecutor,
    pending_draw_plan: Option<Value>,
    last_flush_at_ms: Option<u64>,
    next_flush_due_ms: Option<u64>,
    manual_scroll_active: bool,
    last_runtime_report: Option<StreamJsonTerminalDrawRuntimeReport>,
    runtime_report_count: u64,
    runtime_applied_report_count: u64,
    runtime_queued_report_count: u64,
    runtime_skipped_report_count: u64,
    runtime_manual_scroll_preserved_report_count: u64,
    runtime_manual_scroll_teardown_release_count: u64,
    runtime_dropped_pending_count: usize,
}

impl StreamJsonTerminalDrawRuntime {
    pub fn new(viewport: StreamJsonTerminalViewport) -> Self {
        Self {
            executor: StreamJsonTerminalDrawExecutor::new(viewport),
            pending_draw_plan: None,
            last_flush_at_ms: None,
            next_flush_due_ms: None,
            manual_scroll_active: false,
            last_runtime_report: None,
            runtime_report_count: 0,
            runtime_applied_report_count: 0,
            runtime_queued_report_count: 0,
            runtime_skipped_report_count: 0,
            runtime_manual_scroll_preserved_report_count: 0,
            runtime_manual_scroll_teardown_release_count: 0,
            runtime_dropped_pending_count: 0,
        }
    }

    pub fn for_current_terminal() -> Self {
        Self::new(StreamJsonTerminalViewport::current())
    }

    pub fn viewport(&self) -> StreamJsonTerminalViewport {
        self.executor.viewport()
    }

    pub fn set_viewport(&mut self, viewport: StreamJsonTerminalViewport) {
        self.executor.set_viewport(viewport);
    }

    pub fn set_manual_scroll_active(&mut self, active: bool) {
        self.manual_scroll_active = active;
    }

    pub fn release_manual_scroll_for_terminal_teardown(&mut self) -> bool {
        let released_hold = self.manual_scroll_active && self.pending_draw_plan.is_some();
        self.manual_scroll_active = false;
        if released_hold {
            self.runtime_manual_scroll_teardown_release_count = self
                .runtime_manual_scroll_teardown_release_count
                .saturating_add(1);
        }
        released_hold
    }

    pub fn has_pending_draw(&self) -> bool {
        self.pending_draw_plan.is_some()
    }

    pub fn last_runtime_report(&self) -> Option<&StreamJsonTerminalDrawRuntimeReport> {
        self.last_runtime_report.as_ref()
    }

    pub fn runtime_snapshot(&self) -> StreamJsonTerminalDrawRuntimeSnapshot {
        StreamJsonTerminalDrawRuntimeSnapshot {
            has_pending_draw: self.has_pending_draw(),
            manual_scroll_active: self.manual_scroll_active,
            last_flush_at_ms: self.last_flush_at_ms,
            next_flush_due_ms: self.next_flush_due_ms,
            runtime_report_count: self.runtime_report_count,
            runtime_applied_report_count: self.runtime_applied_report_count,
            runtime_queued_report_count: self.runtime_queued_report_count,
            runtime_skipped_report_count: self.runtime_skipped_report_count,
            runtime_manual_scroll_preserved_report_count: self
                .runtime_manual_scroll_preserved_report_count,
            runtime_manual_scroll_teardown_release_count: self
                .runtime_manual_scroll_teardown_release_count,
            runtime_dropped_pending_count: self.runtime_dropped_pending_count,
            last_runtime_report: self.last_runtime_report.clone(),
        }
    }

    pub fn runtime_diagnostics_value(&self) -> Value {
        terminal_draw_runtime_snapshot_value(&self.runtime_snapshot())
    }

    pub fn submit_draw_plan_at<W: Write>(
        &mut self,
        draw_plan: &Value,
        now_ms: u64,
        writer: &mut W,
    ) -> io::Result<StreamJsonTerminalDrawRuntimeReport> {
        let should_flush = bool_path(draw_plan, &["schedule", "shouldFlush"]).unwrap_or(false);
        let skipped = bool_path(draw_plan, &["draw", "skipped"]).unwrap_or(false);
        if skipped || !should_flush {
            return Ok(
                self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
                    accepted: true,
                    skipped: true,
                    skip_reason: draw_plan_skip_reason(draw_plan)
                        .or_else(|| Some("draw_plan_not_flushable".to_string())),
                    ..Default::default()
                }),
            );
        }

        if self.manual_scroll_active && draw_plan_preserves_manual_scroll(draw_plan) {
            return Ok(self.queue_draw_plan(draw_plan, None, "manual_scroll_preserved"));
        }

        let throttle_ms = draw_plan_throttle_ms(draw_plan);
        let coalesce_safe = bool_path(draw_plan, &["schedule", "coalesceSafe"]).unwrap_or(false);
        let flush_policy = string_path(draw_plan, &["schedule", "flushPolicy"]);
        let elapsed_ms = self
            .last_flush_at_ms
            .map(|last| now_ms.saturating_sub(last))
            .unwrap_or(u64::MAX);
        let can_apply_now = flush_policy == "immediate"
            || !coalesce_safe
            || throttle_ms == 0
            || elapsed_ms >= throttle_ms;

        if can_apply_now {
            let mut report = self.apply_now(draw_plan, now_ms, writer)?;
            if self.pending_draw_plan.take().is_some() {
                report.dropped_pending_count = report.dropped_pending_count.saturating_add(1);
            }
            self.next_flush_due_ms = None;
            return Ok(self.record_runtime_report(report));
        }

        let due_ms = self
            .last_flush_at_ms
            .unwrap_or(now_ms)
            .saturating_add(throttle_ms);
        Ok(self.queue_draw_plan(draw_plan, Some(due_ms), "coalesced_until_throttle_deadline"))
    }

    pub fn submit_draw_plan_value_at<W: Write>(
        &mut self,
        draw_plan: Value,
        now_ms: u64,
        writer: &mut W,
    ) -> io::Result<StreamJsonTerminalDrawRuntimeReport> {
        let should_flush = bool_path(&draw_plan, &["schedule", "shouldFlush"]).unwrap_or(false);
        let skipped = bool_path(&draw_plan, &["draw", "skipped"]).unwrap_or(false);
        if skipped || !should_flush {
            return Ok(
                self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
                    accepted: true,
                    skipped: true,
                    skip_reason: draw_plan_skip_reason(&draw_plan)
                        .or_else(|| Some("draw_plan_not_flushable".to_string())),
                    ..Default::default()
                }),
            );
        }

        if self.manual_scroll_active && draw_plan_preserves_manual_scroll(&draw_plan) {
            return Ok(self.queue_draw_plan_value(draw_plan, None, "manual_scroll_preserved"));
        }

        let throttle_ms = draw_plan_throttle_ms(&draw_plan);
        let coalesce_safe = bool_path(&draw_plan, &["schedule", "coalesceSafe"]).unwrap_or(false);
        let flush_policy = string_path(&draw_plan, &["schedule", "flushPolicy"]);
        let elapsed_ms = self
            .last_flush_at_ms
            .map(|last| now_ms.saturating_sub(last))
            .unwrap_or(u64::MAX);
        let can_apply_now = flush_policy == "immediate"
            || !coalesce_safe
            || throttle_ms == 0
            || elapsed_ms >= throttle_ms;

        if can_apply_now {
            let mut report = self.apply_now(&draw_plan, now_ms, writer)?;
            if self.pending_draw_plan.take().is_some() {
                report.dropped_pending_count = report.dropped_pending_count.saturating_add(1);
            }
            self.next_flush_due_ms = None;
            return Ok(self.record_runtime_report(report));
        }

        let due_ms = self
            .last_flush_at_ms
            .unwrap_or(now_ms)
            .saturating_add(throttle_ms);
        Ok(
            self.queue_draw_plan_value(
                draw_plan,
                Some(due_ms),
                "coalesced_until_throttle_deadline",
            ),
        )
    }

    pub fn flush_pending_at<W: Write>(
        &mut self,
        now_ms: u64,
        writer: &mut W,
    ) -> io::Result<StreamJsonTerminalDrawRuntimeReport> {
        let Some(pending) = self.pending_draw_plan.take() else {
            return Ok(
                self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
                    skipped: true,
                    skip_reason: Some("no_pending_draw_plan".to_string()),
                    ..Default::default()
                }),
            );
        };

        if self.manual_scroll_active && draw_plan_preserves_manual_scroll(&pending) {
            let sequence = draw_plan_sequence(&pending);
            self.pending_draw_plan = Some(pending);
            self.next_flush_due_ms = None;
            return Ok(
                self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
                    accepted: true,
                    queued: true,
                    queued_owned_draw_plan: true,
                    skip_reason: Some("manual_scroll_preserved".to_string()),
                    pending_sequence: sequence,
                    next_flush_due_ms: None,
                    ..Default::default()
                }),
            );
        }

        if let Some(due_ms) = self.next_flush_due_ms {
            if now_ms < due_ms {
                let sequence = draw_plan_sequence(&pending);
                self.pending_draw_plan = Some(pending);
                return Ok(
                    self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
                        accepted: true,
                        queued: true,
                        queued_owned_draw_plan: true,
                        skip_reason: Some("throttle_deadline_not_reached".to_string()),
                        pending_sequence: sequence,
                        next_flush_due_ms: Some(due_ms),
                        ..Default::default()
                    }),
                );
            }
        }

        self.next_flush_due_ms = None;
        let report = self.apply_now(&pending, now_ms, writer)?;
        Ok(self.record_runtime_report(report))
    }

    fn queue_draw_plan(
        &mut self,
        draw_plan: &Value,
        next_flush_due_ms: Option<u64>,
        reason: &str,
    ) -> StreamJsonTerminalDrawRuntimeReport {
        let dropped_pending_count =
            usize::from(self.pending_draw_plan.replace(draw_plan.clone()).is_some());
        self.next_flush_due_ms = next_flush_due_ms;
        self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
            accepted: true,
            queued: true,
            queued_cloned_draw_plan: true,
            skip_reason: Some(reason.to_string()),
            pending_sequence: draw_plan_sequence(draw_plan),
            next_flush_due_ms,
            dropped_pending_count,
            ..Default::default()
        })
    }

    fn queue_draw_plan_value(
        &mut self,
        draw_plan: Value,
        next_flush_due_ms: Option<u64>,
        reason: &str,
    ) -> StreamJsonTerminalDrawRuntimeReport {
        let pending_sequence = draw_plan_sequence(&draw_plan);
        let dropped_pending_count =
            usize::from(self.pending_draw_plan.replace(draw_plan).is_some());
        self.next_flush_due_ms = next_flush_due_ms;
        self.record_runtime_report(StreamJsonTerminalDrawRuntimeReport {
            accepted: true,
            queued: true,
            queued_owned_draw_plan: true,
            skip_reason: Some(reason.to_string()),
            pending_sequence,
            next_flush_due_ms,
            dropped_pending_count,
            ..Default::default()
        })
    }

    fn record_runtime_report(
        &mut self,
        report: StreamJsonTerminalDrawRuntimeReport,
    ) -> StreamJsonTerminalDrawRuntimeReport {
        self.runtime_report_count = self.runtime_report_count.saturating_add(1);
        if report.applied {
            self.runtime_applied_report_count = self.runtime_applied_report_count.saturating_add(1);
        }
        if report.queued {
            self.runtime_queued_report_count = self.runtime_queued_report_count.saturating_add(1);
        }
        if report.skipped {
            self.runtime_skipped_report_count = self.runtime_skipped_report_count.saturating_add(1);
        }
        if report.skip_reason.as_deref() == Some("manual_scroll_preserved") {
            self.runtime_manual_scroll_preserved_report_count = self
                .runtime_manual_scroll_preserved_report_count
                .saturating_add(1);
        }
        self.runtime_dropped_pending_count = self
            .runtime_dropped_pending_count
            .saturating_add(report.dropped_pending_count);
        self.last_runtime_report = Some(report.clone());
        report
    }

    fn apply_now<W: Write>(
        &mut self,
        draw_plan: &Value,
        now_ms: u64,
        writer: &mut W,
    ) -> io::Result<StreamJsonTerminalDrawRuntimeReport> {
        let execution = self.executor.apply_draw_plan(draw_plan, writer)?;
        let skipped = execution.skipped;
        if execution.flushed {
            self.last_flush_at_ms = Some(now_ms);
        }
        Ok(StreamJsonTerminalDrawRuntimeReport {
            accepted: true,
            applied: !skipped,
            skipped,
            skip_reason: execution.skip_reason.clone(),
            execution: Some(execution),
            ..Default::default()
        })
    }
}

fn terminal_draw_runtime_snapshot_value(snapshot: &StreamJsonTerminalDrawRuntimeSnapshot) -> Value {
    json!({
        "hasPendingDraw": snapshot.has_pending_draw,
        "manualScrollActive": snapshot.manual_scroll_active,
        "lastFlushAtMs": snapshot.last_flush_at_ms,
        "nextFlushDueMs": snapshot.next_flush_due_ms,
        "reportCount": snapshot.runtime_report_count,
        "appliedReportCount": snapshot.runtime_applied_report_count,
        "queuedReportCount": snapshot.runtime_queued_report_count,
        "skippedReportCount": snapshot.runtime_skipped_report_count,
        "manualScrollPreservedReportCount": snapshot.runtime_manual_scroll_preserved_report_count,
        "manualScrollTeardownReleaseCount": snapshot.runtime_manual_scroll_teardown_release_count,
        "droppedPendingCount": snapshot.runtime_dropped_pending_count,
        "lastReport": snapshot
            .last_runtime_report
            .as_ref()
            .map(terminal_draw_runtime_report_value),
    })
}

fn terminal_draw_runtime_report_value(report: &StreamJsonTerminalDrawRuntimeReport) -> Value {
    json!({
        "accepted": report.accepted,
        "queued": report.queued,
        "queuedOwnedDrawPlan": report.queued_owned_draw_plan,
        "queuedClonedDrawPlan": report.queued_cloned_draw_plan,
        "applied": report.applied,
        "skipped": report.skipped,
        "skipReason": report.skip_reason,
        "pendingSequence": report.pending_sequence,
        "nextFlushDueMs": report.next_flush_due_ms,
        "droppedPendingCount": report.dropped_pending_count,
        "execution": report
            .execution
            .as_ref()
            .map(terminal_draw_execution_report_value),
    })
}

fn terminal_draw_execution_report_value(report: &StreamJsonTerminalDrawExecutionReport) -> Value {
    json!({
        "skipped": report.skipped,
        "skipReason": report.skip_reason,
        "viewportRows": report.viewport_rows,
        "viewportColumns": report.viewport_columns,
        "viewportWidthProfile": report.viewport_width_profile,
        "terminalOpCount": report.terminal_op_count,
        "terminalOpBudgeted": report.terminal_op_budgeted,
        "terminalOpBudgetMax": report.terminal_op_budget_max,
        "terminalOpBudgetExceeded": report.terminal_op_budget_exceeded,
        "terminalOpBudgetOmittedCount": report.terminal_op_budget_omitted_count,
        "executedTerminalOpCount": report.executed_terminal_op_count,
        "writtenLineCount": report.written_line_count,
        "clearedLineCount": report.cleared_line_count,
        "boundedLineCount": report.bounded_line_count,
        "clippedRowCount": report.clipped_row_count,
        "topClippedRowCount": report.top_clipped_row_count,
        "reservedBottomRows": report.reserved_bottom_rows,
        "visibleTopRowBudget": report.visible_top_row_budget,
        "scrollbackAppendCount": report.scrollback_append_count,
        "scrollbackLineCount": report.scrollback_line_count,
        "scrollbackWrappedLineCount": report.scrollback_wrapped_line_count,
        "terminalTextByteBudgeted": report.terminal_text_byte_budgeted,
        "terminalTextByteBudgetMax": report.terminal_text_byte_budget_max,
        "terminalTextByteBudgetWrittenBytes": report.terminal_text_byte_budget_written_bytes,
        "terminalTextByteBudgetExceeded": report.terminal_text_byte_budget_exceeded,
        "synchronizedUpdate": report.synchronized_update,
        "synchronizedUpdateFailSafeCount": report.synchronized_update_fail_safe_count,
        "savedCursor": report.saved_cursor,
        "restoredCursor": report.restored_cursor,
        "cursorRestoreFailSafeCount": report.cursor_restore_fail_safe_count,
        "styleResetCount": report.style_reset_count,
        "asciiFallbackCount": report.ascii_fallback_count,
        "controlSequenceStrippedCount": report.control_sequence_stripped_count,
        "controlCharNormalizedCount": report.control_char_normalized_count,
        "formatControlStrippedCount": report.format_control_stripped_count,
        "flushed": report.flushed,
    })
}

fn render_draw_region_plan(
    operation: &Value,
    previous_region_line_counts: &BTreeMap<String, usize>,
    draw_line_budget: &mut StreamJsonTerminalDrawLineBudget,
    terminal_ops: &mut Vec<Value>,
) -> Value {
    let region_id = string_field(operation, "regionId");
    let anchor = string_field(operation, "anchor");
    let role = string_field(operation, "role");
    let lines = operation.get("lines").and_then(Value::as_array);
    let source_line_count = lines.map_or(0, Vec::len);
    let update_mode = string_field(operation, "updateMode");
    let previous_line_count =
        render_draw_previous_region_line_count(operation, previous_region_line_counts, &region_id);
    let mut planned_lines = Vec::with_capacity(source_line_count);
    let top_start_row = operation
        .get("topStartRow")
        .and_then(Value::as_u64)
        .map(|row| row as usize);
    let previous_top_rows_to_clear = operation
        .get("previousTopRowsToClear")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_u64)
                .map(|row| row as usize)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let line_budget = operation
        .get("lineBudget")
        .cloned()
        .unwrap_or_else(|| json!({ "bounded": false }));
    let draw_line_budget_max = render_draw_operation_line_budget(operation, draw_line_budget);

    if update_mode == "append_scrollback" || anchor == "scrollback" || role == "transcript" {
        for (line_index, line) in lines.into_iter().flatten().enumerate() {
            let text = line.as_str().unwrap_or_default();
            let semantic_style = terminal_semantic_style_for_line(&role, &update_mode, text);
            planned_lines.push(json!({
                "index": line_index,
                "row": "scrollback",
                "text": text,
                "semanticStyle": semantic_style,
                "widthCells": UnicodeWidthStr::width(text),
            }));
        }
        terminal_ops.push(json!({
            "op": "append_scrollback_block",
            "regionId": region_id,
            "clearVisibleBottomRows": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS,
            "clearVisibleRowsBudget": terminal_draw_scrollback_clear_visible_rows_budget_value(),
            "wrapLongLines": true,
            "wrapMode": "soft_viewport_columns",
            "physicalLineBudget": terminal_draw_scrollback_physical_line_budget_value(source_line_count),
            "lines": render_draw_lines_value(lines),
        }));

        return json!({
            "regionId": region_id,
            "role": role,
            "anchor": anchor,
            "placement": string_field(operation, "placement"),
            "updateMode": update_mode,
            "regionHash": string_field(operation, "regionHash"),
            "lineCount": source_line_count,
            "sourceLineCount": source_line_count,
            "previousLineCount": previous_line_count,
            "clearTrailingLineCount": 0,
            "startRow": "scrollback",
            "commitsToScrollback": true,
            "lineBudget": line_budget,
            "scrollbackPhysicalLineBudget": terminal_draw_scrollback_physical_line_budget_value(source_line_count),
            "drawLineBudget": render_draw_line_budget_value(None, source_line_count, source_line_count, 0),
            "lines": planned_lines,
        });
    }

    let mut rendered_line_count = 0usize;
    let mut omitted_line_count = 0usize;
    for (line_index, line) in lines.into_iter().flatten().enumerate() {
        let text = line.as_str().unwrap_or_default();
        let semantic_style = terminal_semantic_style_for_line(&role, &update_mode, text);
        let row_expression = region_row_expression(
            &region_id,
            &role,
            &anchor,
            source_line_count,
            line_index,
            top_start_row,
        );
        let width_cells = UnicodeWidthStr::width(text);
        let mut write_op = json!({
            "op": "write_line",
            "regionId": region_id,
            "text": text,
            "semanticStyle": semantic_style,
            "widthCells": width_cells,
        });
        let mut planned_line = json!({
            "index": line_index,
            "row": row_expression.clone(),
            "text": text,
            "semanticStyle": semantic_style,
            "widthCells": width_cells,
        });
        if draw_line_budget_max
            .map(|max_lines| rendered_line_count >= max_lines)
            .unwrap_or(false)
        {
            omitted_line_count = omitted_line_count.saturating_add(1);
            if let Value::Object(map) = &mut planned_line {
                map.insert("terminalOpsOmitted".to_string(), Value::Bool(true));
                map.insert(
                    "omissionReason".to_string(),
                    Value::String("noncritical_top_cumulative_line_budget_exceeded".to_string()),
                );
            }
            planned_lines.push(planned_line);
            continue;
        }
        if let Some(line_variants) = render_draw_line_variants(operation, line_index) {
            if let Value::Object(map) = &mut write_op {
                map.insert("textVariants".to_string(), line_variants.clone());
                map.insert(
                    "textVariantPolicy".to_string(),
                    Value::String("choose_shortest_fitting_variant".to_string()),
                );
            }
            if let Value::Object(map) = &mut planned_line {
                map.insert("textVariants".to_string(), line_variants);
                map.insert(
                    "textVariantPolicy".to_string(),
                    Value::String("choose_shortest_fitting_variant".to_string()),
                );
            }
        }
        terminal_ops.push(json!({
            "op": "move_to_row",
            "regionId": region_id,
            "row": row_expression,
        }));
        terminal_ops.push(json!({
            "op": "clear_line",
            "regionId": region_id,
        }));
        terminal_ops.push(write_op);
        planned_lines.push(planned_line);
        rendered_line_count = rendered_line_count.saturating_add(1);
    }

    let clear_trailing_line_count = previous_line_count.saturating_sub(rendered_line_count);
    if draw_line_budget_max.is_some() {
        draw_line_budget.consume_noncritical_top_lines(rendered_line_count);
    }
    for stale_index in 0..clear_trailing_line_count {
        let row_expression = stale_region_row_expression(
            &region_id,
            &role,
            &anchor,
            rendered_line_count,
            stale_index,
            top_start_row,
        );
        terminal_ops.push(json!({
            "op": "move_to_row",
            "regionId": region_id,
            "row": row_expression,
        }));
        terminal_ops.push(json!({
            "op": "clear_line",
            "regionId": region_id,
            "stale": true,
        }));
    }

    for row in &previous_top_rows_to_clear {
        terminal_ops.push(json!({
            "op": "move_to_row",
            "regionId": region_id,
            "row": format!("top+{row}"),
            "previousTopRow": true,
        }));
        terminal_ops.push(json!({
            "op": "clear_line",
            "regionId": region_id,
            "stale": true,
            "previousTopRow": true,
        }));
    }

    json!({
        "regionId": region_id,
        "role": role,
        "anchor": anchor,
        "placement": string_field(operation, "placement"),
        "updateMode": update_mode,
        "regionHash": string_field(operation, "regionHash"),
        "lineCount": rendered_line_count,
        "sourceLineCount": source_line_count,
        "previousLineCount": previous_line_count,
        "clearTrailingLineCount": clear_trailing_line_count,
        "startRow": region_start_row_expression(&region_id, &role, &anchor, rendered_line_count, top_start_row),
        "topStartRow": top_start_row,
        "previousTopRowsToClear": previous_top_rows_to_clear,
        "layoutMode": terminal_region_layout_mode(&anchor, top_start_row),
        "lineBudget": line_budget,
        "drawLineBudget": render_draw_line_budget_value(
            draw_line_budget_max,
            source_line_count,
            rendered_line_count,
            omitted_line_count
        ),
        "lines": planned_lines,
    })
}

fn render_draw_line_variants(operation: &Value, line_index: usize) -> Option<Value> {
    if line_index != 0 {
        return None;
    }
    operation.get("lineVariants").cloned()
}

fn render_draw_lines_value(lines: Option<&Vec<Value>>) -> Value {
    Value::Array(
        lines
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|line| Value::String(line.to_string()))
            .collect(),
    )
}

fn render_draw_operation_line_budget(
    operation: &Value,
    draw_line_budget: &StreamJsonTerminalDrawLineBudget,
) -> Option<usize> {
    render_patch_operation_is_noncritical_widget_update(operation).then(|| {
        STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES
            .min(draw_line_budget.available_noncritical_top_lines())
    })
}

fn render_draw_line_budget_value(
    max_lines: Option<usize>,
    source_line_count: usize,
    rendered_line_count: usize,
    omitted_line_count: usize,
) -> Value {
    if let Some(max_lines) = max_lines {
        return json!({
            "bounded": true,
            "policy": "cap_noncritical_top_lines_before_terminal_ops",
            "scope": "cumulative_noncritical_top_widgets",
            "maxLines": max_lines,
            "maxTotalLines": STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES,
            "sourceLineCount": source_line_count,
            "renderedLineCount": rendered_line_count,
            "omittedLineCount": omitted_line_count,
            "exceeded": omitted_line_count > 0,
        });
    }

    json!({
        "bounded": false,
        "policy": "unbounded",
        "sourceLineCount": source_line_count,
        "renderedLineCount": rendered_line_count,
        "omittedLineCount": 0,
        "exceeded": false,
    })
}

fn terminal_draw_scrollback_physical_line_budget(operation: &Value) -> usize {
    terminal_draw_usize_budget_with_hard_cap(
        value_path(operation, &["physicalLineBudget", "maxPhysicalLines"]),
        STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES,
    )
}

fn terminal_draw_scrollback_physical_line_budget_value(source_line_count: usize) -> Value {
    json!({
        "bounded": true,
        "policy": "cap_scrollback_physical_lines_before_terminal_writes",
        "scope": "append_scrollback_block",
        "viewportDependent": true,
        "maxPhysicalLines": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES,
        "sourceLineCount": source_line_count,
    })
}

fn terminal_draw_scrollback_clear_visible_rows_budget(operation: &Value) -> usize {
    terminal_draw_usize_budget_with_hard_cap(
        value_path(operation, &["clearVisibleRowsBudget", "maxRows"]),
        STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS,
    )
}

fn terminal_draw_scrollback_clear_visible_rows_budget_value() -> Value {
    json!({
        "bounded": true,
        "policy": "cap_scrollback_clear_visible_rows_before_terminal_writes",
        "scope": "append_scrollback_block",
        "maxRows": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS,
    })
}

fn terminal_draw_terminal_op_budget(draw_plan: &Value) -> usize {
    terminal_draw_usize_budget_with_hard_cap(
        value_path(draw_plan, &["safety", "terminalOpBudget", "maxOps"])
            .or_else(|| value_path(draw_plan, &["draw", "terminalOpBudgetMaxOps"])),
        STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS,
    )
}

fn terminal_draw_terminal_op_budget_value() -> Value {
    json!({
        "bounded": true,
        "policy": "cap_terminal_ops_before_execution",
        "scope": "draw_plan",
        "maxOps": STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS,
        "counts": "terminal_ops_before_executor_match",
    })
}

fn terminal_draw_text_byte_budget(draw_plan: &Value) -> usize {
    terminal_draw_usize_budget_with_hard_cap(
        value_path(draw_plan, &["safety", "terminalTextByteBudget", "maxBytes"])
            .or_else(|| value_path(draw_plan, &["draw", "terminalTextByteBudgetMaxBytes"])),
        STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES,
    )
}

fn terminal_draw_text_byte_budget_value() -> Value {
    json!({
        "bounded": true,
        "policy": "cap_terminal_text_bytes_before_terminal_writes",
        "scope": "draw_plan",
        "maxBytes": STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES,
        "counts": "sanitized_text_bytes_before_print",
        "graphemeSafeTruncation": true,
    })
}

fn terminal_draw_usize_budget_with_hard_cap(value: Option<&Value>, hard_cap: usize) -> usize {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(hard_cap)
        .min(hard_cap)
}

fn terminal_draw_executor_budget_hard_caps_value() -> Value {
    json!({
        "bounded": true,
        "policy": "min_declared_budget_with_renderer_hard_cap",
        "scope": "draw_executor",
        "terminalOpMaxOps": STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS,
        "terminalTextMaxBytes": STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES,
        "scrollbackPhysicalMaxLines": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES,
        "scrollbackClearVisibleMaxRows": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS,
    })
}

fn terminal_draw_executor_zero_copy_budgeting_value() -> Value {
    json!({
        "bounded": true,
        "policy": "borrow_terminal_ops_and_scrollback_lines_before_budget",
        "scope": "draw_executor",
        "terminalOps": "borrowed_array_iterated_with_budget_take",
        "scrollbackLines": "borrowed_array_iterated_with_physical_line_budget",
        "prevents": [
            "pre_budget_terminal_ops_clone",
            "pre_budget_scrollback_lines_clone"
        ],
    })
}

fn terminal_draw_plan_borrowed_patch_inputs_value() -> Value {
    json!({
        "bounded": true,
        "policy": "borrow_patch_operations_and_lines_until_draw_json_emit",
        "scope": "draw_plan_scheduler",
        "patchOperations": "borrowed_array_iterated_without_preclone",
        "patchLines": "borrowed_array_iterated_without_preclone",
        "prevents": [
            "pre_draw_plan_patch_operations_clone",
            "pre_draw_plan_region_lines_clone"
        ],
    })
}

fn terminal_draw_runtime_owned_pending_submit_value() -> Value {
    json!({
        "bounded": true,
        "policy": "move_owned_draw_plan_into_pending_queue",
        "scope": "draw_runtime",
        "ownedSubmitApi": "submit_draw_plan_value_at",
        "borrowedSubmitApi": "submit_draw_plan_at",
        "prevents": [
            "frontend_draw_plan_clone_on_throttle_queue",
            "frontend_draw_plan_clone_on_manual_scroll_hold"
        ],
    })
}

fn terminal_draw_scrollback_soft_wrap_materialization_budget_value() -> Value {
    json!({
        "bounded": true,
        "policy": "cap_soft_wrap_materialization_before_scrollback_allocation",
        "scope": "append_scrollback_block",
        "budgetSource": "remaining_scrollback_physical_lines",
        "prevents": [
            "pre_budget_long_line_soft_wrap_vec",
            "single_line_wrap_memory_amplification"
        ],
    })
}

fn terminal_draw_scrollback_soft_wrap_streaming_sanitizer_value() -> Value {
    json!({
        "bounded": true,
        "policy": "strip_terminal_controls_while_budgeting_soft_wrap",
        "scope": "append_scrollback_block",
        "sanitizeBeforeAllocation": false,
        "controlSequenceHandling": "streaming_escape_sequence_consumer",
        "inlineControlHandling": "streaming_carriage_return_and_backspace_normalization",
        "prevents": [
            "pre_budget_sanitized_line_clone",
            "pre_budget_inline_control_grapheme_vec"
        ],
    })
}

fn terminal_draw_budget_text_for_write(
    text: &str,
    budget: &mut StreamJsonTerminalTextByteBudget,
    report: &mut StreamJsonTerminalDrawExecutionReport,
) -> Option<String> {
    let result = budget.cap_text(text);
    report.terminal_text_byte_budget_written_bytes = budget.written_bytes();
    if result.omitted {
        report.terminal_text_byte_budget_exceeded = true;
        report.terminal_text_byte_budget_omitted_write_count = report
            .terminal_text_byte_budget_omitted_write_count
            .saturating_add(1);
        report.bounded_line_count = report.bounded_line_count.saturating_add(1);
        return None;
    }

    if result.truncated {
        report.terminal_text_byte_budget_exceeded = true;
        report.terminal_text_byte_budget_truncated_write_count = report
            .terminal_text_byte_budget_truncated_write_count
            .saturating_add(1);
        report.bounded_line_count = report.bounded_line_count.saturating_add(1);
    }

    Some(result.text)
}

fn patch_operation_appends_scrollback(operation: &Value) -> bool {
    string_field(operation, "updateMode") == "append_scrollback"
        || string_field(operation, "anchor") == "scrollback"
        || string_field(operation, "role") == "transcript"
}

fn render_patch_scroll_contract(frame: &Value, operations: &[Value]) -> Value {
    let mut contract = json!({
        "stable": bool_path(frame, &["scroll", "stable"]).unwrap_or(true),
        "preserveOnActiveUpdate": bool_path(frame, &["scroll", "preserveOnActiveUpdate"]).unwrap_or(true),
        "historyPolicy": string_path(frame, &["scroll", "historyPolicy"]),
    });

    let scrollback_commit = operations.iter().any(patch_operation_appends_scrollback);
    let lifecycle_clear = operations
        .iter()
        .any(|operation| string_field(operation, "updateMode") == "clear_retired");
    let critical_update = operations
        .iter()
        .any(render_patch_operation_requires_manual_scroll_bypass);

    if critical_update {
        let (manual_scroll_policy, history_policy) = if lifecycle_clear {
            ("bypass_for_lifecycle_clear", "clear_retired_region")
        } else {
            (
                "bypass_for_critical_region_update",
                "critical_region_update",
            )
        };
        if let Value::Object(map) = &mut contract {
            map.insert("preserveDuringManualScroll".to_string(), Value::Bool(false));
            map.insert(
                "manualScrollPolicy".to_string(),
                Value::String(manual_scroll_policy.to_string()),
            );
            map.insert("manualScrollBypass".to_string(), Value::Bool(true));
            map.insert(
                "historyPolicy".to_string(),
                Value::String(history_policy.to_string()),
            );
            map.insert(
                "manualScrollPendingPolicy".to_string(),
                Value::String("bypass_pending_hold".to_string()),
            );
            map.insert(
                "commitToScrollback".to_string(),
                Value::Bool(scrollback_commit),
            );
        }
        return contract;
    }

    if scrollback_commit {
        if let Value::Object(map) = &mut contract {
            map.insert("preserveDuringManualScroll".to_string(), Value::Bool(true));
            map.insert(
                "manualScrollPolicy".to_string(),
                Value::String("hold_noncritical_scrollback_commit".to_string()),
            );
            map.insert("manualScrollBypass".to_string(), Value::Bool(false));
            map.insert(
                "historyPolicy".to_string(),
                Value::String("commit_scrollback".to_string()),
            );
            map.insert(
                "manualScrollPendingPolicy".to_string(),
                Value::String("replace_pending_with_latest".to_string()),
            );
            map.insert("commitToScrollback".to_string(), Value::Bool(true));
        }
        return contract;
    }

    if operations
        .iter()
        .any(render_patch_operation_is_noncritical_completion_update)
    {
        if let Value::Object(map) = &mut contract {
            map.insert("preserveDuringManualScroll".to_string(), Value::Bool(true));
            map.insert(
                "manualScrollPolicy".to_string(),
                Value::String("hold_noncritical_completion_update".to_string()),
            );
            map.insert("manualScrollBypass".to_string(), Value::Bool(false));
            map.insert(
                "historyPolicy".to_string(),
                Value::String("update_completion_region".to_string()),
            );
            map.insert(
                "manualScrollPendingPolicy".to_string(),
                Value::String("replace_pending_with_latest".to_string()),
            );
            map.insert("commitToScrollback".to_string(), Value::Bool(false));
        }
        return contract;
    }

    if operations
        .iter()
        .any(render_patch_operation_is_noncritical_widget_update)
    {
        if let Value::Object(map) = &mut contract {
            map.insert("preserveDuringManualScroll".to_string(), Value::Bool(true));
            map.insert(
                "manualScrollPolicy".to_string(),
                Value::String("hold_noncritical_widget_region_update".to_string()),
            );
            map.insert("manualScrollBypass".to_string(), Value::Bool(false));
            map.insert(
                "historyPolicy".to_string(),
                Value::String("update_widget_region".to_string()),
            );
            map.insert(
                "manualScrollPendingPolicy".to_string(),
                Value::String("replace_pending_with_latest".to_string()),
            );
            map.insert("commitToScrollback".to_string(), Value::Bool(false));
        }
    }

    contract
}

fn render_patch_operation_requires_manual_scroll_bypass(operation: &Value) -> bool {
    string_field(operation, "updateMode") == "clear_retired"
        || string_field(operation, "updateMode") == "replace_blocking"
        || matches!(
            string_field(operation, "role").as_str(),
            "approval" | "error"
        )
}

fn render_patch_operation_is_noncritical_completion_update(operation: &Value) -> bool {
    string_field(operation, "role") == "final_summary"
}

fn render_patch_operation_is_noncritical_widget_update(operation: &Value) -> bool {
    string_field(operation, "anchor") == "top"
        && matches!(
            string_field(operation, "role").as_str(),
            "slash_result" | "plan" | "command" | "background_tasks" | "file_changes" | "diff"
        )
}

fn render_region_plan_is_blocking(plan: &Value) -> bool {
    string_field(plan, "updateMode") == "replace_blocking"
        || string_field(plan, "role") == "approval"
}

fn region_start_row_expression(
    region_id: &str,
    role: &str,
    anchor: &str,
    line_count: usize,
    top_start_row: Option<usize>,
) -> String {
    region_row_expression(region_id, role, anchor, line_count.max(1), 0, top_start_row)
}

fn region_row_expression(
    region_id: &str,
    role: &str,
    anchor: &str,
    line_count: usize,
    line_index: usize,
    top_start_row: Option<usize>,
) -> String {
    if anchor == "top" {
        let start_row = top_start_row.unwrap_or_else(|| top_region_row_base_offset(role));
        return format!("top+{}", start_row + line_index);
    }

    if region_id == "footer" || role == "footer" {
        let offset = line_count.saturating_sub(1).saturating_sub(line_index);
        return format!("bottom-{offset}");
    }

    let offset = line_count.saturating_sub(line_index);
    format!("bottom-{offset}")
}

fn stale_region_row_expression(
    region_id: &str,
    role: &str,
    anchor: &str,
    current_line_count: usize,
    stale_index: usize,
    top_start_row: Option<usize>,
) -> String {
    if anchor == "top" {
        let start_row = top_start_row.unwrap_or_else(|| top_region_row_base_offset(role));
        return format!("top+{}", start_row + current_line_count + stale_index);
    }

    if region_id == "footer" || role == "footer" {
        return format!("bottom-{}", current_line_count + stale_index);
    }

    format!("bottom-{}", current_line_count + stale_index + 1)
}

fn terminal_region_layout_mode(anchor: &str, top_start_row: Option<usize>) -> &'static str {
    if anchor == "top" && top_start_row.is_some() {
        "dynamic_top_stack"
    } else {
        "anchored_fallback"
    }
}

fn terminal_semantic_style_for_line(role: &str, update_mode: &str, text: &str) -> &'static str {
    if update_mode == "clear_retired" {
        return "muted";
    }

    let normalized = text.trim().to_ascii_lowercase();
    match role {
        "status" => {
            if normalized.contains("failed") || normalized.contains("cancelled") {
                "error"
            } else if normalized.contains("waiting approval") || normalized.contains("retrying") {
                "warning"
            } else if normalized.contains("done") {
                "success"
            } else {
                "accent"
            }
        }
        "approval" => "warning",
        "error" => "error",
        "footer" => "muted",
        "final_summary" => {
            if normalized.contains("failed") || normalized.contains("risk:") {
                "error"
            } else if normalized.contains("success") {
                "success"
            } else if normalized.starts_with("reason:") || normalized.starts_with("changes:") {
                "muted"
            } else {
                "accent"
            }
        }
        "command" => {
            if normalized.starts_with("exit: 0") {
                "success"
            } else if normalized.starts_with("exit:") {
                "error"
            } else if normalized.starts_with("cwd:")
                || normalized.starts_with("duration:")
                || normalized.starts_with("full log:")
            {
                "muted"
            } else {
                "info"
            }
        }
        "diff" => {
            if normalized.starts_with('+') {
                "success"
            } else if normalized.starts_with('-') {
                "error"
            } else if normalized.contains("collapsed") {
                "muted"
            } else {
                "warning"
            }
        }
        "file_changes" => {
            if normalized.starts_with("d ") {
                "error"
            } else if normalized.starts_with("a ") || normalized.contains(" +") {
                "success"
            } else if normalized.starts_with("files:") {
                "muted"
            } else {
                "warning"
            }
        }
        "plan" => {
            if normalized.contains("blocked") && !normalized.contains("blocked 0") {
                "warning"
            } else if normalized.starts_with("active:") {
                "info"
            } else if normalized.starts_with("progress:") {
                "muted"
            } else {
                "accent"
            }
        }
        "activity" => "info",
        "transcript"
            if (normalized == "assistant transcript" || normalized.starts_with("... ")) =>
        {
            "muted"
        }
        _ => "plain",
    }
}

fn terminal_semantic_color(semantic_style: &str) -> Option<style::Color> {
    match semantic_style {
        "accent" => Some(style::Color::Cyan),
        "info" => Some(style::Color::Blue),
        "success" => Some(style::Color::Green),
        "warning" => Some(style::Color::Yellow),
        "error" => Some(style::Color::Red),
        "muted" => Some(style::Color::DarkGrey),
        _ => None,
    }
}

fn terminal_render_semantic_colors_enabled() -> bool {
    terminal_render_semantic_colors_enabled_for_env(
        std::env::var(STREAM_JSON_TERMINAL_RENDER_COLOR_ENV)
            .ok()
            .as_deref(),
        std::env::var("NO_COLOR").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var("CLICOLOR").ok().as_deref(),
    )
}

fn terminal_render_semantic_colors_enabled_for_env(
    color_override: Option<&str>,
    no_color: Option<&str>,
    term: Option<&str>,
    clicolor: Option<&str>,
) -> bool {
    if let Some(value) = color_override {
        match value.trim().to_ascii_lowercase().as_str() {
            "always" | "on" | "true" | "1" | "yes" => return true,
            "never" | "off" | "false" | "0" | "no" => return false,
            _ => {}
        }
    }

    if no_color.is_some_and(|value| !value.is_empty()) {
        return false;
    }
    if term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
        return false;
    }
    if clicolor.is_some_and(|value| value.trim() == "0") {
        return false;
    }

    true
}

fn terminal_render_unicode_enabled() -> bool {
    terminal_render_unicode_enabled_for_env(
        std::env::var(STREAM_JSON_TERMINAL_RENDER_UNICODE_ENV)
            .ok()
            .as_deref(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LC_CTYPE").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

fn terminal_render_unicode_enabled_for_env(
    unicode_override: Option<&str>,
    term: Option<&str>,
    lc_all: Option<&str>,
    lc_ctype: Option<&str>,
    lang: Option<&str>,
) -> bool {
    if let Some(value) = unicode_override {
        match value.trim().to_ascii_lowercase().as_str() {
            "always" | "on" | "true" | "1" | "yes" => return true,
            "never" | "off" | "false" | "0" | "no" | "ascii" => return false,
            _ => {}
        }
    }

    if term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
        return false;
    }

    for locale in [lc_all, lc_ctype, lang].into_iter().flatten() {
        let normalized = locale.trim().to_ascii_uppercase();
        if normalized.contains("UTF-8") || normalized.contains("UTF8") {
            return true;
        }
        if normalized == "C"
            || normalized == "POSIX"
            || normalized.contains("US-ASCII")
            || normalized.contains("ASCII")
        {
            return false;
        }
    }

    true
}

fn top_region_row_base_offset(role: &str) -> usize {
    match role {
        "status" => 0,
        "plan" => 1,
        "file_changes" => 1,
        "command" => 1,
        "diff" => 7,
        "error" => 11,
        "final_summary" => 16,
        _ => 0,
    }
}

fn draw_plan_sequence(draw_plan: &Value) -> Option<u64> {
    draw_plan.get("sequence").and_then(Value::as_u64)
}

fn terminal_patch_sequence_value(patch: &Value) -> Value {
    patch
        .get("sequence")
        .or_else(|| patch.get("sourceEventSequence"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn draw_plan_throttle_ms(draw_plan: &Value) -> u64 {
    value_path(draw_plan, &["schedule", "throttleMs"])
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn draw_plan_skip_reason(draw_plan: &Value) -> Option<String> {
    value_path(draw_plan, &["draw", "skipReason"])
        .and_then(Value::as_str)
        .filter(|reason| !reason.is_empty())
        .map(str::to_string)
}

fn draw_plan_preserves_manual_scroll(draw_plan: &Value) -> bool {
    if !bool_path(draw_plan, &["scroll", "preserveOnActiveUpdate"]).unwrap_or(true) {
        return false;
    }
    if draw_plan_requires_manual_scroll_bypass(draw_plan) {
        return false;
    }

    let history_policy = string_path(draw_plan, &["scroll", "historyPolicy"]);
    bool_path(draw_plan, &["scroll", "preserveDuringManualScroll"]).unwrap_or(false)
        || matches!(
            history_policy.as_str(),
            "update_active" | "update_top_region" | "update_widget_region"
        )
}

fn draw_plan_requires_manual_scroll_bypass(draw_plan: &Value) -> bool {
    if bool_path(draw_plan, &["scroll", "manualScrollBypass"]).unwrap_or(false) {
        return true;
    }

    if bool_path(draw_plan, &["draw", "hasBlockingRegion"]).unwrap_or(false) {
        return true;
    }

    draw_plan
        .get("regionPlans")
        .and_then(Value::as_array)
        .map(|plans| {
            plans.iter().any(|plan| {
                matches!(string_field(plan, "role").as_str(), "approval" | "error")
                    || string_field(plan, "updateMode") == "clear_retired"
            })
        })
        .unwrap_or(false)
}

fn terminal_draw_reserved_bottom_rows(draw_plan: &Value, viewport_rows: u16) -> u16 {
    let mut footer_rows = 0usize;
    let mut bottom_content_rows = 0usize;
    let Some(region_plans) = draw_plan.get("regionPlans").and_then(Value::as_array) else {
        return 1.min(viewport_rows);
    };

    for plan in region_plans {
        if string_field(plan, "anchor") != "bottom" {
            continue;
        }
        let line_count = plan.get("lineCount").and_then(Value::as_u64).unwrap_or(0) as usize;
        if line_count == 0 {
            continue;
        }
        if string_field(plan, "role") == "footer" || string_field(plan, "regionId") == "footer" {
            footer_rows = footer_rows.max(line_count);
        } else {
            bottom_content_rows = bottom_content_rows.max(line_count);
        }
    }

    let reserved = footer_rows
        .saturating_add(bottom_content_rows)
        .max(usize::from(footer_rows == 0));
    let max_reservable = viewport_rows.saturating_sub(1) as usize;
    reserved.min(max_reservable) as u16
}

fn resolve_terminal_draw_row(
    row_expression: &str,
    viewport: StreamJsonTerminalViewport,
    reserved_bottom_rows: u16,
) -> Option<u16> {
    let expression = row_expression.trim();
    if let Some(rest) = expression.strip_prefix("top+") {
        let offset = rest.parse::<u16>().ok()?;
        let top_limit = viewport.rows.saturating_sub(reserved_bottom_rows);
        return (offset < top_limit).then_some(offset);
    }

    if let Some(rest) = expression.strip_prefix("bottom-") {
        let offset = rest.parse::<u16>().ok()?;
        return (offset < viewport.rows).then_some(viewport.rows - 1 - offset);
    }

    if let Some(rest) = expression.strip_prefix("absolute:") {
        let row = rest.parse::<u16>().ok()?;
        return (row < viewport.rows).then_some(row);
    }

    let row = expression.parse::<u16>().ok()?;
    (row < viewport.rows).then_some(row)
}

fn render_patch_region_ids(frame: &Value, frame_hash_unchanged: bool) -> Vec<String> {
    if frame_hash_unchanged {
        return Vec::new();
    }

    let changed = value_path(frame, &["draw", "changedRegionIds"])
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut region_ids = if !changed.is_empty() {
        changed
    } else {
        value_path(frame, &["regions"])
            .and_then(Value::as_array)
            .map(|regions| {
                regions
                    .iter()
                    .filter_map(|region| region.get("id").and_then(Value::as_str))
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    merge_render_patch_region_ids(&mut region_ids, render_patch_retired_region_ids(frame));
    region_ids
}

fn render_patch_suppress_forced_scrollback_reappend(
    frame: &Value,
    region_ids: &mut Vec<String>,
) -> bool {
    let Some(regions) = frame.get("regions").and_then(Value::as_array) else {
        return false;
    };
    let before = region_ids.len();
    region_ids.retain(|region_id| {
        !regions
            .iter()
            .find(|region| region.get("id").and_then(Value::as_str) == Some(region_id.as_str()))
            .is_some_and(patch_operation_appends_scrollback)
    });
    region_ids.len() != before
}

fn render_patch_retired_region_ids(frame: &Value) -> Vec<String> {
    value_path(frame, &["changes", "retiredRegions"])
        .and_then(Value::as_array)
        .map(|regions| {
            regions
                .iter()
                .filter_map(|region| region.get("id").and_then(Value::as_str))
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn merge_render_patch_region_ids(region_ids: &mut Vec<String>, additional_region_ids: Vec<String>) {
    for region_id in additional_region_ids {
        if !region_ids.iter().any(|existing| existing == &region_id) {
            region_ids.push(region_id);
        }
    }
}

fn render_patch_top_layout_changed_region_ids(
    current: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
    previous: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
) -> Vec<String> {
    current
        .iter()
        .filter(|(region_id, layout)| previous.get(region_id.as_str()) != Some(layout))
        .map(|(region_id, _)| region_id.clone())
        .collect()
}

fn render_frame_top_region_layouts(
    frame: &Value,
) -> BTreeMap<String, StreamJsonTerminalRegionLayout> {
    let mut layouts = BTreeMap::new();
    let mut next_start_row = 0usize;

    if let Some(regions) = frame.get("regions").and_then(Value::as_array) {
        for region in regions {
            if string_field(region, "anchor") != "top" {
                continue;
            }
            let Some(region_id) = region.get("id").and_then(Value::as_str) else {
                continue;
            };
            let line_count = render_patch_region_budgeted_line_count(region);
            layouts.insert(
                region_id.to_string(),
                StreamJsonTerminalRegionLayout {
                    start_row: next_start_row,
                    line_count,
                },
            );
            next_start_row = next_start_row.saturating_add(line_count);
        }
    }

    layouts
}

fn render_patch_operations(
    frame: &Value,
    region_ids: &[String],
    previous_top_region_layouts: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
    current_top_region_layouts: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
) -> Vec<Value> {
    let Some(regions) = frame.get("regions").and_then(Value::as_array) else {
        return Vec::new();
    };
    let retired_regions = value_path(frame, &["changes", "retiredRegions"])
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    region_ids
        .iter()
        .filter_map(|region_id| {
            if let Some(region) = regions
                .iter()
                .find(|region| region.get("id").and_then(Value::as_str) == Some(region_id))
            {
                return Some(render_patch_operation(
                    region,
                    current_top_region_layouts.get(region_id),
                    previous_top_region_layouts.get(region_id),
                    current_top_region_layouts,
                ));
            }

            retired_regions
                .iter()
                .find(|region| region.get("id").and_then(Value::as_str) == Some(region_id))
                .map(|region| {
                    render_patch_retired_region_operation(
                        region,
                        previous_top_region_layouts.get(region_id),
                        current_top_region_layouts,
                    )
                })
        })
        .collect()
}

fn render_patch_operation(
    region: &Value,
    top_layout: Option<&StreamJsonTerminalRegionLayout>,
    previous_top_layout: Option<&StreamJsonTerminalRegionLayout>,
    current_top_region_layouts: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
) -> Value {
    let line_set = render_patch_lines(region);
    let mut operation = json!({
        "op": "replace_region",
        "regionId": string_field(region, "id"),
        "role": string_field(region, "role"),
        "anchor": string_field(region, "anchor"),
        "placement": string_field(region, "placement"),
        "updateMode": string_field(region, "updateMode"),
        "regionHash": string_field(region, "regionHash"),
        "lineCount": line_set.lines.len(),
        "maxLineWidthCells": line_set.max_width_cells,
        "truncated": line_set.any_truncated || line_set.omitted_line_count > 0,
        "controlCharsStripped": line_set.any_stripped,
        "lineBudget": {
            "bounded": true,
            "policy": "cap_region_lines_before_terminal_ops",
            "maxLines": line_set.max_line_count,
            "sourceLineCount": line_set.source_line_count,
            "renderedLineCount": line_set.lines.len(),
            "omittedLineCount": line_set.omitted_line_count,
            "exceeded": line_set.omitted_line_count > 0,
        },
        "lines": line_set.lines,
    });
    if let Some(line_variants) = render_patch_line_variants(region) {
        if let Value::Object(map) = &mut operation {
            map.insert("lineVariants".to_string(), line_variants);
            map.insert(
                "lineVariantPolicy".to_string(),
                Value::String("choose_shortest_fitting_variant".to_string()),
            );
            map.insert("viewportSelectableLines".to_string(), Value::Bool(true));
        }
    }
    render_patch_attach_top_layout(
        &mut operation,
        region,
        top_layout,
        previous_top_layout,
        current_top_region_layouts,
    );
    operation
}

fn render_patch_retired_region_operation(
    region: &Value,
    previous_top_layout: Option<&StreamJsonTerminalRegionLayout>,
    current_top_region_layouts: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
) -> Value {
    let source_previous_line_count =
        render_patch_retired_region_source_line_count(region, previous_top_layout);
    let max_line_count = render_patch_region_line_budget(region);
    let rendered_previous_line_count = source_previous_line_count.min(max_line_count);
    let omitted_line_count =
        source_previous_line_count.saturating_sub(rendered_previous_line_count);
    let mut operation = json!({
        "op": "clear_region",
        "regionId": string_field(region, "id"),
        "role": string_field(region, "role"),
        "anchor": string_field(region, "anchor"),
        "placement": string_field(region, "placement"),
        "updateMode": "clear_retired",
        "regionHash": string_field(region, "regionHash"),
        "lineCount": 0,
        "previousLineCount": rendered_previous_line_count,
        "maxLineWidthCells": 0,
        "truncated": omitted_line_count > 0,
        "controlCharsStripped": false,
        "lineBudget": {
            "bounded": true,
            "policy": "cap_retired_region_clear_lines_before_terminal_ops",
            "maxLines": max_line_count,
            "sourceLineCount": source_previous_line_count,
            "renderedLineCount": rendered_previous_line_count,
            "omittedLineCount": omitted_line_count,
            "exceeded": omitted_line_count > 0,
        },
        "lines": [],
    });
    render_patch_attach_top_layout(
        &mut operation,
        region,
        previous_top_layout,
        previous_top_layout,
        current_top_region_layouts,
    );
    operation
}

fn render_patch_retired_region_source_line_count(
    region: &Value,
    previous_top_layout: Option<&StreamJsonTerminalRegionLayout>,
) -> usize {
    region
        .get("previousLineCount")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .or_else(|| previous_top_layout.map(|layout| layout.line_count))
        .unwrap_or(0)
}

fn render_patch_attach_top_layout(
    operation: &mut Value,
    region: &Value,
    top_layout: Option<&StreamJsonTerminalRegionLayout>,
    previous_top_layout: Option<&StreamJsonTerminalRegionLayout>,
    current_top_region_layouts: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
) {
    if string_field(region, "anchor") != "top" {
        return;
    }
    let start_row = top_layout
        .map(|layout| layout.start_row)
        .unwrap_or_else(|| top_region_row_base_offset(&string_field(region, "role")));
    let line_count = top_layout
        .map(|layout| layout.line_count)
        .unwrap_or_else(|| {
            region
                .get("lines")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        });
    if let Value::Object(map) = operation {
        map.insert("topStartRow".to_string(), json!(start_row));
        map.insert("topLineCount".to_string(), json!(line_count));
        if let Some(previous_layout) = previous_top_layout {
            map.insert(
                "previousTopStartRow".to_string(),
                json!(previous_layout.start_row),
            );
            map.insert(
                "previousTopLineCount".to_string(),
                json!(previous_layout.line_count),
            );
            if top_layout.map(|layout| layout.start_row) != Some(previous_layout.start_row) {
                let rows_to_clear = render_patch_previous_top_rows_to_clear(
                    previous_layout,
                    current_top_region_layouts,
                );
                if !rows_to_clear.is_empty() {
                    map.insert("previousTopRowsToClear".to_string(), json!(rows_to_clear));
                }
            }
        }
        map.insert(
            "layoutMode".to_string(),
            Value::String("dynamic_top_stack".to_string()),
        );
    }
}

fn render_patch_previous_top_rows_to_clear(
    previous_layout: &StreamJsonTerminalRegionLayout,
    current_top_region_layouts: &BTreeMap<String, StreamJsonTerminalRegionLayout>,
) -> Vec<usize> {
    let current_rows = current_top_region_layouts
        .values()
        .flat_map(|layout| layout.start_row..layout.start_row.saturating_add(layout.line_count))
        .collect::<Vec<_>>();

    (previous_layout.start_row
        ..previous_layout
            .start_row
            .saturating_add(previous_layout.line_count))
        .filter(|row| !current_rows.iter().any(|current_row| current_row == row))
        .collect()
}

fn render_patch_region_source_line_count(region: &Value) -> usize {
    region
        .get("lines")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn render_patch_region_line_budget(_region: &Value) -> usize {
    STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
}

fn render_patch_region_budgeted_line_count(region: &Value) -> usize {
    render_patch_region_source_line_count(region).min(render_patch_region_line_budget(region))
}

fn render_patch_lines(region: &Value) -> RenderPatchLineSet {
    let mut max_width_cells = 0;
    let mut any_truncated = false;
    let mut any_stripped = false;
    let source_line_count = render_patch_region_source_line_count(region);
    let max_line_count = render_patch_region_line_budget(region);
    let lines = region
        .get("lines")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .take(max_line_count)
                .filter_map(Value::as_str)
                .map(|line| {
                    let (safe, truncated, stripped) =
                        terminal_patch_safe_line(line, STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS);
                    max_width_cells = max_width_cells.max(UnicodeWidthStr::width(safe.as_str()));
                    any_truncated |= truncated;
                    any_stripped |= stripped;
                    safe
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let omitted_line_count = source_line_count.saturating_sub(lines.len());
    RenderPatchLineSet {
        lines,
        max_width_cells,
        any_truncated,
        any_stripped,
        source_line_count,
        max_line_count,
        omitted_line_count,
    }
}

fn render_draw_previous_region_line_count(
    operation: &Value,
    previous_region_line_counts: &BTreeMap<String, usize>,
    region_id: &str,
) -> usize {
    let previous_line_count = previous_region_line_counts
        .get(region_id)
        .copied()
        .or_else(|| {
            operation
                .get("previousLineCount")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .unwrap_or(0);

    render_draw_line_budget_max_lines(operation)
        .map(|max_line_count| previous_line_count.min(max_line_count))
        .unwrap_or(previous_line_count)
}

fn render_draw_line_budget_max_lines(operation: &Value) -> Option<usize> {
    bool_path(operation, &["lineBudget", "bounded"])
        .unwrap_or(false)
        .then(|| {
            value_path(operation, &["lineBudget", "maxLines"])
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES)
        })
}

fn render_patch_line_variants(region: &Value) -> Option<Value> {
    let variants = region.get("lineVariants").and_then(Value::as_object)?;
    let mut out = serde_json::Map::new();
    for key in ["full", "compact", "minimal"] {
        if let Some(line) = variants.get(key).and_then(Value::as_str) {
            let (safe, width_cells, truncated, stripped) =
                stream_json_terminal_patch_safe_line(line, STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS);
            out.insert(
                key.to_string(),
                json!({
                    "text": safe,
                    "widthCells": width_cells,
                    "truncated": truncated,
                    "controlCharsStripped": stripped,
                }),
            );
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

fn terminal_patch_safe_line(line: &str, max_cells: usize) -> (String, bool, bool) {
    let (line, control_sequence_stripped_count, inline_control_normalized_count) =
        terminal_sanitize_terminal_control_text(line);
    let mut out = String::new();
    let mut cells = 0usize;
    let mut truncated = false;
    let mut stripped = control_sequence_stripped_count > 0 || inline_control_normalized_count > 0;
    let text_budget = max_cells.saturating_sub(3);

    for grapheme in UnicodeSegmentation::graphemes(line.as_str(), true) {
        let (safe, safe_stripped, _, _, _) = terminal_safe_grapheme(grapheme, true);
        stripped |= safe_stripped;
        if safe.is_empty() {
            continue;
        }
        let width = UnicodeWidthStr::width(safe.as_str());
        if cells.saturating_add(width) > text_budget {
            truncated = true;
            out.push_str("...");
            break;
        }
        cells = cells.saturating_add(width);
        out.push_str(&safe);
    }

    (out, truncated, stripped)
}

pub(crate) fn stream_json_terminal_patch_safe_line(
    line: &str,
    max_cells: usize,
) -> (String, usize, bool, bool) {
    let (safe, truncated, stripped) = terminal_patch_safe_line(line, max_cells);
    let width_cells = UnicodeWidthStr::width(safe.as_str());
    (safe, width_cells, truncated, stripped)
}

fn terminal_draw_bounded_line(line: &str, max_cells: usize) -> (String, bool, bool) {
    let (line, truncated, stripped, _, _, _, _, _) =
        terminal_draw_bounded_line_with_unicode(line, max_cells, true);
    (line, truncated, stripped)
}

fn terminal_draw_bounded_line_with_unicode(
    line: &str,
    max_cells: usize,
    unicode_enabled: bool,
) -> (String, bool, bool, usize, usize, usize, usize, usize) {
    if max_cells == 0 {
        let (_, control_sequence_stripped_count, inline_control_normalized_count) =
            terminal_sanitize_terminal_control_text(line);
        return (
            String::new(),
            !line.is_empty(),
            control_sequence_stripped_count > 0 || inline_control_normalized_count > 0,
            0,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            0,
            0,
        );
    }

    let (line, control_sequence_stripped_count, inline_control_normalized_count) =
        terminal_sanitize_terminal_control_text(line);
    let mut out = String::new();
    let mut cells = 0usize;
    let mut truncated = false;
    let mut stripped = control_sequence_stripped_count > 0 || inline_control_normalized_count > 0;
    let mut ascii_fallback_count = 0usize;
    let mut control_char_normalized_count = 0usize;
    let mut format_control_stripped_count = 0usize;
    let ellipsis = if max_cells >= 3 {
        "...".to_string()
    } else {
        ".".repeat(max_cells)
    };
    let ellipsis_cells = UnicodeWidthStr::width(ellipsis.as_str());
    let text_budget = max_cells.saturating_sub(ellipsis_cells);

    for grapheme in UnicodeSegmentation::graphemes(line.as_str(), true) {
        let (safe, safe_stripped, fallback_count, control_char_count, format_control_count) =
            terminal_safe_grapheme(grapheme, unicode_enabled);
        stripped |= safe_stripped;
        if safe.is_empty() {
            control_char_normalized_count =
                control_char_normalized_count.saturating_add(control_char_count);
            format_control_stripped_count =
                format_control_stripped_count.saturating_add(format_control_count);
            continue;
        }
        let width = UnicodeWidthStr::width(safe.as_str());
        if cells.saturating_add(width) > text_budget {
            truncated = true;
            out.push_str(&ellipsis);
            break;
        }
        ascii_fallback_count = ascii_fallback_count.saturating_add(fallback_count);
        control_char_normalized_count =
            control_char_normalized_count.saturating_add(control_char_count);
        format_control_stripped_count =
            format_control_stripped_count.saturating_add(format_control_count);
        cells = cells.saturating_add(width);
        out.push_str(&safe);
    }

    (
        out,
        truncated,
        stripped,
        ascii_fallback_count,
        control_sequence_stripped_count,
        inline_control_normalized_count,
        control_char_normalized_count,
        format_control_stripped_count,
    )
}

fn terminal_truncate_text_to_byte_budget(text: &str, max_bytes: usize) -> String {
    if max_bytes == 0 {
        return String::new();
    }

    let mut out = String::new();
    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        if out.len().saturating_add(grapheme.len()) > max_bytes {
            break;
        }
        out.push_str(grapheme);
    }
    out
}

fn terminal_draw_soft_wrapped_lines(line: &str, max_cells: usize) -> (Vec<String>, usize, bool) {
    let (lines, wrapped_count, stripped, _, _, _, _, _) =
        terminal_draw_soft_wrapped_lines_with_unicode(line, max_cells, true);
    (lines, wrapped_count, stripped)
}

fn terminal_draw_soft_wrapped_lines_with_unicode(
    line: &str,
    max_cells: usize,
    unicode_enabled: bool,
) -> (Vec<String>, usize, bool, usize, usize, usize, usize, usize) {
    let (
        lines,
        wrapped_count,
        _omitted_wrapped_line_count,
        stripped,
        ascii_fallback_count,
        control_sequence_stripped_count,
        inline_control_normalized_count,
        control_char_normalized_count,
        format_control_stripped_count,
    ) = terminal_draw_soft_wrapped_lines_with_unicode_budget(
        line,
        max_cells,
        unicode_enabled,
        usize::MAX,
    );
    (
        lines,
        wrapped_count,
        stripped,
        ascii_fallback_count,
        control_sequence_stripped_count,
        inline_control_normalized_count,
        control_char_normalized_count,
        format_control_stripped_count,
    )
}

fn terminal_draw_soft_wrapped_lines_with_unicode_budget(
    line: &str,
    max_cells: usize,
    unicode_enabled: bool,
    max_materialized_lines: usize,
) -> (
    Vec<String>,
    usize,
    usize,
    bool,
    usize,
    usize,
    usize,
    usize,
    usize,
) {
    if max_cells == 0 {
        let (_, control_sequence_stripped_count, inline_control_normalized_count) =
            terminal_sanitize_terminal_control_text(line);
        let omitted_wrapped_line_count =
            usize::from(max_materialized_lines == 0 && !line.is_empty());
        return (
            if max_materialized_lines == 0 {
                Vec::new()
            } else {
                vec![String::new()]
            },
            usize::from(!line.is_empty()),
            omitted_wrapped_line_count,
            control_sequence_stripped_count > 0 || inline_control_normalized_count > 0,
            0,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            0,
            0,
        );
    }

    let mut lines = Vec::new();
    let mut current_parts = Vec::<String>::new();
    let mut current_cells = 0usize;
    let mut stripped = false;
    let mut ascii_fallback_count = 0usize;
    let mut control_sequence_stripped_count = 0usize;
    let mut inline_control_normalized_count = 0usize;
    let mut control_char_normalized_count = 0usize;
    let mut format_control_stripped_count = 0usize;
    let mut wrapped_count = 0usize;
    let mut omitted_wrapped_line_count = 0usize;
    let mut graphemes = UnicodeSegmentation::graphemes(line, true).peekable();

    while let Some(grapheme) = graphemes.next() {
        if terminal_consume_terminal_control_grapheme(grapheme, &mut graphemes) {
            control_sequence_stripped_count = control_sequence_stripped_count.saturating_add(1);
            stripped = true;
            continue;
        }
        match grapheme {
            "\r" => {
                lines.clear();
                current_parts.clear();
                current_cells = 0;
                wrapped_count = 0;
                omitted_wrapped_line_count = 0;
                inline_control_normalized_count = inline_control_normalized_count.saturating_add(1);
                stripped = true;
                continue;
            }
            "\u{8}" | "\u{7f}" => {
                if let Some(part) = current_parts.pop() {
                    current_cells =
                        current_cells.saturating_sub(UnicodeWidthStr::width(part.as_str()));
                } else {
                    terminal_pop_last_grapheme_from_materialized_lines(&mut lines);
                }
                inline_control_normalized_count = inline_control_normalized_count.saturating_add(1);
                stripped = true;
                continue;
            }
            _ => {}
        }
        let (safe, safe_stripped, fallback_count, control_char_count, format_control_count) =
            terminal_safe_grapheme(grapheme, unicode_enabled);
        stripped |= safe_stripped;
        if safe.is_empty() {
            control_char_normalized_count =
                control_char_normalized_count.saturating_add(control_char_count);
            format_control_stripped_count =
                format_control_stripped_count.saturating_add(format_control_count);
            continue;
        }
        let width = UnicodeWidthStr::width(safe.as_str());

        if width > max_cells {
            if !current_parts.is_empty() {
                if !terminal_draw_push_soft_wrapped_line_with_budget(
                    &mut lines,
                    terminal_take_soft_wrap_current_line(&mut current_parts),
                    max_materialized_lines,
                    &mut omitted_wrapped_line_count,
                ) {
                    break;
                }
                current_cells = 0;
                wrapped_count = wrapped_count.saturating_add(1);
            }
            let (
                bounded,
                truncated,
                fallback_stripped,
                fallback_fallback_count,
                fallback_control_sequence_stripped_count,
                fallback_inline_control_normalized_count,
                fallback_control_char_normalized_count,
                fallback_format_control_stripped_count,
            ) = terminal_draw_bounded_line_with_unicode(&safe, max_cells, unicode_enabled);
            stripped |= fallback_stripped || truncated;
            ascii_fallback_count = ascii_fallback_count.saturating_add(fallback_fallback_count);
            stripped |= fallback_control_sequence_stripped_count > 0;
            stripped |= fallback_inline_control_normalized_count > 0;
            stripped |= fallback_control_char_normalized_count > 0;
            stripped |= fallback_format_control_stripped_count > 0;
            control_char_normalized_count = control_char_normalized_count
                .saturating_add(fallback_control_char_normalized_count);
            format_control_stripped_count = format_control_stripped_count
                .saturating_add(fallback_format_control_stripped_count);
            if !terminal_draw_push_soft_wrapped_line_with_budget(
                &mut lines,
                bounded,
                max_materialized_lines,
                &mut omitted_wrapped_line_count,
            ) {
                break;
            }
            wrapped_count = wrapped_count.saturating_add(1);
            continue;
        }

        if current_cells.saturating_add(width) > max_cells && !current_parts.is_empty() {
            if !terminal_draw_push_soft_wrapped_line_with_budget(
                &mut lines,
                terminal_take_soft_wrap_current_line(&mut current_parts),
                max_materialized_lines,
                &mut omitted_wrapped_line_count,
            ) {
                break;
            }
            current_cells = 0;
            wrapped_count = wrapped_count.saturating_add(1);
        }

        ascii_fallback_count = ascii_fallback_count.saturating_add(fallback_count);
        control_char_normalized_count =
            control_char_normalized_count.saturating_add(control_char_count);
        format_control_stripped_count =
            format_control_stripped_count.saturating_add(format_control_count);
        current_cells = current_cells.saturating_add(width);
        current_parts.push(safe);
    }

    if !current_parts.is_empty() {
        terminal_draw_push_soft_wrapped_line_with_budget(
            &mut lines,
            terminal_take_soft_wrap_current_line(&mut current_parts),
            max_materialized_lines,
            &mut omitted_wrapped_line_count,
        );
    } else if lines.is_empty() && omitted_wrapped_line_count == 0 && max_materialized_lines > 0 {
        lines.push(String::new());
    }

    (
        lines,
        wrapped_count,
        omitted_wrapped_line_count,
        stripped,
        ascii_fallback_count,
        control_sequence_stripped_count,
        inline_control_normalized_count,
        control_char_normalized_count,
        format_control_stripped_count,
    )
}

fn terminal_draw_push_soft_wrapped_line_with_budget(
    lines: &mut Vec<String>,
    line: String,
    max_materialized_lines: usize,
    omitted_wrapped_line_count: &mut usize,
) -> bool {
    if lines.len() < max_materialized_lines {
        lines.push(line);
        true
    } else {
        *omitted_wrapped_line_count = omitted_wrapped_line_count.saturating_add(1);
        false
    }
}

fn terminal_take_soft_wrap_current_line(current_parts: &mut Vec<String>) -> String {
    let line = current_parts.concat();
    current_parts.clear();
    line
}

fn terminal_pop_last_grapheme_from_materialized_lines(lines: &mut Vec<String>) {
    while let Some(line) = lines.last_mut() {
        let Some((index, _)) =
            UnicodeSegmentation::grapheme_indices(line.as_str(), true).next_back()
        else {
            let _ = lines.pop();
            continue;
        };
        line.truncate(index);
        if line.is_empty() {
            let _ = lines.pop();
        }
        break;
    }
}

fn terminal_consume_terminal_control_grapheme<'a, I>(
    grapheme: &str,
    graphemes: &mut std::iter::Peekable<I>,
) -> bool
where
    I: Iterator<Item = &'a str>,
{
    match terminal_single_char_grapheme(grapheme) {
        Some('\u{1b}') => {
            terminal_consume_escape_sequence_graphemes(graphemes);
            true
        }
        Some('\u{9b}') => {
            terminal_consume_csi_sequence_graphemes(graphemes);
            true
        }
        Some('\u{9d}') => {
            terminal_consume_string_control_sequence_graphemes(graphemes, true);
            true
        }
        Some('\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}') => {
            terminal_consume_string_control_sequence_graphemes(graphemes, false);
            true
        }
        _ => false,
    }
}

fn terminal_single_char_grapheme(grapheme: &str) -> Option<char> {
    let mut chars = grapheme.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

fn terminal_consume_escape_sequence_graphemes<'a, I>(graphemes: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = &'a str>,
{
    let Some(grapheme) = graphemes.next() else {
        return;
    };
    match terminal_single_char_grapheme(grapheme) {
        Some('[') => terminal_consume_csi_sequence_graphemes(graphemes),
        Some(']') => terminal_consume_string_control_sequence_graphemes(graphemes, true),
        Some('P' | 'X' | '^' | '_') => {
            terminal_consume_string_control_sequence_graphemes(graphemes, false)
        }
        _ => {}
    }
}

fn terminal_consume_csi_sequence_graphemes<'a, I>(graphemes: &mut I)
where
    I: Iterator<Item = &'a str>,
{
    for grapheme in graphemes.by_ref() {
        if terminal_single_char_grapheme(grapheme).is_some_and(terminal_is_ansi_final_byte) {
            break;
        }
    }
}

fn terminal_consume_string_control_sequence_graphemes<'a, I>(
    graphemes: &mut std::iter::Peekable<I>,
    stop_on_bel: bool,
) where
    I: Iterator<Item = &'a str>,
{
    while let Some(grapheme) = graphemes.next() {
        match terminal_single_char_grapheme(grapheme) {
            Some('\u{7}') if stop_on_bel => break,
            Some('\u{1b}')
                if graphemes
                    .peek()
                    .and_then(|next| terminal_single_char_grapheme(next))
                    .is_some_and(|next| next == '\\') =>
            {
                let _ = graphemes.next();
                break;
            }
            Some('\u{9c}') => break,
            _ => {}
        }
    }
}

fn terminal_sanitize_terminal_control_text(line: &str) -> (String, usize, usize) {
    let (line, control_sequence_stripped_count) = terminal_strip_terminal_control_sequences(line);
    let (line, inline_control_normalized_count) =
        terminal_normalize_inline_terminal_controls(line.as_str());
    (
        line,
        control_sequence_stripped_count,
        inline_control_normalized_count,
    )
}

fn terminal_strip_terminal_control_sequences(line: &str) -> (String, usize) {
    let mut out = String::with_capacity(line.len());
    let mut stripped_count = 0usize;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => {
                stripped_count = stripped_count.saturating_add(1);
                terminal_consume_escape_sequence(&mut chars);
            }
            '\u{9b}' => {
                stripped_count = stripped_count.saturating_add(1);
                terminal_consume_csi_sequence(&mut chars);
            }
            '\u{9d}' => {
                stripped_count = stripped_count.saturating_add(1);
                terminal_consume_string_control_sequence(&mut chars, true);
            }
            '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => {
                stripped_count = stripped_count.saturating_add(1);
                terminal_consume_string_control_sequence(&mut chars, false);
            }
            _ => out.push(ch),
        }
    }

    (out, stripped_count)
}

fn terminal_normalize_inline_terminal_controls(line: &str) -> (String, usize) {
    let mut visible_graphemes = Vec::<String>::new();
    let mut normalized_count = 0usize;

    for grapheme in UnicodeSegmentation::graphemes(line, true) {
        match grapheme {
            "\r" => {
                visible_graphemes.clear();
                normalized_count = normalized_count.saturating_add(1);
            }
            "\u{8}" | "\u{7f}" => {
                let _ = visible_graphemes.pop();
                normalized_count = normalized_count.saturating_add(1);
            }
            _ => visible_graphemes.push(grapheme.to_string()),
        }
    }

    (visible_graphemes.concat(), normalized_count)
}

fn terminal_consume_escape_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    let Some(ch) = chars.next() else {
        return;
    };
    match ch {
        '[' => terminal_consume_csi_sequence(chars),
        ']' => terminal_consume_string_control_sequence(chars, true),
        'P' | 'X' | '^' | '_' => terminal_consume_string_control_sequence(chars, false),
        _ => {}
    }
}

fn terminal_consume_csi_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    for ch in chars.by_ref() {
        if terminal_is_ansi_final_byte(ch) {
            break;
        }
    }
}

fn terminal_consume_string_control_sequence<I>(
    chars: &mut std::iter::Peekable<I>,
    stop_on_bel: bool,
) where
    I: Iterator<Item = char>,
{
    while let Some(ch) = chars.next() {
        if stop_on_bel && ch == '\u{7}' {
            break;
        }
        if ch == '\u{1b}' {
            if chars.peek().is_some_and(|next| *next == '\\') {
                let _ = chars.next();
                break;
            }
        } else if ch == '\u{9c}' {
            break;
        }
    }
}

fn terminal_is_ansi_final_byte(ch: char) -> bool {
    matches!(ch as u32, 0x40..=0x7e)
}

fn terminal_safe_grapheme(
    grapheme: &str,
    unicode_enabled: bool,
) -> (String, bool, usize, usize, usize) {
    let mut out = String::new();
    let mut stripped = false;
    let mut control_char_normalized_count = 0usize;
    let mut format_control_stripped_count = 0usize;
    for ch in grapheme.chars() {
        if terminal_control_char_becomes_space(ch) {
            out.push(' ');
            stripped = true;
            control_char_normalized_count = control_char_normalized_count.saturating_add(1);
        } else if ch.is_control() {
            stripped = true;
            control_char_normalized_count = control_char_normalized_count.saturating_add(1);
        } else if terminal_unsafe_format_control(ch) {
            stripped = true;
            format_control_stripped_count = format_control_stripped_count.saturating_add(1);
        } else {
            out.push(ch);
        }
    }
    if unicode_enabled || out.is_ascii() {
        return (
            out,
            stripped,
            0,
            control_char_normalized_count,
            format_control_stripped_count,
        );
    }

    (
        terminal_ascii_fallback_grapheme(&out),
        stripped,
        1,
        control_char_normalized_count,
        format_control_stripped_count,
    )
}

fn terminal_control_char_becomes_space(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\u{b}' | '\u{c}')
}

fn terminal_unsafe_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn terminal_ascii_fallback_grapheme(grapheme: &str) -> String {
    match grapheme {
        "✓" | "✔" | "✅" => "+".to_string(),
        "✗" | "✘" | "✕" | "❌" | "×" => "x".to_string(),
        "→" | "⇒" | "➜" | "➤" | "▶" | "▸" | "›" | "»" => ">".to_string(),
        "←" | "⇐" | "‹" | "«" => "<".to_string(),
        "•" | "●" | "◦" | "▪" | "▫" | "■" | "□" | "◆" | "◇" => "*".to_string(),
        "─" | "━" | "═" | "—" | "–" | "−" => "-".to_string(),
        "│" | "┃" | "║" | "┆" | "┊" => "|".to_string(),
        "┌" | "┐" | "└" | "┘" | "├" | "┤" | "┬" | "┴" | "┼" | "╭" | "╮" | "╰" | "╯" | "╞" | "╡"
        | "╤" | "╧" | "╪" | "╔" | "╗" | "╚" | "╝" | "╠" | "╣" | "╦" | "╩" | "╬" => {
            "+".to_string()
        }
        "…" => "...".to_string(),
        _ => "?".to_string(),
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn string_path(value: &Value, path: &[&str]) -> String {
    value_path(value, path)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    value_path(value, path).and_then(Value::as_bool)
}

fn value_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};

    #[derive(Default)]
    struct FailAfterSynchronizedUpdateWriter {
        bytes: Vec<u8>,
        failed_once: bool,
    }

    impl Write for FailAfterSynchronizedUpdateWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let sync_started = self
                .bytes
                .windows(b"\x1b[?2026h".len())
                .any(|window| window == b"\x1b[?2026h");
            if sync_started && !self.failed_once {
                self.failed_once = true;
                return Err(io::Error::other("forced terminal write failure"));
            }
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailAfterStyleColorWriter {
        bytes: Vec<u8>,
        failed_once: bool,
    }

    impl Write for FailAfterStyleColorWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let ansi_started = self
                .bytes
                .windows(b"\x1b[".len())
                .any(|window| window == b"\x1b[");
            let style_started = ansi_started && self.bytes.contains(&b'm');
            if style_started && !self.failed_once {
                self.failed_once = true;
                return Err(io::Error::other("forced styled terminal write failure"));
            }
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn frame(frame_hash: &str, changed_region_ids: Vec<&str>) -> Value {
        json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 7,
            "frameId": "render-frame-00000007",
            "frameHash": frame_hash,
            "draw": {
                "dirty": true,
                "changedRegionIds": changed_region_ids,
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "replace",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "active",
                    "role": "activity",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "patch",
                    "regionHash": "hash-active",
                    "lines": ["assistant text: 5 bytes"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        })
    }

    #[test]
    fn renderer_emits_replace_ops_for_changed_regions() {
        let mut renderer = StreamJsonTerminalPatchRenderer::new();

        let patch = renderer.render_frame_value(&frame("frame-a", vec!["status", "active"]));

        assert_eq!(patch["type"], STREAM_JSON_RENDER_PATCH_TYPE);
        assert_eq!(
            patch["schemaVersion"],
            STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION
        );
        assert_eq!(patch["draw"]["replaceWholeScreen"], false);
        assert_eq!(patch["draw"]["skipped"], false);
        assert_eq!(patch["flush"]["shouldFlush"], true);
        assert_eq!(patch["cursor"]["preservePrompt"], true);
        let ops = patch["operations"].as_array().expect("operations");
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0]["op"], "replace_region");
        assert_eq!(ops[0]["regionId"], "status");
        assert_eq!(ops[0]["lines"][0], "Thinking | main");
        assert_eq!(ops[1]["regionId"], "active");
    }

    #[test]
    fn patch_renderer_caps_pathological_region_lines_before_terminal_ops() {
        let mut renderer = StreamJsonTerminalPatchRenderer::new();
        let mut huge_frame = frame("frame-huge-region", vec!["active"]);
        let huge_lines = (0..STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES + 5)
            .map(|index| format!("line-{index}"))
            .collect::<Vec<_>>();
        huge_frame["regions"][1]["lines"] = json!(huge_lines);

        let patch = renderer.render_frame_value(&huge_frame);
        let operation = patch["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .find(|operation| operation["regionId"] == "active")
            .expect("active operation");

        assert_eq!(
            operation["lineCount"],
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );
        assert_eq!(
            operation["lineBudget"]["maxLines"],
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );
        assert_eq!(
            operation["lineBudget"]["sourceLineCount"],
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES + 5
        );
        assert_eq!(operation["lineBudget"]["omittedLineCount"], 5);
        assert_eq!(operation["lineBudget"]["exceeded"], true);
        assert_eq!(operation["truncated"], true);
        assert_eq!(
            operation["lines"].as_array().expect("bounded lines").len(),
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );

        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let plan = scheduler.render_patch_value(&patch);
        assert_eq!(plan["draw"]["regionLineBudgeted"], true);
        assert_eq!(
            plan["draw"]["regionLineBudgetPolicy"],
            "cap_region_lines_before_terminal_ops"
        );
        assert_eq!(
            plan["draw"]["regionLineBudgetMaxLines"],
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );
        assert_eq!(plan["draw"]["regionLineBudgetedRegionCount"], 1);
        assert_eq!(plan["draw"]["regionLineBudgetOmittedLineCount"], 5);
        assert_eq!(plan["draw"]["terminalOpsPrebudgetedLines"], true);
        let region_plan = plan["regionPlans"]
            .as_array()
            .expect("region plans")
            .iter()
            .find(|plan| plan["regionId"] == "active")
            .expect("active region plan");
        assert_eq!(region_plan["lineBudget"]["omittedLineCount"], 5);
        let active_write_count = plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .filter(|operation| {
                operation["op"] == "write_line" && operation["regionId"] == "active"
            })
            .count();
        assert_eq!(
            active_write_count,
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );
    }

    #[test]
    fn renderer_skips_duplicate_frame_hash() {
        let mut renderer = StreamJsonTerminalPatchRenderer::new();

        let first = renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let second = renderer.render_frame_value(&frame("frame-a", vec!["status"]));

        assert_eq!(first["draw"]["skipped"], false);
        assert_eq!(second["draw"]["skipped"], true);
        assert_eq!(second["draw"]["skipReason"], "frame_hash_unchanged");
        assert_eq!(
            second["operations"].as_array().expect("operations").len(),
            0
        );
        assert_eq!(second["flush"]["shouldFlush"], false);
    }

    #[test]
    fn renderer_forces_redraw_for_resize_even_when_frame_hash_is_unchanged() {
        let mut renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let first = frame("frame-a", vec!["status"]);
        let _ = renderer.render_frame_value(&first);

        let mut unchanged = frame("frame-a", vec![]);
        unchanged["draw"]["dirty"] = json!(false);
        let skipped = renderer.render_frame_value(&unchanged);
        assert_eq!(skipped["draw"]["skipped"], true);
        assert_eq!(skipped["draw"]["skipReason"], "frame_hash_unchanged");

        let forced = renderer.render_frame_value_forced(&unchanged, "viewport_resize");
        assert_eq!(forced["draw"]["skipped"], false);
        assert_eq!(forced["draw"]["forcedRedraw"], true);
        assert_eq!(forced["draw"]["forceRedrawReason"], "viewport_resize");
        assert_eq!(forced["draw"]["skipIfFrameHashUnchanged"], false);
        assert_eq!(forced["flush"]["shouldFlush"], true);
        assert_eq!(forced["flush"]["policy"], "immediate");
        assert!(
            forced["operations"]
                .as_array()
                .expect("forced operations")
                .len()
                >= 2
        );

        let draw_plan = scheduler.render_patch_value(&forced);
        assert_eq!(draw_plan["draw"]["forcedRedraw"], true);
        assert_eq!(draw_plan["draw"]["forceRedrawReason"], "viewport_resize");
        assert_eq!(draw_plan["schedule"]["flushPolicy"], "immediate");
        assert_eq!(draw_plan["schedule"]["dropWhenSuperseded"], false);
        assert_eq!(draw_plan["schedule"]["supersededSequenceBypass"], true);
        assert_eq!(draw_plan["schedule"]["shouldFlush"], true);
    }

    #[test]
    fn renderer_strips_controls_and_truncates_pathological_lines() {
        let mut renderer = StreamJsonTerminalPatchRenderer::new();
        let mut frame = frame("frame-a", vec!["active"]);
        frame["regions"][1]["lines"] = json!([format!("\u{1b}\n{}tail", "x".repeat(300))]);

        let patch = renderer.render_frame_value(&frame);
        let op = &patch["operations"].as_array().expect("operations")[0];
        let line = op["lines"][0].as_str().expect("line");

        assert!(line.ends_with("..."));
        assert!(!line.contains('\n'));
        assert!(!line.contains('\u{1b}'));
        assert_eq!(op["truncated"], true);
        assert_eq!(op["controlCharsStripped"], true);
    }

    #[test]
    fn draw_scheduler_emits_anchored_terminal_ops_and_restores_cursor() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();

        let patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status", "active"]));
        let plan = scheduler.render_patch_value(&patch);

        assert_eq!(plan["type"], STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        assert_eq!(
            plan["schemaVersion"],
            STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION
        );
        assert_eq!(plan["draw"]["strategy"], "anchored_region_patch");
        assert_eq!(plan["draw"]["replaceWholeScreen"], false);
        assert_eq!(plan["schedule"]["dropWhenSuperseded"], true);
        assert_eq!(plan["cursor"]["saveBeforeDraw"], true);
        assert_eq!(plan["cursor"]["restoreAfterDraw"], true);
        let terminal_ops = plan["terminalOps"].as_array().expect("terminal ops");
        assert_eq!(terminal_ops.first().unwrap()["op"], "save_cursor");
        assert_eq!(terminal_ops.last().unwrap()["op"], "restore_cursor");
        assert!(terminal_ops
            .iter()
            .any(|op| op["op"] == "move_to_row" && op["row"] == "top+0"));
        assert!(terminal_ops
            .iter()
            .any(|op| op["op"] == "move_to_row" && op["row"] == "bottom-1"));
    }

    #[test]
    fn draw_scheduler_uses_source_event_sequence_for_event_patch() {
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = json!({
            "schemaVersion": 1,
            "sourceEventSequence": 42,
            "operations": [{
                "op": "replace_region",
                "regionId": "slash_result",
                "role": "slash_result",
                "anchor": "top",
                "placement": "top",
                "updateMode": "replace_slash_result",
                "topStartRow": 1,
                "layoutMode": "dynamic_top_stack",
                "lines": ["rules: allow 1, deny 0, ask 2"],
            }],
            "flush": {
                "shouldFlush": true,
                "policy": "immediate",
                "coalesceSafe": false,
            },
            "cursor": {
                "preservePrompt": true,
                "restoreAfterDraw": true,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
        });

        let plan = scheduler.render_patch_value(&patch);

        assert_eq!(plan["sequence"].as_u64(), Some(42));
        assert_eq!(plan["draw"]["topLayoutMode"], "dynamic_stack");
        assert_eq!(plan["regionPlans"][0]["startRow"], "top+1");
        assert!(plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .any(|op| op["op"] == "move_to_row" && op["row"] == "top+1"));
    }

    #[test]
    fn draw_scheduler_attaches_semantic_styles_to_region_lines() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 11,
            "frameId": "render-frame-00000011",
            "frameHash": "frame-style",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["status", "command", "diff", "error", "footer"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "command",
                    "role": "command",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_summary",
                    "regionHash": "hash-command",
                    "lines": ["cmd: cargo test", "exit: 0", "duration: 120ms"]
                },
                {
                    "id": "diff",
                    "role": "diff",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_collapsed",
                    "regionHash": "hash-diff",
                    "lines": ["diff summary", "+ added line", "- removed line", "diff: collapsed"]
                },
                {
                    "id": "error",
                    "role": "error",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_layered",
                    "regionHash": "hash-error",
                    "lines": ["error", "summary: failed"]
                },
                {
                    "id": "footer",
                    "role": "footer",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "replace",
                    "regionHash": "hash-footer",
                    "lines": ["streaming | coalescing updates every 33ms"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });

        let patch = patch_renderer.render_frame_value(&frame);
        let plan = scheduler.render_patch_value(&patch);

        assert_eq!(patch["style"]["semanticColors"], true);
        assert_eq!(plan["style"]["semanticColors"], true);
        assert_eq!(plan["style"]["plainTextFallback"], true);
        assert_eq!(plan["style"]["resetAfterLine"], true);
        assert_eq!(plan["style"]["graphemeClusterSafe"], true);
        assert_eq!(plan["style"]["asciiFallback"], true);
        assert_eq!(plan["safety"]["graphemeClusterWidthGuard"], true);
        assert_eq!(plan["safety"]["asciiGlyphFallback"], true);
        assert_eq!(plan["safety"]["terminalControlSequencesStripped"], true);
        assert_eq!(plan["safety"]["oscControlSequencesStripped"], true);
        assert_eq!(plan["safety"]["inlineControlsNormalized"], true);
        assert_eq!(plan["safety"]["carriageReturnProgressNormalized"], true);
        assert_eq!(plan["safety"]["backspaceProgressNormalized"], true);
        assert_eq!(plan["safety"]["c0ControlCharsNormalized"], true);
        assert_eq!(plan["safety"]["tabsNormalizedToSpaces"], true);
        assert_eq!(plan["safety"]["newlineWritesSuppressed"], true);
        assert_eq!(plan["safety"]["bidiControlsStripped"], true);
        assert_eq!(plan["safety"]["unsafeFormatControlsStripped"], true);
        assert_eq!(plan["safety"]["unicodeBidiSpoofGuard"], true);
        assert_eq!(plan["safety"]["semanticStyleResets"], true);
        let terminal_ops = plan["terminalOps"].as_array().expect("terminal ops");
        assert!(terminal_ops.iter().any(|op| {
            op["op"] == "write_line"
                && op["regionId"] == "status"
                && op["semanticStyle"] == "accent"
        }));
        assert!(terminal_ops.iter().any(|op| {
            op["op"] == "write_line" && op["text"] == "exit: 0" && op["semanticStyle"] == "success"
        }));
        assert!(terminal_ops.iter().any(|op| {
            op["op"] == "write_line"
                && op["text"] == "- removed line"
                && op["semanticStyle"] == "error"
        }));
        assert!(terminal_ops.iter().any(|op| {
            op["op"] == "write_line" && op["regionId"] == "footer" && op["semanticStyle"] == "muted"
        }));
    }

    #[test]
    fn draw_scheduler_clears_stale_lines_when_region_shrinks() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut first_frame = frame("frame-a", vec!["active"]);
        first_frame["regions"][1]["lines"] = json!(["one", "two", "three"]);
        let mut second_frame = frame("frame-b", vec!["active"]);
        second_frame["regions"][1]["lines"] = json!(["one"]);

        let first_patch = patch_renderer.render_frame_value(&first_frame);
        let _ = scheduler.render_patch_value(&first_patch);
        let second_patch = patch_renderer.render_frame_value(&second_frame);
        let second_plan = scheduler.render_patch_value(&second_patch);

        let region_plan = &second_plan["regionPlans"].as_array().expect("region plans")[0];
        assert_eq!(region_plan["regionId"], "active");
        assert_eq!(region_plan["previousLineCount"], 3);
        assert_eq!(region_plan["lineCount"], 1);
        assert_eq!(region_plan["clearTrailingLineCount"], 2);
        let stale_clear_count = second_plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .filter(|op| op["op"] == "clear_line" && op["stale"] == true)
            .count();
        assert_eq!(stale_clear_count, 2);
    }

    #[test]
    fn patch_renderer_redraws_unchanged_top_region_when_stack_offset_changes() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let first_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 21,
            "frameId": "render-frame-00000021",
            "frameHash": "frame-top-stack-first",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["status", "diff"],
            },
            "changes": {
                "firstFrame": true,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "diff",
                    "role": "diff",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_collapsed",
                    "regionHash": "hash-diff",
                    "lines": ["diff summary"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
        });
        let second_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 22,
            "frameId": "render-frame-00000022",
            "frameHash": "frame-top-stack-second",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["command"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "command",
                    "role": "command",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_summary",
                    "regionHash": "hash-command",
                    "lines": ["$ cargo test", "running"]
                },
                {
                    "id": "diff",
                    "role": "diff",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_collapsed",
                    "regionHash": "hash-diff",
                    "lines": ["diff summary"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
        });

        let _ = patch_renderer.render_frame_value(&first_frame);
        let second_patch = patch_renderer.render_frame_value(&second_frame);
        let ops = second_patch["operations"].as_array().expect("operations");
        let region_ids = ops
            .iter()
            .filter_map(|op| op.get("regionId").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(region_ids, vec!["command", "diff"]);
        assert_eq!(ops[0]["topStartRow"], 1);
        assert_eq!(ops[1]["topStartRow"], 3);
        assert_eq!(ops[1]["layoutMode"], "dynamic_top_stack");
    }

    #[test]
    fn draw_scheduler_clears_unoccupied_previous_top_rows_after_stack_compacts() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let first_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 31,
            "frameId": "render-frame-00000031",
            "frameHash": "frame-top-stack-expanded",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["status", "command", "diff"],
            },
            "changes": {
                "firstFrame": true,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "command",
                    "role": "command",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_summary",
                    "regionHash": "hash-command",
                    "lines": ["$ cargo test", "running"]
                },
                {
                    "id": "diff",
                    "role": "diff",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_collapsed",
                    "regionHash": "hash-diff",
                    "lines": ["diff summary"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
        });
        let compacted_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 32,
            "frameId": "render-frame-00000032",
            "frameHash": "frame-top-stack-compacted",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["command"],
            },
            "changes": {
                "firstFrame": false,
                "retiredRegions": [
                    {
                        "id": "command",
                        "role": "command",
                        "anchor": "top",
                        "placement": "top",
                        "regionHash": "hash-command",
                        "previousLineCount": 2
                    }
                ]
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "diff",
                    "role": "diff",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_collapsed",
                    "regionHash": "hash-diff",
                    "lines": ["diff summary"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
        });

        let first_patch = patch_renderer.render_frame_value(&first_frame);
        let _ = scheduler.render_patch_value(&first_patch);
        let compacted_patch = patch_renderer.render_frame_value(&compacted_frame);
        let compacted_plan = scheduler.render_patch_value(&compacted_patch);
        let ops = compacted_plan["terminalOps"]
            .as_array()
            .expect("terminal ops");

        assert_eq!(compacted_plan["draw"]["topLayoutCompactsGaps"], true);
        assert!(ops.iter().any(|op| {
            op["op"] == "move_to_row" && op["regionId"] == "diff" && op["row"] == "top+3"
        }));
        assert!(ops.iter().any(|op| {
            op["op"] == "clear_line" && op["regionId"] == "diff" && op["previousTopRow"] == true
        }));
    }

    #[test]
    fn draw_scheduler_clears_retired_regions_from_frame_delta() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let first_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 8,
            "frameId": "render-frame-00000008",
            "frameHash": "frame-retired-first",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["approval"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "approval",
                    "role": "approval",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "replace_blocking",
                    "regionHash": "hash-approval",
                    "lines": ["approval required", "tool: Bash", "blocking"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });
        let first_patch = patch_renderer.render_frame_value(&first_frame);
        let _ = scheduler.render_patch_value(&first_patch);

        let resolved_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 9,
            "frameId": "render-frame-00000009",
            "frameHash": "frame-retired-resolved",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["approval"],
            },
            "changes": {
                "firstFrame": false,
                "removedRegionIds": ["approval"],
                "retiredRegions": [
                    {
                        "id": "approval",
                        "role": "approval",
                        "anchor": "bottom",
                        "placement": "bottom",
                        "regionHash": "hash-approval",
                        "previousLineCount": 3
                    }
                ]
            },
            "regions": [],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });
        let resolved_patch = patch_renderer.render_frame_value(&resolved_frame);
        let clear_op = resolved_patch["operations"]
            .as_array()
            .expect("operations")
            .first()
            .expect("clear op");
        assert_eq!(clear_op["op"], "clear_region");
        assert_eq!(clear_op["updateMode"], "clear_retired");
        assert_eq!(clear_op["previousLineCount"], 3);

        let plan = scheduler.render_patch_value(&resolved_patch);
        let terminal_ops = plan["terminalOps"].as_array().expect("terminal ops");
        let clear_count = terminal_ops
            .iter()
            .filter(|op| op["op"] == "clear_line" && op["regionId"] == "approval")
            .count();
        assert_eq!(clear_count, 3);
        assert_eq!(plan["regionPlans"][0]["lineCount"], 0);
        assert_eq!(plan["regionPlans"][0]["previousLineCount"], 3);
    }

    #[test]
    fn patch_renderer_caps_retired_region_clear_lines_before_terminal_ops() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let source_previous_line_count = STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES + 7;
        let frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 12,
            "frameId": "render-frame-00000012",
            "frameHash": "frame-retired-budget",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["approval"],
            },
            "changes": {
                "firstFrame": false,
                "removedRegionIds": ["approval"],
                "retiredRegions": [
                    {
                        "id": "approval",
                        "role": "approval",
                        "anchor": "bottom",
                        "placement": "bottom",
                        "regionHash": "hash-approval",
                        "previousLineCount": source_previous_line_count
                    }
                ]
            },
            "regions": [],
            "refresh": {
                "throttleMs": 33,
            },
        });

        let patch = patch_renderer.render_frame_value(&frame);
        let operation = patch["operations"]
            .as_array()
            .expect("operations")
            .first()
            .expect("clear operation");
        assert_eq!(operation["op"], "clear_region");
        assert_eq!(
            operation["previousLineCount"],
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );
        assert_eq!(
            operation["lineBudget"]["policy"],
            "cap_retired_region_clear_lines_before_terminal_ops"
        );
        assert_eq!(
            operation["lineBudget"]["sourceLineCount"],
            source_previous_line_count
        );
        assert_eq!(operation["lineBudget"]["omittedLineCount"], 7);
        assert_eq!(operation["lineBudget"]["exceeded"], true);
        assert_eq!(operation["truncated"], true);

        let plan = scheduler.render_patch_value(&patch);
        assert_eq!(plan["draw"]["regionLineBudgetedRegionCount"], 1);
        assert_eq!(plan["draw"]["regionLineBudgetOmittedLineCount"], 7);
        assert_eq!(
            plan["regionPlans"][0]["previousLineCount"],
            STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES
        );
        assert_eq!(
            plan["regionPlans"][0]["lineBudget"]["policy"],
            "cap_retired_region_clear_lines_before_terminal_ops"
        );
        let clear_count = plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .filter(|op| op["op"] == "clear_line" && op["regionId"] == "approval")
            .count();
        assert_eq!(clear_count, STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES);
    }

    #[test]
    fn draw_scheduler_caps_noncritical_top_lines_before_terminal_ops() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let source_line_count = STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES + 5;
        let command_lines = (0..source_line_count)
            .map(|index| format!("cmd-line-{index}"))
            .collect::<Vec<_>>();
        let frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 13,
            "frameId": "render-frame-00000013",
            "frameHash": "frame-top-line-budget",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["command"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "command",
                    "role": "command",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_summary",
                    "regionHash": "hash-command",
                    "lines": command_lines
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
        });

        let patch = patch_renderer.render_frame_value(&frame);
        let plan = scheduler.render_patch_value(&patch);
        assert_eq!(plan["draw"]["terminalOpsLineBudgeted"], true);
        assert_eq!(
            plan["draw"]["drawLineBudgetPolicy"],
            "cap_noncritical_top_lines_before_terminal_ops"
        );
        assert_eq!(
            plan["draw"]["drawLineBudgetMaxNoncriticalTopLines"],
            STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES
        );
        assert_eq!(plan["draw"]["drawLineBudgetedRegionCount"], 1);
        assert_eq!(plan["draw"]["drawLineBudgetOmittedLineCount"], 5);

        let region_plan = plan["regionPlans"]
            .as_array()
            .expect("region plans")
            .iter()
            .find(|plan| plan["regionId"] == "command")
            .expect("command region plan");
        assert_eq!(region_plan["sourceLineCount"], source_line_count);
        assert_eq!(
            region_plan["lineCount"],
            STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES
        );
        assert_eq!(
            region_plan["drawLineBudget"]["policy"],
            "cap_noncritical_top_lines_before_terminal_ops"
        );
        assert_eq!(region_plan["drawLineBudget"]["omittedLineCount"], 5);
        assert_eq!(region_plan["drawLineBudget"]["exceeded"], true);
        let omitted_planned_lines = region_plan["lines"]
            .as_array()
            .expect("planned lines")
            .iter()
            .filter(|line| line["terminalOpsOmitted"] == true)
            .count();
        assert_eq!(omitted_planned_lines, 5);

        let terminal_ops = plan["terminalOps"].as_array().expect("terminal ops");
        let write_count = terminal_ops
            .iter()
            .filter(|operation| {
                operation["op"] == "write_line" && operation["regionId"] == "command"
            })
            .count();
        assert_eq!(
            write_count,
            STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES
        );
        assert!(!terminal_ops.iter().any(|operation| {
            operation["op"] == "write_line"
                && operation["regionId"] == "command"
                && operation["text"] == "cmd-line-120"
        }));
    }

    #[test]
    fn draw_scheduler_caps_cumulative_noncritical_top_lines_before_terminal_ops() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let command_lines = (0..100)
            .map(|index| format!("command-line-{index}"))
            .collect::<Vec<_>>();
        let diff_lines = (0..100)
            .map(|index| format!("diff-line-{index}"))
            .collect::<Vec<_>>();
        let frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 14,
            "frameId": "render-frame-00000014",
            "frameHash": "frame-top-cumulative-line-budget",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["command", "diff"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "command",
                    "role": "command",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_summary",
                    "regionHash": "hash-command",
                    "lines": command_lines
                },
                {
                    "id": "diff",
                    "role": "diff",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_collapsed",
                    "regionHash": "hash-diff",
                    "lines": diff_lines
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
        });

        let patch = patch_renderer.render_frame_value(&frame);
        let plan = scheduler.render_patch_value(&patch);
        assert_eq!(
            plan["draw"]["drawLineBudgetScope"],
            "cumulative_noncritical_top_widgets"
        );
        assert_eq!(
            plan["draw"]["drawLineBudgetMaxNoncriticalTopTotalLines"],
            STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES
        );
        assert_eq!(plan["draw"]["drawLineBudgetedRegionCount"], 1);
        assert_eq!(plan["draw"]["drawLineBudgetOmittedLineCount"], 40);

        let region_plans = plan["regionPlans"].as_array().expect("region plans");
        let command_plan = region_plans
            .iter()
            .find(|plan| plan["regionId"] == "command")
            .expect("command region plan");
        let diff_plan = region_plans
            .iter()
            .find(|plan| plan["regionId"] == "diff")
            .expect("diff region plan");
        assert_eq!(command_plan["lineCount"], 100);
        assert_eq!(command_plan["drawLineBudget"]["maxLines"], 120);
        assert_eq!(command_plan["drawLineBudget"]["omittedLineCount"], 0);
        assert_eq!(diff_plan["lineCount"], 60);
        assert_eq!(diff_plan["drawLineBudget"]["maxLines"], 60);
        assert_eq!(diff_plan["drawLineBudget"]["maxTotalLines"], 160);
        assert_eq!(diff_plan["drawLineBudget"]["omittedLineCount"], 40);
        assert_eq!(diff_plan["drawLineBudget"]["exceeded"], true);

        let terminal_ops = plan["terminalOps"].as_array().expect("terminal ops");
        let command_write_count = terminal_ops
            .iter()
            .filter(|operation| {
                operation["op"] == "write_line" && operation["regionId"] == "command"
            })
            .count();
        let diff_write_count = terminal_ops
            .iter()
            .filter(|operation| operation["op"] == "write_line" && operation["regionId"] == "diff")
            .count();
        assert_eq!(command_write_count, 100);
        assert_eq!(diff_write_count, 60);
        assert_eq!(
            command_write_count + diff_write_count,
            STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES
        );
        assert!(!terminal_ops.iter().any(|operation| {
            operation["op"] == "write_line"
                && operation["regionId"] == "diff"
                && operation["text"] == "diff-line-60"
        }));
    }

    #[test]
    fn draw_scheduler_skips_duplicate_frame_patch_without_terminal_ops() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let _ = scheduler.render_patch_value(&first_patch);
        let duplicate_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let plan = scheduler.render_patch_value(&duplicate_patch);

        assert_eq!(plan["draw"]["skipped"], true);
        assert_eq!(plan["schedule"]["shouldFlush"], false);
        assert_eq!(
            plan["terminalOps"].as_array().expect("terminal ops").len(),
            0
        );
    }

    #[test]
    fn draw_executor_writes_synchronized_region_patch_without_full_clear() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();

        let patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status", "active"]));
        let plan = scheduler.render_patch_value(&patch);
        let mut bytes = Vec::new();
        let mut executor = StreamJsonTerminalDrawExecutor::with_semantic_colors(
            StreamJsonTerminalViewport::new(24, 40),
            true,
        );
        let report = executor
            .apply_draw_plan(&plan, &mut bytes)
            .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert!(report.synchronized_update);
        assert!(report.restored_cursor);
        assert!(report.written_line_count >= 2);
        assert!(ansi.contains("\u{1b}[?2026h"));
        assert!(ansi.contains("\u{1b}[?2026l"));
        assert!(ansi.contains("\u{1b}7"));
        assert!(ansi.contains("\u{1b}8"));
        assert!(ansi.contains("\u{1b}[1;1H"));
        assert!(ansi.contains("\u{1b}[23;1H"));
        assert!(!ansi.contains("\u{1b}[2J"));
    }

    #[test]
    fn draw_executor_fail_closes_synchronized_update_on_write_error() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status", "active"]));
        let plan = scheduler.render_patch_value(&patch);
        let mut writer = FailAfterSynchronizedUpdateWriter::default();
        let mut executor =
            StreamJsonTerminalDrawExecutor::new(StreamJsonTerminalViewport::new(24, 40));

        let err = executor
            .apply_draw_plan(&plan, &mut writer)
            .expect_err("forced write error");
        let ansi = String::from_utf8(writer.bytes).expect("ansi");

        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(ansi.contains("\u{1b}[?2026h"));
        assert!(ansi.contains("\u{1b}[?2026l"));
    }

    #[test]
    fn draw_executor_fail_safe_restores_cursor_on_write_error() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = patch_renderer.render_frame_value(&frame("frame-cursor-error", vec!["status"]));
        let plan = scheduler.render_patch_value(&patch);
        let mut writer = FailAfterSynchronizedUpdateWriter::default();
        let mut executor =
            StreamJsonTerminalDrawExecutor::new(StreamJsonTerminalViewport::new(24, 40));

        let err = executor
            .apply_draw_plan(&plan, &mut writer)
            .expect_err("forced cursor-preserving write error");
        let ansi = String::from_utf8(writer.bytes).expect("ansi");

        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(ansi.contains("\u{1b}7"));
        assert!(ansi.contains("\u{1b}[?2026l"));
        assert!(ansi.contains("\u{1b}8"));
    }

    #[test]
    fn draw_executor_applies_semantic_colors_and_resets_after_each_styled_line() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut error_frame = frame("frame-style-error", vec!["status", "active"]);
        error_frame["regions"][0]["lines"] = json!(["Failed | main"]);
        error_frame["regions"][1]["role"] = json!("error");
        error_frame["regions"][1]["updateMode"] = json!("replace_layered");
        error_frame["regions"][1]["lines"] = json!(["error", "summary: fatal failure"]);

        let patch = patch_renderer.render_frame_value(&error_frame);
        let plan = scheduler.render_patch_value(&patch);
        let mut executor = StreamJsonTerminalDrawExecutor::with_semantic_colors(
            StreamJsonTerminalViewport::new(24, 40),
            true,
        );
        let mut bytes = Vec::new();
        let report = executor
            .apply_draw_plan(&plan, &mut bytes)
            .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert!(report.styled_line_count >= 1);
        assert_eq!(report.style_reset_count, report.styled_line_count);
        assert!(ansi.contains("fatal failure"));
        assert!(ansi.contains("\u{1b}["));
        assert_eq!(terminal_semantic_color("error"), Some(style::Color::Red));
    }

    #[test]
    fn draw_executor_fail_resets_style_on_styled_write_error() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut error_frame = frame("frame-style-error-fail-safe", vec!["status", "active"]);
        error_frame["regions"][0]["lines"] = json!(["Failed | main"]);
        error_frame["regions"][1]["role"] = json!("error");
        error_frame["regions"][1]["updateMode"] = json!("replace_layered");
        error_frame["regions"][1]["lines"] = json!(["error", "summary: fatal failure"]);

        let patch = patch_renderer.render_frame_value(&error_frame);
        let plan = scheduler.render_patch_value(&patch);
        let mut writer = FailAfterStyleColorWriter::default();
        let mut executor = StreamJsonTerminalDrawExecutor::with_semantic_colors(
            StreamJsonTerminalViewport::new(24, 40),
            true,
        );

        let err = executor
            .apply_draw_plan(&plan, &mut writer)
            .expect_err("forced styled write error");
        let ansi = String::from_utf8(writer.bytes).expect("ansi");

        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(ansi.contains("\u{1b}[0m"));
        assert!(ansi.contains("\u{1b}[?2026l"));
    }

    #[test]
    fn draw_executor_plain_text_fallback_omits_semantic_color_writes() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut error_frame = frame("frame-style-plain", vec!["status", "active"]);
        error_frame["regions"][0]["lines"] = json!(["Failed | main"]);
        error_frame["regions"][1]["role"] = json!("error");
        error_frame["regions"][1]["updateMode"] = json!("replace_layered");
        error_frame["regions"][1]["lines"] = json!(["error", "summary: fatal failure"]);

        let patch = patch_renderer.render_frame_value(&error_frame);
        let plan = scheduler.render_patch_value(&patch);
        let mut executor = StreamJsonTerminalDrawExecutor::with_semantic_colors(
            StreamJsonTerminalViewport::new(24, 40),
            false,
        );
        let mut bytes = Vec::new();
        let report = executor
            .apply_draw_plan(&plan, &mut bytes)
            .expect("plain draw plan");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert_eq!(report.styled_line_count, 0);
        assert_eq!(report.style_reset_count, 0);
        assert!(ansi.contains("fatal failure"));
    }

    #[test]
    fn terminal_render_semantic_color_policy_respects_plain_fallback_env() {
        assert!(!terminal_render_semantic_colors_enabled_for_env(
            Some("never"),
            None,
            None,
            None
        ));
        assert!(!terminal_render_semantic_colors_enabled_for_env(
            None,
            Some("1"),
            None,
            None
        ));
        assert!(!terminal_render_semantic_colors_enabled_for_env(
            None,
            None,
            Some("dumb"),
            None
        ));
        assert!(!terminal_render_semantic_colors_enabled_for_env(
            None,
            None,
            None,
            Some("0")
        ));
        assert!(terminal_render_semantic_colors_enabled_for_env(
            Some("always"),
            Some("1"),
            Some("dumb"),
            Some("0")
        ));
    }

    #[test]
    fn terminal_render_unicode_policy_respects_ascii_fallback_env() {
        assert!(!terminal_render_unicode_enabled_for_env(
            Some("never"),
            None,
            None,
            None,
            None
        ));
        assert!(!terminal_render_unicode_enabled_for_env(
            Some("ascii"),
            None,
            None,
            None,
            None
        ));
        assert!(!terminal_render_unicode_enabled_for_env(
            None,
            Some("dumb"),
            Some("en_US.UTF-8"),
            None,
            None
        ));
        assert!(!terminal_render_unicode_enabled_for_env(
            None,
            None,
            Some("C"),
            None,
            Some("en_US.UTF-8")
        ));
        assert!(!terminal_render_unicode_enabled_for_env(
            None,
            None,
            None,
            Some("US-ASCII"),
            None
        ));
        assert!(terminal_render_unicode_enabled_for_env(
            Some("always"),
            Some("dumb"),
            Some("C"),
            None,
            None
        ));
        assert!(terminal_render_unicode_enabled_for_env(
            None,
            None,
            None,
            None,
            Some("en_US.UTF-8")
        ));
    }

    #[test]
    fn terminal_bounded_line_ascii_fallback_replaces_fancy_glyphs() {
        let (
            bounded,
            truncated,
            stripped,
            ascii_fallback_count,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_bounded_line_with_unicode("ok ✅ → 文件", 32, false);

        assert_eq!(bounded, "ok + > ??");
        assert!(!truncated);
        assert!(!stripped);
        assert_eq!(ascii_fallback_count, 4);
        assert_eq!(control_sequence_stripped_count, 0);
        assert_eq!(inline_control_normalized_count, 0);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
        assert!(bounded.is_ascii());
    }

    #[test]
    fn terminal_bounded_line_strips_ansi_and_osc_sequences() {
        let input = "plain \u{1b}[31mred\u{1b}[0m title \u{1b}]0;owned\u{7}done";
        let (
            bounded,
            truncated,
            stripped,
            ascii_fallback_count,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_bounded_line_with_unicode(input, 80, true);

        assert_eq!(bounded, "plain red title done");
        assert!(!truncated);
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_stripped_count, 3);
        assert_eq!(inline_control_normalized_count, 0);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
        assert!(!bounded.contains("[31m"));
        assert!(!bounded.contains("owned"));
        assert!(!bounded.contains('\u{1b}'));
        assert!(!bounded.contains('\u{7}'));
    }

    #[test]
    fn terminal_soft_wrap_strips_csi_without_fragmenting_escape_text() {
        let input = "before \u{1b}[2Jafter \u{1b}[999Dtail";
        let (
            lines,
            _wrapped_count,
            stripped,
            ascii_fallback_count,
            control_sequence_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_soft_wrapped_lines_with_unicode(input, 12, true);
        let rendered = lines.concat();

        assert_eq!(rendered, "before after tail");
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_count, 2);
        assert_eq!(inline_control_normalized_count, 0);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
        assert!(!rendered.contains("[2J"));
        assert!(!rendered.contains("[999D"));
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn terminal_bounded_line_normalizes_carriage_return_progress() {
        let (
            bounded,
            truncated,
            stripped,
            ascii_fallback_count,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_bounded_line_with_unicode("download 10%\rdownload 100%", 80, true);

        assert_eq!(bounded, "download 100%");
        assert!(!truncated);
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_stripped_count, 0);
        assert_eq!(inline_control_normalized_count, 1);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
    }

    #[test]
    fn terminal_bounded_line_normalizes_backspace_progress() {
        let (
            bounded,
            truncated,
            stripped,
            ascii_fallback_count,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_bounded_line_with_unicode("test fail\u{8}\u{8}\u{8}\u{8}pass", 80, true);

        assert_eq!(bounded, "test pass");
        assert!(!truncated);
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_stripped_count, 0);
        assert_eq!(inline_control_normalized_count, 4);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
    }

    #[test]
    fn terminal_bounded_line_normalizes_control_chars_without_terminal_effects() {
        let input = "alpha\tbeta\u{7}bell\nnext\u{0}nul";
        let (
            bounded,
            truncated,
            stripped,
            ascii_fallback_count,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_bounded_line_with_unicode(input, 80, true);

        assert_eq!(bounded, "alpha betabell nextnul");
        assert!(!truncated);
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_stripped_count, 0);
        assert_eq!(inline_control_normalized_count, 0);
        assert_eq!(control_char_normalized_count, 4);
        assert_eq!(format_control_stripped_count, 0);
        assert!(!bounded.contains('\t'));
        assert!(!bounded.contains('\n'));
        assert!(!bounded.contains('\u{7}'));
        assert!(!bounded.contains('\u{0}'));
    }

    #[test]
    fn terminal_bounded_line_strips_bidi_format_controls() {
        let input = "file \u{202e}cod.exe\u{202c} done \u{2066}safe\u{2069}";
        let (
            bounded,
            truncated,
            stripped,
            ascii_fallback_count,
            control_sequence_stripped_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_bounded_line_with_unicode(input, 80, true);

        assert_eq!(bounded, "file cod.exe done safe");
        assert!(!truncated);
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_stripped_count, 0);
        assert_eq!(inline_control_normalized_count, 0);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 4);
        assert!(!bounded.contains('\u{202e}'));
        assert!(!bounded.contains('\u{202c}'));
        assert!(!bounded.contains('\u{2066}'));
        assert!(!bounded.contains('\u{2069}'));
    }

    #[test]
    fn terminal_bounded_line_preserves_complex_unicode_graphemes() {
        let rainbow_flag = "🏳️‍🌈";
        let (bounded, truncated, stripped) =
            terminal_draw_bounded_line(&format!("a{rainbow_flag}b"), 5);

        assert_eq!(bounded, "a...");
        assert_eq!(UnicodeWidthStr::width(bounded.as_str()), 4);
        assert!(truncated);
        assert!(!stripped);
        assert!(!bounded.contains('\u{200d}'));
        assert!(!bounded.contains('🏳'));
    }

    #[test]
    fn terminal_soft_wrap_preserves_complex_unicode_graphemes() {
        let rainbow_flag = "🏳️‍🌈";
        let input = format!("ab{rainbow_flag}cd");
        let (lines, wrapped_count, stripped) = terminal_draw_soft_wrapped_lines(&input, 4);

        assert!(wrapped_count >= 1);
        assert!(!stripped);
        assert_eq!(lines.concat(), input);
        assert!(lines
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 4));
        assert!(lines
            .iter()
            .filter(|line| line.contains('\u{200d}'))
            .all(|line| line.contains(rainbow_flag)));
    }

    #[test]
    fn terminal_soft_wrap_materialization_respects_line_budget() {
        let (
            lines,
            _wrapped_count,
            omitted_wrapped_line_count,
            stripped,
            ascii_fallback_count,
            control_sequence_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_soft_wrapped_lines_with_unicode_budget("abcdefghijklmnop", 4, true, 2);

        assert_eq!(lines, vec!["abcd".to_string(), "efgh".to_string()]);
        assert_eq!(omitted_wrapped_line_count, 1);
        assert!(!stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_count, 0);
        assert_eq!(inline_control_normalized_count, 0);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
    }

    #[test]
    fn terminal_soft_wrap_streaming_sanitizer_handles_controls_with_budget() {
        let input = "abcd\u{1b}[2Jefgh\rfinal\u{8}e";
        let (
            lines,
            _wrapped_count,
            omitted_wrapped_line_count,
            stripped,
            ascii_fallback_count,
            control_sequence_count,
            inline_control_normalized_count,
            control_char_normalized_count,
            format_control_stripped_count,
        ) = terminal_draw_soft_wrapped_lines_with_unicode_budget(input, 4, true, 2);

        assert_eq!(lines, vec!["fina".to_string(), "e".to_string()]);
        assert_eq!(omitted_wrapped_line_count, 0);
        assert!(stripped);
        assert_eq!(ascii_fallback_count, 0);
        assert_eq!(control_sequence_count, 1);
        assert_eq!(inline_control_normalized_count, 2);
        assert_eq!(control_char_normalized_count, 0);
        assert_eq!(format_control_stripped_count, 0);
    }

    #[test]
    fn draw_executor_ascii_fallback_omits_unicode_output() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut frame = frame("frame-ascii-fallback", vec!["active"]);
        frame["regions"][1]["lines"] = json!(["ok ✅ → 文件"]);

        let patch = patch_renderer.render_frame_value(&frame);
        let plan = scheduler.render_patch_value(&patch);
        let mut executor = StreamJsonTerminalDrawExecutor::with_terminal_capabilities(
            StreamJsonTerminalViewport::new(12, 40),
            false,
            false,
        );
        let mut bytes = Vec::new();
        let report = executor
            .apply_draw_plan(&plan, &mut bytes)
            .expect("ascii fallback draw plan");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(report.ascii_fallback_count >= 4);
        assert!(ansi.is_ascii());
        assert!(ansi.contains("ok + > ??"));
        assert!(!ansi.contains('✅'));
        assert!(!ansi.contains('→'));
        assert!(!ansi.contains('文'));
    }

    #[test]
    fn draw_executor_strips_terminal_control_sequences_before_printing() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut frame = frame("frame-control-sequence", vec!["active"]);
        frame["regions"][1]["lines"] =
            json!(["safe \u{1b}[31mred\u{1b}[0m \u{1b}]0;bad-title\u{7}done"]);

        let patch = patch_renderer.render_frame_value(&frame);
        let mut plan = scheduler.render_patch_value(&patch);
        for op in plan["terminalOps"]
            .as_array_mut()
            .expect("terminal ops")
            .iter_mut()
        {
            if op["op"] == "write_line" && op["regionId"] == "active" {
                op["text"] = json!("safe \u{1b}[31mred\u{1b}[0m \u{1b}]0;bad-title\u{7}done");
            }
        }
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 80))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(report.control_sequence_stripped_count >= 3);
        assert!(ansi.contains("safe red done"));
        assert!(!ansi.contains("[31m"));
        assert!(!ansi.contains("bad-title"));
        assert!(!ansi.contains("\u{1b}]0;"));
    }

    #[test]
    fn draw_executor_normalizes_inline_progress_controls_before_printing() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut frame = frame("frame-inline-control", vec!["active"]);
        frame["regions"][1]["lines"] = json!(["build 10%\rbuild 100%"]);

        let patch = patch_renderer.render_frame_value(&frame);
        let mut plan = scheduler.render_patch_value(&patch);
        for op in plan["terminalOps"]
            .as_array_mut()
            .expect("terminal ops")
            .iter_mut()
        {
            if op["op"] == "write_line" && op["regionId"] == "active" {
                op["text"] = json!("build 10%\rbuild 100%");
            }
        }
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 80))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(report.inline_control_normalized_count >= 1);
        assert!(ansi.contains("build 100%"));
        assert!(!ansi.contains("build 10%build 100%"));
        assert!(!ansi.contains('\r'));
    }

    #[test]
    fn draw_executor_normalizes_control_chars_before_printing() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut frame = frame("frame-control-char", vec!["active"]);
        frame["regions"][1]["lines"] = json!(["safe\ttext\u{7}\nnext\u{0}"]);

        let patch = patch_renderer.render_frame_value(&frame);
        let mut plan = scheduler.render_patch_value(&patch);
        for op in plan["terminalOps"]
            .as_array_mut()
            .expect("terminal ops")
            .iter_mut()
        {
            if op["op"] == "write_line" && op["regionId"] == "active" {
                op["text"] = json!("safe\ttext\u{7}\nnext\u{0}");
            }
        }
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 80))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(report.control_char_normalized_count >= 4);
        assert!(ansi.contains("safe text next"));
        assert!(!ansi.contains('\t'));
        assert!(!ansi.contains('\n'));
        assert!(!ansi.contains('\u{7}'));
        assert!(!ansi.contains('\u{0}'));
    }

    #[test]
    fn draw_executor_strips_bidi_format_controls_before_printing() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut frame = frame("frame-bidi-format", vec!["active"]);
        frame["regions"][1]["lines"] = json!(["file \u{202e}cod.exe\u{202c} done"]);

        let patch = patch_renderer.render_frame_value(&frame);
        let mut plan = scheduler.render_patch_value(&patch);
        for op in plan["terminalOps"]
            .as_array_mut()
            .expect("terminal ops")
            .iter_mut()
        {
            if op["op"] == "write_line" && op["regionId"] == "active" {
                op["text"] = json!("file \u{202e}cod.exe\u{202c} done");
            }
        }
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 80))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(report.format_control_stripped_count >= 2);
        assert!(ansi.contains("file cod.exe done"));
        assert!(!ansi.contains('\u{202e}'));
        assert!(!ansi.contains('\u{202c}'));
    }

    #[test]
    fn draw_executor_bounds_lines_to_viewport_width_without_newlines() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut narrow_frame = frame("frame-a", vec!["active"]);
        narrow_frame["regions"][1]["lines"] = json!(["1234567890ABCDEFGHIJ\nbad"]);

        let patch = patch_renderer.render_frame_value(&narrow_frame);
        let plan = scheduler.render_patch_value(&patch);
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 8))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert!(report.bounded_line_count >= 1);
        assert!(ansi.contains("12345..."));
        assert!(!ansi.contains("ABCDEFGHIJ"));
        assert!(!ansi.contains('\n'));
    }

    #[test]
    fn draw_scheduler_advertises_viewport_width_adaptation_contract() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();

        let patch = patch_renderer.render_frame_value(&frame("frame-viewport", vec!["status"]));
        let plan = scheduler.render_patch_value(&patch);

        assert_eq!(plan["draw"]["viewportAdaptive"], true);
        assert_eq!(plan["viewportAdaptation"]["viewportAdaptive"], true);
        assert_eq!(
            plan["viewportAdaptation"]["widthProfiles"],
            json!(["full", "compact", "minimal"])
        );
        assert_eq!(plan["viewportAdaptation"]["fullColumnsFrom"], 120);
        assert_eq!(plan["viewportAdaptation"]["compactColumnsFrom"], 80);
        assert_eq!(plan["viewportAdaptation"]["minimalColumnsBelow"], 80);
        assert_eq!(
            plan["viewportAdaptation"]["statusLinePolicy"],
            "choose_shortest_fitting_variant"
        );
        assert_eq!(
            plan["viewportAdaptation"]["secondaryFieldPolicy"],
            "drop_secondary_before_truncation"
        );
        assert_eq!(
            plan["viewportAdaptation"]["overflowPolicy"],
            "clip_top_stack_before_bottom_regions"
        );
        assert_eq!(
            plan["viewportAdaptation"]["lineWrapPolicy"],
            "soft_wrap_scrollback_bound_anchored_lines"
        );
        assert_eq!(
            plan["viewportAdaptation"]["resizePolicy"],
            "recompute_profile_before_pending_flush"
        );
        assert_eq!(
            plan["viewportAdaptation"]["immediateResizeRedrawPolicy"],
            "force_redraw_current_frame_with_latest_viewport"
        );
    }

    #[test]
    fn draw_executor_reports_viewport_width_profile_tiers() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut adaptive_frame = frame("frame-width-profile", vec!["active"]);
        adaptive_frame["regions"][1]["lines"] = json!([
            "1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890abcdefghijklmnopqrstuvwxyz"
        ]);

        let patch = patch_renderer.render_frame_value(&adaptive_frame);
        let plan = scheduler.render_patch_value(&patch);

        for (columns, profile, status_policy, secondary_policy) in [
            (72, "minimal", "minimal_status", "hide_secondary"),
            (80, "compact", "compact_status", "collapse_secondary"),
            (120, "full", "full_status", "show_all"),
        ] {
            let (_bytes, report) = render_draw_plan_to_terminal_bytes(
                &plan,
                StreamJsonTerminalViewport::new(12, columns),
            )
            .expect("draw plan bytes");
            assert_eq!(report.viewport_rows, 12);
            assert_eq!(report.viewport_columns, columns);
            assert_eq!(report.viewport_width_profile, profile);
            assert_eq!(report.viewport_status_line_policy, status_policy);
            assert_eq!(report.viewport_secondary_field_policy, secondary_policy);
        }
    }

    #[test]
    fn draw_plan_preserves_viewport_selectable_line_variants() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut adaptive_frame = frame("frame-line-variants", vec!["status"]);
        adaptive_frame["regions"][0]["lineVariants"] = json!({
            "full": "Thinking 1s | main | model:very-long-model-name | mode:Supervised | reasoning:active | ctx:100k",
            "compact": "Thinking 1s | main | model | mode:sup | r:on | ctx:100k",
            "minimal": "Thinking 1s | main | ctx:100k"
        });

        let patch = patch_renderer.render_frame_value(&adaptive_frame);
        let operation = patch["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .find(|operation| operation["regionId"] == "status")
            .expect("status operation");
        assert_eq!(
            operation["lineVariantPolicy"],
            "choose_shortest_fitting_variant"
        );
        assert_eq!(operation["viewportSelectableLines"], true);
        assert_eq!(
            operation["lineVariants"]["minimal"]["text"],
            "Thinking 1s | main | ctx:100k"
        );

        let plan = scheduler.render_patch_value(&patch);
        let write_op = plan["terminalOps"]
            .as_array()
            .expect("terminal ops")
            .iter()
            .find(|operation| operation["op"] == "write_line" && operation["regionId"] == "status")
            .expect("status write");
        assert_eq!(
            write_op["textVariants"]["compact"]["text"],
            "Thinking 1s | main | model | mode:sup | r:on | ctx:100k"
        );
        assert_eq!(
            write_op["textVariantPolicy"],
            "choose_shortest_fitting_variant"
        );
    }

    #[test]
    fn draw_executor_selects_minimal_line_variant_for_narrow_viewport() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut adaptive_frame = frame("frame-narrow-line-variant", vec!["status"]);
        adaptive_frame["regions"][0]["lines"] =
            json!(["FULL STATUS SHOULD NOT BE USED ON NARROW VIEWPORT"]);
        adaptive_frame["regions"][0]["lineVariants"] = json!({
            "full": "FULL STATUS SHOULD NOT BE USED ON NARROW VIEWPORT",
            "compact": "COMPACT STATUS ALSO TOO LONG",
            "minimal": "OK"
        });

        let patch = patch_renderer.render_frame_value(&adaptive_frame);
        let plan = scheduler.render_patch_value(&patch);
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 10))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert_eq!(report.viewport_width_profile, "minimal");
        assert_eq!(report.viewport_variant_line_count, 1);
        assert_eq!(report.viewport_variant_selection_count, 1);
        assert_eq!(report.viewport_variant_fallback_count, 0);
        assert!(ansi.contains("OK"));
        assert!(!ansi.contains("FULL STATUS"));
        assert!(!ansi.contains("COMPACT STATUS"));
    }

    #[test]
    fn draw_executor_clips_top_widgets_before_bottom_regions_on_short_viewports() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 9,
            "frameId": "render-frame-00000009",
            "frameHash": "frame-collision",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["status", "final_summary", "active", "footer"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "final_summary",
                    "role": "final_summary",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_final_summary",
                    "regionHash": "hash-final",
                    "lines": ["final-0", "final-1", "final-2", "final-3", "final-4", "final-5"]
                },
                {
                    "id": "active",
                    "role": "activity",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "replace_active",
                    "regionHash": "hash-active",
                    "lines": ["active-0", "active-1", "active-2", "active-3", "active-4", "active-5"]
                },
                {
                    "id": "footer",
                    "role": "footer",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "replace",
                    "regionHash": "hash-footer",
                    "lines": ["footer"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });

        let patch = patch_renderer.render_frame_value(&frame);
        let plan = scheduler.render_patch_value(&patch);
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(10, 40))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert_eq!(
            terminal_draw_reserved_bottom_rows(&plan, StreamJsonTerminalViewport::new(10, 40).rows),
            7
        );
        assert_eq!(plan["draw"]["topLayoutMode"], "dynamic_stack");
        assert_eq!(plan["draw"]["topLayoutCompactsGaps"], true);
        assert_eq!(plan["draw"]["topRegionCount"], 2);
        assert_eq!(plan["draw"]["topRegionLineCount"], 7);
        assert_eq!(
            plan["draw"]["topRegionOverflowPolicy"],
            "clip_before_bottom_regions"
        );
        assert_eq!(plan["draw"]["topRegionClipDiagnostics"], true);
        assert_eq!(report.reserved_bottom_rows, 7);
        assert_eq!(report.visible_top_row_budget, 3);
        assert!(report.clipped_row_count > 0);
        assert!(report.top_clipped_row_count > 0);
        assert!(ansi.contains("final-0"));
        assert!(ansi.contains("final-1"));
        assert!(!ansi.contains("final-2"));
        assert!(ansi.contains("active-0"));
        assert!(ansi.contains("footer"));
    }

    #[test]
    fn draw_executor_skips_non_flushable_plan_without_writing_bytes() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let _ = scheduler.render_patch_value(&first_patch);
        let duplicate_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let plan = scheduler.render_patch_value(&duplicate_patch);
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::default())
                .expect("draw plan bytes");

        assert!(bytes.is_empty());
        assert!(report.skipped);
        assert_eq!(report.skip_reason.as_deref(), Some("frame_hash_unchanged"));
        assert!(!report.flushed);
    }

    #[test]
    fn draw_scheduler_commits_transcript_region_to_scrollback_once() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut final_frame = frame("frame-final", vec!["transcript"]);
        final_frame["regions"] = json!([
            {
                "id": "transcript",
                "role": "transcript",
                "anchor": "scrollback",
                "placement": "scrollback",
                "updateMode": "append_scrollback",
                "regionHash": "hash-transcript",
                "lines": ["assistant transcript", "hello from final answer"]
            }
        ]);
        final_frame["scroll"]["commitToScrollback"] = json!(true);
        final_frame["terminal"]["finished"] = json!(true);

        let patch = patch_renderer.render_frame_value(&final_frame);
        let plan = scheduler.render_patch_value(&patch);

        assert_eq!(patch["operations"][0]["updateMode"], "append_scrollback");
        assert_eq!(
            plan["draw"]["strategy"],
            "anchored_region_patch_with_scrollback_commit"
        );
        assert_eq!(plan["draw"]["commitsScrollback"], true);
        assert_eq!(plan["cursor"]["restoreAfterDraw"], false);
        let terminal_ops = plan["terminalOps"].as_array().expect("terminal ops");
        assert_eq!(terminal_ops.len(), 1);
        assert_eq!(terminal_ops[0]["op"], "append_scrollback_block");
        assert_eq!(terminal_ops[0]["lines"][1], "hello from final answer");

        let duplicate_patch = patch_renderer.render_frame_value(&final_frame);
        let duplicate_plan = scheduler.render_patch_value(&duplicate_patch);
        assert_eq!(duplicate_plan["draw"]["skipped"], true);
        assert_eq!(
            duplicate_plan["terminalOps"]
                .as_array()
                .expect("duplicate terminal ops")
                .len(),
            0
        );

        let forced_patch =
            patch_renderer.render_frame_value_forced(&final_frame, "viewport_resize");
        let forced_plan = scheduler.render_patch_value(&forced_patch);
        assert_eq!(forced_patch["draw"]["forcedRedraw"], true);
        assert_eq!(forced_patch["draw"]["scrollbackAppendOnceSuppressed"], true);
        assert_eq!(forced_plan["draw"]["scrollbackAppendOnceSuppressed"], true);
        assert_eq!(
            forced_plan["terminalOps"]
                .as_array()
                .expect("forced terminal ops")
                .len(),
            0
        );
    }

    #[test]
    fn draw_runtime_holds_noncritical_scrollback_commit_while_manual_scroll_is_active() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();
        let mut final_frame = frame("frame-final-manual-scroll", vec!["transcript"]);
        final_frame["sequence"] = json!(44);
        final_frame["regions"] = json!([
            {
                "id": "transcript",
                "role": "transcript",
                "anchor": "scrollback",
                "placement": "scrollback",
                "updateMode": "append_scrollback",
                "regionHash": "hash-transcript-manual",
                "lines": ["assistant transcript", "held final answer while reading history"]
            }
        ]);
        final_frame["scroll"]["commitToScrollback"] = json!(true);
        final_frame["terminal"]["finished"] = json!(true);

        let patch = patch_renderer.render_frame_value(&final_frame);
        let draw_plan = scheduler.render_patch_value(&patch);

        assert_eq!(
            patch["scroll"]["manualScrollPolicy"],
            "hold_noncritical_scrollback_commit"
        );
        assert_eq!(patch["scroll"]["manualScrollBypass"], false);
        assert_eq!(patch["scroll"]["commitToScrollback"], true);
        assert_eq!(draw_plan["draw"]["commitsScrollback"], true);
        assert!(draw_plan_preserves_manual_scroll(&draw_plan));
        assert!(!draw_plan_requires_manual_scroll_bypass(&draw_plan));

        runtime.set_manual_scroll_active(true);
        let held = runtime
            .submit_draw_plan_at(&draw_plan, 100, &mut bytes)
            .expect("manual scroll holds transcript commit");
        assert!(held.queued);
        assert_eq!(held.skip_reason.as_deref(), Some("manual_scroll_preserved"));
        assert!(bytes.is_empty());

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(150, &mut bytes)
            .expect("released transcript commit");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("held final answer while reading history"));
    }

    #[test]
    fn draw_runtime_holds_final_summary_completion_update_while_manual_scroll_is_active() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();
        let summary_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 45,
            "frameId": "render-frame-00000045",
            "frameHash": "frame-final-summary-manual-scroll",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["final_summary"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "final_summary",
                    "role": "final_summary",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_final_summary",
                    "regionHash": "hash-final-summary-manual",
                    "lines": ["final summary", "completion should wait for scroll end"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": true,
            },
        });

        let patch = patch_renderer.render_frame_value(&summary_frame);
        let draw_plan = scheduler.render_patch_value(&patch);

        assert_eq!(
            patch["scroll"]["manualScrollPolicy"],
            "hold_noncritical_completion_update"
        );
        assert_eq!(patch["scroll"]["manualScrollBypass"], false);
        assert!(draw_plan_preserves_manual_scroll(&draw_plan));
        assert!(!draw_plan_requires_manual_scroll_bypass(&draw_plan));

        runtime.set_manual_scroll_active(true);
        let held = runtime
            .submit_draw_plan_at(&draw_plan, 100, &mut bytes)
            .expect("manual scroll holds final summary");
        assert!(held.queued);
        assert_eq!(held.skip_reason.as_deref(), Some("manual_scroll_preserved"));
        assert!(bytes.is_empty());

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(150, &mut bytes)
            .expect("released final summary");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("completion should wait for scroll end"));
    }

    #[test]
    fn draw_executor_appends_scrollback_block_without_fullscreen_clear() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut final_frame = frame("frame-final", vec!["transcript"]);
        final_frame["regions"] = json!([
            {
                "id": "transcript",
                "role": "transcript",
                "anchor": "scrollback",
                "placement": "scrollback",
                "updateMode": "append_scrollback",
                "regionHash": "hash-transcript",
                "lines": ["assistant transcript", "1234567890ABCDEFGHIJ"]
            }
        ]);
        final_frame["terminal"]["finished"] = json!(true);

        let patch = patch_renderer.render_frame_value(&final_frame);
        let plan = scheduler.render_patch_value(&patch);
        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&plan, StreamJsonTerminalViewport::new(12, 12))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert_eq!(report.scrollback_append_count, 1);
        assert_eq!(report.scrollback_line_count, 2);
        assert_eq!(report.scrollback_wrapped_line_count, 2);
        assert_eq!(report.bounded_line_count, 0);
        assert!(ansi.contains("assistant"));
        assert!(ansi.contains("1234567890AB"));
        assert!(ansi.contains("CDEFGHIJ"));
        assert!(!ansi.contains("..."));
        assert!(ansi.contains("\r\n"));
        assert!(!ansi.contains("\u{1b}[2J"));
        assert!(!ansi.contains("\u{1b}[3J"));
    }

    #[test]
    fn draw_executor_caps_scrollback_physical_lines_before_terminal_writes() {
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "terminalOps": [
                {
                    "op": "append_scrollback_block",
                    "regionId": "transcript",
                    "clearVisibleBottomRows": 0,
                    "wrapLongLines": true,
                    "wrapMode": "soft_viewport_columns",
                    "physicalLineBudget": {
                        "bounded": true,
                        "policy": "cap_scrollback_physical_lines_before_terminal_writes",
                        "maxPhysicalLines": 3
                    },
                    "lines": [
                        "abcdefghijkl",
                        "mnopqrstuvwx"
                    ]
                }
            ],
        });

        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 4))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert_eq!(report.scrollback_append_count, 1);
        assert_eq!(report.scrollback_line_count, 2);
        assert_eq!(report.written_line_count, 3);
        assert!(report.scrollback_physical_line_budgeted);
        assert_eq!(report.scrollback_physical_line_budget_max, 3);
        assert!(report.scrollback_physical_line_budget_exceeded);
        assert_eq!(
            report.scrollback_physical_line_budget_omitted_source_line_count,
            1
        );
        assert!(ansi.contains("abcd"));
        assert!(ansi.contains("efgh"));
        assert!(ansi.contains("ijkl"));
        assert!(!ansi.contains("mnop"));
    }

    #[test]
    fn draw_executor_caps_scrollback_soft_wrap_materialization_before_allocation() {
        let long_line = "x".repeat(STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES + 5);
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "terminalOps": [
                {
                    "op": "append_scrollback_block",
                    "regionId": "transcript",
                    "clearVisibleBottomRows": 0,
                    "wrapLongLines": true,
                    "wrapMode": "soft_viewport_columns",
                    "physicalLineBudget": {
                        "bounded": true,
                        "policy": "cap_scrollback_physical_lines_before_terminal_writes",
                        "maxPhysicalLines": 3
                    },
                    "lines": [
                        long_line
                    ]
                }
            ],
        });

        let (_, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 1))
                .expect("draw plan bytes");

        assert!(!report.skipped);
        assert_eq!(report.scrollback_append_count, 1);
        assert_eq!(report.scrollback_line_count, 1);
        assert_eq!(report.written_line_count, 3);
        assert!(report.scrollback_physical_line_budgeted);
        assert_eq!(report.scrollback_physical_line_budget_max, 3);
        assert!(report.scrollback_physical_line_budget_exceeded);
        assert_eq!(
            report.scrollback_physical_line_budget_omitted_source_line_count,
            0
        );
        assert_eq!(
            report.scrollback_physical_line_budget_omitted_wrapped_line_count,
            1
        );
    }

    #[test]
    fn draw_executor_caps_scrollback_clear_visible_rows_before_terminal_writes() {
        let requested_clear_rows = STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS + 5;
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "terminalOps": [
                {
                    "op": "append_scrollback_block",
                    "regionId": "transcript",
                    "clearVisibleBottomRows": requested_clear_rows,
                    "clearVisibleRowsBudget": {
                        "bounded": true,
                        "policy": "cap_scrollback_clear_visible_rows_before_terminal_writes",
                        "maxRows": STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS
                    },
                    "wrapLongLines": true,
                    "wrapMode": "soft_viewport_columns",
                    "lines": []
                }
            ],
        });

        let (_, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(40, 12))
                .expect("draw plan bytes");

        assert!(!report.skipped);
        assert_eq!(report.scrollback_append_count, 1);
        assert!(report.scrollback_clear_visible_rows_budgeted);
        assert_eq!(
            report.scrollback_clear_visible_rows_budget_max,
            STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS
        );
        assert!(report.scrollback_clear_visible_rows_budget_exceeded);
        assert_eq!(report.scrollback_clear_visible_rows_budget_omitted_count, 5);
        assert_eq!(
            report.cleared_line_count,
            STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS
        );
    }

    #[test]
    fn draw_executor_hard_caps_external_scrollback_budget_declarations() {
        let requested_clear_rows = STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS + 5;
        let lines = (0..(STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES + 5))
            .map(|index| json!(format!("line-{index}")))
            .collect::<Vec<_>>();
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "terminalOps": [
                {
                    "op": "append_scrollback_block",
                    "regionId": "transcript",
                    "clearVisibleBottomRows": requested_clear_rows,
                    "clearVisibleRowsBudget": {
                        "bounded": true,
                        "policy": "cap_scrollback_clear_visible_rows_before_terminal_writes",
                        "maxRows": u64::MAX
                    },
                    "wrapLongLines": true,
                    "wrapMode": "soft_viewport_columns",
                    "physicalLineBudget": {
                        "bounded": true,
                        "policy": "cap_scrollback_physical_lines_before_terminal_writes",
                        "maxPhysicalLines": u64::MAX
                    },
                    "lines": lines
                }
            ],
        });

        let (_, report) = render_draw_plan_to_terminal_bytes(
            &draw_plan,
            StreamJsonTerminalViewport::new(40, 120),
        )
        .expect("draw plan bytes");

        assert!(!report.skipped);
        assert_eq!(report.scrollback_append_count, 1);
        assert_eq!(
            report.scrollback_clear_visible_rows_budget_max,
            STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS
        );
        assert!(report.scrollback_clear_visible_rows_budget_exceeded);
        assert_eq!(report.scrollback_clear_visible_rows_budget_omitted_count, 5);
        assert_eq!(
            report.scrollback_physical_line_budget_max,
            STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES
        );
        assert!(report.scrollback_physical_line_budget_exceeded);
        assert_eq!(
            report.scrollback_physical_line_budget_omitted_source_line_count,
            5
        );
        assert_eq!(
            report.written_line_count,
            STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES
        );
    }

    #[test]
    fn draw_executor_caps_terminal_text_bytes_before_terminal_writes() {
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "safety": {
                "terminalTextByteBudget": {
                    "bounded": true,
                    "policy": "cap_terminal_text_bytes_before_terminal_writes",
                    "maxBytes": 5
                }
            },
            "terminalOps": [
                {
                    "op": "append_scrollback_block",
                    "regionId": "transcript",
                    "clearVisibleBottomRows": 0,
                    "wrapLongLines": true,
                    "wrapMode": "soft_viewport_columns",
                    "lines": [
                        "abcdef",
                        "ZZ"
                    ]
                }
            ],
        });

        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 20))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert!(report.terminal_text_byte_budgeted);
        assert_eq!(report.terminal_text_byte_budget_max, 5);
        assert_eq!(report.terminal_text_byte_budget_written_bytes, 5);
        assert!(report.terminal_text_byte_budget_exceeded);
        assert_eq!(report.terminal_text_byte_budget_truncated_write_count, 1);
        assert_eq!(report.terminal_text_byte_budget_omitted_write_count, 1);
        assert_eq!(report.written_line_count, 1);
        assert!(ansi.contains("abcde"));
        assert!(!ansi.contains("abcdef"));
        assert!(!ansi.contains("ZZ"));
    }

    #[test]
    fn draw_executor_hard_caps_external_text_byte_budget_declaration() {
        let long_line = "x".repeat(STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES + 16);
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "safety": {
                "terminalTextByteBudget": {
                    "bounded": true,
                    "policy": "cap_terminal_text_bytes_before_terminal_writes",
                    "maxBytes": u64::MAX
                }
            },
            "terminalOps": [
                {
                    "op": "append_scrollback_block",
                    "regionId": "transcript",
                    "clearVisibleBottomRows": 0,
                    "wrapLongLines": true,
                    "wrapMode": "soft_viewport_columns",
                    "lines": [
                        long_line
                    ]
                }
            ],
        });

        let (_, report) = render_draw_plan_to_terminal_bytes(
            &draw_plan,
            StreamJsonTerminalViewport::new(12, u16::MAX),
        )
        .expect("draw plan bytes");

        assert!(!report.skipped);
        assert!(report.terminal_text_byte_budgeted);
        assert_eq!(
            report.terminal_text_byte_budget_max,
            STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES
        );
        assert_eq!(
            report.terminal_text_byte_budget_written_bytes,
            STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES
        );
        assert!(report.terminal_text_byte_budget_exceeded);
        assert_eq!(report.terminal_text_byte_budget_truncated_write_count, 1);
    }

    #[test]
    fn draw_executor_caps_terminal_ops_before_execution() {
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "safety": {
                "terminalOpBudget": {
                    "bounded": true,
                    "policy": "cap_terminal_ops_before_execution",
                    "maxOps": 3
                }
            },
            "terminalOps": [
                {
                    "op": "save_cursor"
                },
                {
                    "op": "move_to_row",
                    "row": "top+0"
                },
                {
                    "op": "write_line",
                    "text": "visible"
                },
                {
                    "op": "write_line",
                    "text": "hidden"
                },
                {
                    "op": "restore_cursor"
                }
            ],
        });

        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 20))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(!report.skipped);
        assert_eq!(report.terminal_op_count, 5);
        assert!(report.terminal_op_budgeted);
        assert_eq!(report.terminal_op_budget_max, 3);
        assert!(report.terminal_op_budget_exceeded);
        assert_eq!(report.terminal_op_budget_omitted_count, 2);
        assert_eq!(report.executed_terminal_op_count, 4);
        assert_eq!(report.written_line_count, 1);
        assert!(ansi.contains("visible"));
        assert!(!ansi.contains("hidden"));
        assert!(report.saved_cursor);
        assert!(report.restored_cursor);
        assert_eq!(report.cursor_restore_fail_safe_count, 1);
    }

    #[test]
    fn draw_executor_hard_caps_external_terminal_op_budget_declaration() {
        let terminal_ops = (0..(STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS + 5))
            .map(|_| json!({"op": "external_unknown_op"}))
            .collect::<Vec<_>>();
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "safety": {
                "terminalOpBudget": {
                    "bounded": true,
                    "policy": "cap_terminal_ops_before_execution",
                    "maxOps": u64::MAX
                }
            },
            "terminalOps": terminal_ops,
        });

        let (_, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 20))
                .expect("draw plan bytes");

        assert!(!report.skipped);
        assert_eq!(
            report.terminal_op_count,
            STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS + 5
        );
        assert_eq!(
            report.terminal_op_budget_max,
            STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS
        );
        assert!(report.terminal_op_budget_exceeded);
        assert_eq!(report.terminal_op_budget_omitted_count, 5);
        assert_eq!(
            report.invalid_terminal_op_count,
            STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS
        );
    }

    #[test]
    fn draw_executor_fail_safe_restores_cursor_after_budget_truncation() {
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "safety": {
                "terminalOpBudget": {
                    "bounded": true,
                    "policy": "cap_terminal_ops_before_execution",
                    "maxOps": 2
                }
            },
            "terminalOps": [
                {
                    "op": "save_cursor"
                },
                {
                    "op": "move_to_row",
                    "row": "top+0"
                },
                {
                    "op": "restore_cursor"
                }
            ],
        });

        let (_, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 20))
                .expect("draw plan bytes");

        assert!(report.terminal_op_budget_exceeded);
        assert_eq!(report.terminal_op_budget_omitted_count, 1);
        assert!(report.saved_cursor);
        assert!(report.restored_cursor);
        assert_eq!(report.cursor_restore_fail_safe_count, 1);
        assert_eq!(report.executed_terminal_op_count, 3);
    }

    #[test]
    fn draw_executor_fail_safe_closes_synchronized_update_after_budget_truncation() {
        let draw_plan = json!({
            "draw": {
                "skipped": false,
            },
            "schedule": {
                "shouldFlush": true,
            },
            "safety": {
                "terminalOpBudget": {
                    "bounded": true,
                    "policy": "cap_terminal_ops_before_execution",
                    "maxOps": 2
                }
            },
            "terminalOps": [
                {
                    "op": "begin_batch"
                },
                {
                    "op": "move_to_row",
                    "row": "top+0"
                },
                {
                    "op": "end_batch"
                }
            ],
        });

        let (bytes, report) =
            render_draw_plan_to_terminal_bytes(&draw_plan, StreamJsonTerminalViewport::new(12, 20))
                .expect("draw plan bytes");
        let ansi = String::from_utf8(bytes).expect("ansi");

        assert!(report.terminal_op_budget_exceeded);
        assert_eq!(report.terminal_op_budget_omitted_count, 1);
        assert!(report.synchronized_update);
        assert_eq!(report.synchronized_update_fail_safe_count, 1);
        assert_eq!(report.executed_terminal_op_count, 3);
        assert!(ansi.contains("\u{1b}[?2026h"));
        assert!(ansi.contains("\u{1b}[?2026l"));
    }

    #[test]
    fn draw_executor_drops_superseded_sequence_on_reused_executor() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let plan = scheduler.render_patch_value(&patch);
        let mut executor =
            StreamJsonTerminalDrawExecutor::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first = executor
            .apply_draw_plan(&plan, &mut bytes)
            .expect("first draw");
        let len_after_first = bytes.len();
        let second = executor
            .apply_draw_plan(&plan, &mut bytes)
            .expect("second draw");

        assert!(!first.skipped);
        assert!(second.skipped);
        assert_eq!(second.skip_reason.as_deref(), Some("superseded_sequence"));
        assert_eq!(bytes.len(), len_after_first);
    }

    #[test]
    fn draw_runtime_coalesces_throttled_plans_until_deadline() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let first = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");
        assert!(first.applied);
        let len_after_first = bytes.len();

        let mut older_frame = frame("frame-b", vec!["active"]);
        older_frame["sequence"] = json!(8);
        older_frame["draw"]["dirty"] = json!(false);
        older_frame["regions"][1]["lines"] = json!(["queued older active"]);
        older_frame["refresh"]["throttleMs"] = json!(100);
        let older_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&older_frame));
        let older = runtime
            .submit_draw_plan_at(&older_plan, 20, &mut bytes)
            .expect("older queued");
        assert!(older.queued);
        assert_eq!(
            older.skip_reason.as_deref(),
            Some("coalesced_until_throttle_deadline")
        );
        assert_eq!(bytes.len(), len_after_first);

        let mut latest_frame = frame("frame-c", vec!["active"]);
        latest_frame["sequence"] = json!(9);
        latest_frame["draw"]["dirty"] = json!(false);
        latest_frame["regions"][1]["lines"] = json!(["latest active"]);
        latest_frame["refresh"]["throttleMs"] = json!(100);
        let latest_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&latest_frame));
        let latest = runtime
            .submit_draw_plan_at(&latest_plan, 40, &mut bytes)
            .expect("latest queued");
        assert!(latest.queued);
        assert_eq!(latest.dropped_pending_count, 1);

        let early = runtime
            .flush_pending_at(99, &mut bytes)
            .expect("early flush");
        assert!(early.queued);
        assert_eq!(
            early.skip_reason.as_deref(),
            Some("throttle_deadline_not_reached")
        );
        assert_eq!(bytes.len(), len_after_first);

        let due = runtime
            .flush_pending_at(100, &mut bytes)
            .expect("due flush");
        assert!(due.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("latest active"));
        assert!(!ansi.contains("queued older active"));
    }

    #[test]
    fn draw_runtime_snapshot_tracks_last_report_and_counters() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let first = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");
        assert!(first.applied);
        let first_snapshot = runtime.runtime_snapshot();
        assert_eq!(first_snapshot.runtime_report_count, 1);
        assert_eq!(first_snapshot.runtime_applied_report_count, 1);
        assert!(runtime.last_runtime_report().expect("last report").applied);

        let mut queued_frame = frame("frame-b", vec!["active"]);
        queued_frame["sequence"] = json!(8);
        queued_frame["draw"]["dirty"] = json!(false);
        queued_frame["regions"][1]["lines"] = json!(["queued active"]);
        queued_frame["refresh"]["throttleMs"] = json!(100);
        let queued_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&queued_frame));
        let queued = runtime
            .submit_draw_plan_at(&queued_plan, 20, &mut bytes)
            .expect("queued draw");
        assert!(queued.queued);
        let queued_snapshot = runtime.runtime_snapshot();
        assert_eq!(queued_snapshot.runtime_report_count, 2);
        assert_eq!(queued_snapshot.runtime_queued_report_count, 1);
        assert!(queued_snapshot.has_pending_draw);
        assert_eq!(
            queued_snapshot
                .last_runtime_report
                .as_ref()
                .and_then(|report| report.pending_sequence),
            Some(8)
        );

        let mut latest_frame = frame("frame-c", vec!["active"]);
        latest_frame["sequence"] = json!(9);
        latest_frame["draw"]["dirty"] = json!(false);
        latest_frame["regions"][1]["lines"] = json!(["latest active"]);
        latest_frame["refresh"]["throttleMs"] = json!(100);
        let latest_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&latest_frame));
        let latest = runtime
            .submit_draw_plan_at(&latest_plan, 40, &mut bytes)
            .expect("latest queued draw");
        assert!(latest.queued);
        assert_eq!(latest.dropped_pending_count, 1);
        assert_eq!(runtime.runtime_snapshot().runtime_dropped_pending_count, 1);

        let flushed = runtime
            .flush_pending_at(100, &mut bytes)
            .expect("flushed latest draw");
        assert!(flushed.applied);
        let flushed_snapshot = runtime.runtime_snapshot();
        assert_eq!(flushed_snapshot.runtime_report_count, 4);
        assert_eq!(flushed_snapshot.runtime_applied_report_count, 2);
        assert_eq!(flushed_snapshot.runtime_queued_report_count, 2);
        assert!(!flushed_snapshot.has_pending_draw);
        assert_eq!(
            flushed_snapshot
                .last_runtime_report
                .as_ref()
                .and_then(|report| report.execution.as_ref())
                .map(|execution| execution.flushed),
            Some(true)
        );
    }

    #[test]
    fn draw_runtime_diagnostics_value_serializes_last_report_summary() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");

        let mut queued_frame = frame("frame-b", vec!["active"]);
        queued_frame["sequence"] = json!(8);
        queued_frame["draw"]["dirty"] = json!(false);
        queued_frame["regions"][1]["lines"] = json!(["diagnostic queued active"]);
        queued_frame["refresh"]["throttleMs"] = json!(100);
        let queued_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&queued_frame));
        runtime
            .submit_draw_plan_at(&queued_plan, 20, &mut bytes)
            .expect("queued draw");

        let diagnostics = runtime.runtime_diagnostics_value();
        assert_eq!(diagnostics["hasPendingDraw"], true);
        assert_eq!(diagnostics["manualScrollActive"], false);
        assert_eq!(diagnostics["reportCount"], 2);
        assert_eq!(diagnostics["appliedReportCount"], 1);
        assert_eq!(diagnostics["queuedReportCount"], 1);
        assert_eq!(diagnostics["manualScrollPreservedReportCount"], 0);
        assert_eq!(diagnostics["lastReport"]["queued"], true);
        assert_eq!(
            diagnostics["lastReport"]["skipReason"],
            "coalesced_until_throttle_deadline"
        );
        assert_eq!(diagnostics["lastReport"]["pendingSequence"], 8);
        assert!(diagnostics["lastReport"]["execution"].is_null());

        runtime
            .flush_pending_at(100, &mut bytes)
            .expect("flushed queued draw");
        let flushed = runtime.runtime_diagnostics_value();
        assert_eq!(flushed["hasPendingDraw"], false);
        assert_eq!(flushed["lastReport"]["applied"], true);
        assert_eq!(flushed["lastReport"]["execution"]["flushed"], true);
        assert_eq!(
            flushed["lastReport"]["execution"]["viewportWidthProfile"],
            "compact"
        );
        assert!(
            flushed["lastReport"]["execution"]["executedTerminalOpCount"]
                .as_u64()
                .unwrap()
                > 0
        );
    }

    #[test]
    fn draw_runtime_diagnostics_soak_tracks_stream_resize_scroll_without_stuck_pending() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");

        runtime.set_manual_scroll_active(true);
        for index in 0..80usize {
            let mut stream_frame = frame(&format!("manual-scroll-held-{index}"), vec!["active"]);
            stream_frame["sequence"] = json!(100 + index);
            stream_frame["draw"]["dirty"] = json!(false);
            stream_frame["regions"][1]["lines"] =
                json!([format!("held stream chunk {index} {}", "x".repeat(80))]);
            stream_frame["refresh"]["throttleMs"] = json!(33);
            let plan =
                scheduler.render_patch_value(&patch_renderer.render_frame_value(&stream_frame));
            let report = runtime
                .submit_draw_plan_at(&plan, 10 + index as u64, &mut bytes)
                .expect("manual-scroll held draw");
            assert!(report.queued);
        }

        let held = runtime.runtime_diagnostics_value();
        assert_eq!(held["hasPendingDraw"], true);
        assert_eq!(held["manualScrollActive"], true);
        assert!(held["queuedReportCount"].as_u64().unwrap() >= 80);
        assert!(held["manualScrollPreservedReportCount"].as_u64().unwrap() >= 80);
        assert!(held["droppedPendingCount"].as_u64().unwrap() >= 79);
        assert_eq!(held["lastReport"]["skipReason"], "manual_scroll_preserved");

        runtime.set_viewport(StreamJsonTerminalViewport::new(12, 24));
        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(200, &mut bytes)
            .expect("release held pending draw");
        assert!(released.applied);
        let released_diag = runtime.runtime_diagnostics_value();
        assert_eq!(released_diag["hasPendingDraw"], false);
        assert_eq!(released_diag["manualScrollActive"], false);
        assert_eq!(
            released_diag["lastReport"]["execution"]["viewportColumns"],
            24
        );
        assert!(
            released_diag["lastReport"]["execution"]["boundedLineCount"]
                .as_u64()
                .unwrap()
                > 0
        );

        for index in 0..160usize {
            if index % 40 == 0 {
                let columns = if index % 80 == 0 { 120 } else { 32 };
                runtime.set_viewport(StreamJsonTerminalViewport::new(18, columns));
            }
            let mut stream_frame = frame(&format!("live-stream-{index}"), vec!["active"]);
            stream_frame["sequence"] = json!(1_000 + index);
            stream_frame["draw"]["dirty"] = json!(false);
            stream_frame["regions"][1]["lines"] =
                json!([format!("live stream chunk {index} {}", "y".repeat(72))]);
            stream_frame["refresh"]["throttleMs"] = json!(33);
            let plan =
                scheduler.render_patch_value(&patch_renderer.render_frame_value(&stream_frame));
            runtime
                .submit_draw_plan_at(&plan, 205 + index as u64, &mut bytes)
                .expect("live stream draw");
        }
        if runtime
            .runtime_diagnostics_value()
            .get("hasPendingDraw")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            runtime
                .flush_pending_at(500, &mut bytes)
                .expect("final pending flush");
        }

        let final_diag = runtime.runtime_diagnostics_value();
        assert_eq!(final_diag["hasPendingDraw"], false);
        assert_eq!(final_diag["manualScrollActive"], false);
        assert!(final_diag["reportCount"].as_u64().unwrap() > 200);
        assert!(final_diag["appliedReportCount"].as_u64().unwrap() > 2);
        assert!(final_diag["queuedReportCount"].as_u64().unwrap() > 80);
        assert!(
            final_diag["manualScrollPreservedReportCount"]
                .as_u64()
                .unwrap()
                >= 80
        );
        assert!(final_diag["droppedPendingCount"].as_u64().unwrap() > 100);
        assert_eq!(final_diag["lastReport"]["applied"], true);
        assert_eq!(final_diag["lastReport"]["execution"]["flushed"], true);
        assert_eq!(
            final_diag["lastReport"]["execution"]["terminalOpBudgetExceeded"],
            false
        );
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("held stream chunk 79"));
        assert!(ansi.contains("live stream chunk 159"));
        assert!(!ansi.contains("held stream chunk 0"));
    }

    #[test]
    fn draw_runtime_queues_owned_draw_plan_without_clone_path() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let first = runtime
            .submit_draw_plan_value_at(first_plan, 0, &mut bytes)
            .expect("first owned draw");
        assert!(first.applied);
        let len_after_first = bytes.len();

        let mut pending_frame = frame("frame-b", vec!["active"]);
        pending_frame["sequence"] = json!(8);
        pending_frame["draw"]["dirty"] = json!(false);
        pending_frame["regions"][1]["lines"] = json!(["owned queued active"]);
        pending_frame["refresh"]["throttleMs"] = json!(100);
        let pending_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&pending_frame));
        let queued = runtime
            .submit_draw_plan_value_at(pending_plan, 20, &mut bytes)
            .expect("owned queued");

        assert!(queued.queued);
        assert!(queued.queued_owned_draw_plan);
        assert!(!queued.queued_cloned_draw_plan);
        assert_eq!(queued.pending_sequence, Some(8));
        assert_eq!(bytes.len(), len_after_first);

        let flushed = runtime
            .flush_pending_at(100, &mut bytes)
            .expect("owned pending flush");
        assert!(flushed.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("owned queued active"));
    }

    #[test]
    fn draw_runtime_reports_borrowed_queue_clone_compatibility() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let first = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first borrowed draw");
        assert!(first.applied);
        let len_after_first = bytes.len();

        let mut pending_frame = frame("frame-b", vec!["active"]);
        pending_frame["sequence"] = json!(8);
        pending_frame["draw"]["dirty"] = json!(false);
        pending_frame["regions"][1]["lines"] = json!(["borrowed queued active"]);
        pending_frame["refresh"]["throttleMs"] = json!(100);
        let pending_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&pending_frame));
        let queued = runtime
            .submit_draw_plan_at(&pending_plan, 20, &mut bytes)
            .expect("borrowed queued");

        assert!(queued.queued);
        assert!(!queued.queued_owned_draw_plan);
        assert!(queued.queued_cloned_draw_plan);
        assert_eq!(queued.pending_sequence, Some(8));
        assert_eq!(bytes.len(), len_after_first);
    }

    #[test]
    fn draw_runtime_applies_pending_plan_with_latest_resize_viewport() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime =
            StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::new(24, 40));
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let _ = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");
        let len_after_first = bytes.len();

        let mut pending_frame = frame("frame-b", vec!["active"]);
        pending_frame["sequence"] = json!(8);
        pending_frame["draw"]["dirty"] = json!(false);
        pending_frame["regions"][1]["lines"] = json!(["1234567890ABCDEFGHIJ"]);
        pending_frame["refresh"]["throttleMs"] = json!(100);
        let pending_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&pending_frame));
        let queued = runtime
            .submit_draw_plan_at(&pending_plan, 20, &mut bytes)
            .expect("pending queued");
        assert!(queued.queued);

        runtime.set_viewport(StreamJsonTerminalViewport::new(12, 8));
        let due = runtime
            .flush_pending_at(100, &mut bytes)
            .expect("due flush");
        assert!(due.applied);
        assert_eq!(
            due.execution
                .as_ref()
                .expect("execution")
                .bounded_line_count,
            1
        );
        let appended = String::from_utf8(bytes[len_after_first..].to_vec()).expect("ansi");
        assert!(appended.contains("12345..."));
        assert!(!appended.contains("ABCDEFGHIJ"));
    }

    #[test]
    fn draw_runtime_holds_active_updates_while_manual_scroll_is_active() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let _ = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");
        let len_after_first = bytes.len();

        runtime.set_manual_scroll_active(true);
        let mut pending_frame = frame("frame-b", vec!["active"]);
        pending_frame["sequence"] = json!(8);
        pending_frame["draw"]["dirty"] = json!(false);
        pending_frame["regions"][1]["lines"] = json!(["held while scrolling"]);
        pending_frame["refresh"]["throttleMs"] = json!(100);
        let pending_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&pending_frame));
        let held = runtime
            .submit_draw_plan_at(&pending_plan, 500, &mut bytes)
            .expect("manual scroll held");
        assert!(held.queued);
        assert_eq!(held.skip_reason.as_deref(), Some("manual_scroll_preserved"));
        assert_eq!(bytes.len(), len_after_first);

        let still_held = runtime
            .flush_pending_at(600, &mut bytes)
            .expect("still held");
        assert!(still_held.queued);
        assert_eq!(
            still_held.skip_reason.as_deref(),
            Some("manual_scroll_preserved")
        );
        assert_eq!(bytes.len(), len_after_first);

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(600, &mut bytes)
            .expect("released flush");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("held while scrolling"));
    }

    #[test]
    fn draw_runtime_releases_manual_scroll_hold_for_terminal_teardown() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let _ = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");
        let len_after_first = bytes.len();

        runtime.set_manual_scroll_active(true);
        let mut pending_frame = frame("frame-b", vec!["active"]);
        pending_frame["sequence"] = json!(8);
        pending_frame["draw"]["dirty"] = json!(false);
        pending_frame["regions"][1]["lines"] = json!(["teardown-visible final update"]);
        pending_frame["refresh"]["throttleMs"] = json!(100);
        let pending_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&pending_frame));
        let held = runtime
            .submit_draw_plan_at(&pending_plan, 500, &mut bytes)
            .expect("manual scroll held");
        assert!(held.queued);
        assert_eq!(held.skip_reason.as_deref(), Some("manual_scroll_preserved"));
        assert_eq!(bytes.len(), len_after_first);

        let released_hold = runtime.release_manual_scroll_for_terminal_teardown();
        assert!(released_hold);
        assert_eq!(
            runtime
                .runtime_snapshot()
                .runtime_manual_scroll_teardown_release_count,
            1
        );
        let released = runtime
            .flush_pending_at(600, &mut bytes)
            .expect("teardown release flush");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("teardown-visible final update"));
        let diagnostics = runtime.runtime_diagnostics_value();
        assert_eq!(diagnostics["manualScrollTeardownReleaseCount"], 1);
    }

    #[test]
    fn draw_runtime_suppresses_stale_deadline_while_manual_scroll_holds_pending_draw() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let first_patch = patch_renderer.render_frame_value(&frame("frame-a", vec!["status"]));
        let first_plan = scheduler.render_patch_value(&first_patch);
        let _ = runtime
            .submit_draw_plan_at(&first_plan, 0, &mut bytes)
            .expect("first draw");
        let len_after_first = bytes.len();

        let mut pending_frame = frame("frame-b", vec!["active"]);
        pending_frame["sequence"] = json!(8);
        pending_frame["draw"]["dirty"] = json!(false);
        pending_frame["regions"][1]["lines"] = json!(["held past stale deadline"]);
        pending_frame["refresh"]["throttleMs"] = json!(100);
        let pending_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&pending_frame));
        let queued = runtime
            .submit_draw_plan_at(&pending_plan, 20, &mut bytes)
            .expect("pending queued");
        assert!(queued.queued);
        assert_eq!(queued.next_flush_due_ms, Some(100));

        runtime.set_manual_scroll_active(true);
        let held_at_deadline = runtime
            .flush_pending_at(100, &mut bytes)
            .expect("held at stale deadline");
        assert!(held_at_deadline.queued);
        assert_eq!(
            held_at_deadline.skip_reason.as_deref(),
            Some("manual_scroll_preserved")
        );
        assert_eq!(held_at_deadline.next_flush_due_ms, None);
        assert!(runtime.has_pending_draw());
        assert_eq!(bytes.len(), len_after_first);

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(150, &mut bytes)
            .expect("released flush");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("held past stale deadline"));
    }

    #[test]
    fn draw_runtime_bypasses_manual_scroll_hold_for_blocking_approval() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        runtime.set_manual_scroll_active(true);
        let approval_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 12,
            "frameId": "render-frame-00000012",
            "frameHash": "frame-approval-critical",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["approval"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "approval",
                    "role": "approval",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "replace_blocking",
                    "regionHash": "hash-approval-critical",
                    "lines": ["approval required", "tool: Bash", "blocking"]
                }
            ],
            "refresh": {
                "throttleMs": 100,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });

        let approval_plan =
            scheduler.render_patch_value(&patch_renderer.render_frame_value(&approval_frame));
        assert_eq!(approval_plan["draw"]["hasBlockingRegion"], true);
        assert!(!draw_plan_preserves_manual_scroll(&approval_plan));

        let report = runtime
            .submit_draw_plan_at(&approval_plan, 100, &mut bytes)
            .expect("approval draw");
        assert!(report.applied);
        assert!(!report.queued);
        assert!(!runtime.has_pending_draw());
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("approval required"));
    }

    #[test]
    fn draw_runtime_holds_noncritical_top_region_patch_while_manual_scroll_is_active() {
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();
        let patch = json!({
            "type": "render_patch",
            "schemaVersion": 1,
            "sequence": 21,
            "draw": {
                "skipped": false,
                "skipReason": null,
            },
            "operations": [
                {
                    "op": "replace_region",
                    "regionId": "slash_result",
                    "role": "slash_result",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_slash_result",
                    "lineCount": 1,
                    "topStartRow": 1,
                    "lines": ["slash result while scrolled"]
                }
            ],
            "flush": {
                "shouldFlush": true,
                "policy": "immediate",
                "coalesceSafe": false,
            },
            "cursor": {
                "preservePrompt": true,
                "restoreAfterDraw": true,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "preserveDuringManualScroll": true,
                "manualScrollPolicy": "hold_noncritical_top_region_update",
                "manualScrollBypass": false,
                "historyPolicy": "update_top_region",
            },
        });
        let draw_plan = scheduler.render_patch_value(&patch);

        assert_eq!(draw_plan["draw"]["hasBlockingRegion"], false);
        assert!(draw_plan_preserves_manual_scroll(&draw_plan));

        runtime.set_manual_scroll_active(true);
        let held = runtime
            .submit_draw_plan_at(&draw_plan, 100, &mut bytes)
            .expect("manual-scroll top region hold");
        assert!(held.queued);
        assert_eq!(held.skip_reason.as_deref(), Some("manual_scroll_preserved"));
        assert!(bytes.is_empty());

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(150, &mut bytes)
            .expect("released top region patch");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("slash result while scrolled"));
    }

    #[test]
    fn draw_runtime_bypasses_manual_scroll_hold_for_explicit_scroll_bypass() {
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let patch = json!({
            "type": "render_patch",
            "schemaVersion": 1,
            "sequence": 22,
            "draw": {
                "skipped": false,
                "skipReason": null,
            },
            "operations": [
                {
                    "op": "clear_region",
                    "regionId": "slash_result",
                    "role": "slash_result",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "clear_retired",
                    "lineCount": 0,
                    "lines": []
                }
            ],
            "flush": {
                "shouldFlush": true,
                "policy": "immediate",
                "coalesceSafe": false,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "preserveDuringManualScroll": false,
                "manualScrollPolicy": "bypass_for_lifecycle_clear",
                "manualScrollBypass": true,
                "historyPolicy": "clear_retired_region",
            },
        });
        let draw_plan = scheduler.render_patch_value(&patch);

        assert!(draw_plan_requires_manual_scroll_bypass(&draw_plan));
        assert!(!draw_plan_preserves_manual_scroll(&draw_plan));
    }

    #[test]
    fn frame_patch_holds_noncritical_widget_region_update_while_manual_scroll_active() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();
        let _ = patch_renderer
            .render_frame_value(&frame("frame-widget-baseline", vec!["status", "active"]));
        let command_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 31,
            "frameId": "render-frame-00000031",
            "frameHash": "frame-widget-command",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["command"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "command",
                    "role": "command",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_summary",
                    "regionHash": "hash-command",
                    "lines": ["cmd: cargo test", "exit: 0"]
                },
                {
                    "id": "active",
                    "role": "activity",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "patch",
                    "regionHash": "hash-active",
                    "lines": ["assistant text: 5 bytes"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });
        let patch = patch_renderer.render_frame_value(&command_frame);
        let draw_plan = scheduler.render_patch_value(&patch);

        assert_eq!(patch["scroll"]["preserveDuringManualScroll"], true);
        assert_eq!(
            patch["scroll"]["manualScrollPolicy"],
            "hold_noncritical_widget_region_update"
        );
        assert_eq!(patch["scroll"]["manualScrollBypass"], false);
        assert_eq!(patch["scroll"]["historyPolicy"], "update_widget_region");
        assert_eq!(
            patch["scroll"]["manualScrollPendingPolicy"],
            "replace_pending_with_latest"
        );
        assert!(draw_plan_preserves_manual_scroll(&draw_plan));

        runtime.set_manual_scroll_active(true);
        let held = runtime
            .submit_draw_plan_at(&draw_plan, 100, &mut bytes)
            .expect("manual-scroll widget hold");
        assert!(held.queued);
        assert_eq!(held.skip_reason.as_deref(), Some("manual_scroll_preserved"));
        assert!(bytes.is_empty());

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(150, &mut bytes)
            .expect("released widget patch");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("cmd: cargo test"));
    }

    #[test]
    fn frame_patch_bypasses_manual_scroll_hold_for_critical_error_region_update() {
        let mut patch_renderer = StreamJsonTerminalPatchRenderer::new();
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();
        let _ = patch_renderer
            .render_frame_value(&frame("frame-critical-baseline", vec!["status", "active"]));
        let error_frame = json!({
            "type": "render_frame",
            "schemaVersion": 1,
            "sequence": 32,
            "frameId": "render-frame-00000032",
            "frameHash": "frame-critical-error",
            "draw": {
                "dirty": true,
                "changedRegionIds": ["error"],
            },
            "changes": {
                "firstFrame": false,
            },
            "regions": [
                {
                    "id": "status",
                    "role": "status",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace",
                    "regionHash": "hash-status",
                    "lines": ["Thinking | main"]
                },
                {
                    "id": "error",
                    "role": "error",
                    "anchor": "top",
                    "placement": "top",
                    "updateMode": "replace_layered",
                    "regionHash": "hash-error",
                    "lines": ["error: render failed", "retry available"]
                },
                {
                    "id": "active",
                    "role": "activity",
                    "anchor": "bottom",
                    "placement": "bottom",
                    "updateMode": "patch",
                    "regionHash": "hash-active",
                    "lines": ["assistant text: 5 bytes"]
                }
            ],
            "refresh": {
                "throttleMs": 33,
            },
            "scroll": {
                "stable": true,
                "preserveOnActiveUpdate": true,
                "historyPolicy": "update_active",
            },
            "terminal": {
                "finished": false,
            },
        });
        let patch = patch_renderer.render_frame_value(&error_frame);
        let draw_plan = scheduler.render_patch_value(&patch);

        assert_eq!(patch["scroll"]["preserveDuringManualScroll"], false);
        assert_eq!(
            patch["scroll"]["manualScrollPolicy"],
            "bypass_for_critical_region_update"
        );
        assert_eq!(patch["scroll"]["manualScrollBypass"], true);
        assert_eq!(patch["scroll"]["historyPolicy"], "critical_region_update");
        assert_eq!(
            patch["scroll"]["manualScrollPendingPolicy"],
            "bypass_pending_hold"
        );
        assert!(draw_plan_requires_manual_scroll_bypass(&draw_plan));
        assert!(!draw_plan_preserves_manual_scroll(&draw_plan));

        runtime.set_manual_scroll_active(true);
        let applied = runtime
            .submit_draw_plan_at(&draw_plan, 100, &mut bytes)
            .expect("critical error bypass");
        assert!(applied.applied);
        assert!(!applied.queued);
        assert!(!runtime.has_pending_draw());
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("error: render failed"));
    }

    #[test]
    fn draw_runtime_replaces_manual_scroll_held_widget_patch_with_latest_sequence() {
        let mut scheduler = StreamJsonTerminalDrawScheduler::new();
        let mut runtime = StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::default());
        let mut bytes = Vec::new();

        let widget_patch = |sequence: u64, line: &str| {
            json!({
                "type": "render_patch",
                "schemaVersion": 1,
                "sequence": sequence,
                "draw": {
                    "skipped": false,
                    "skipReason": null,
                },
                "operations": [
                    {
                        "op": "replace_region",
                        "regionId": "command",
                        "role": "command",
                        "anchor": "top",
                        "placement": "top",
                        "updateMode": "replace_summary",
                        "lineCount": 1,
                        "topStartRow": 1,
                        "layoutMode": "dynamic_top_stack",
                        "lines": [line]
                    }
                ],
                "flush": {
                    "shouldFlush": true,
                    "policy": "immediate",
                    "coalesceSafe": false,
                },
                "cursor": {
                    "preservePrompt": true,
                    "restoreAfterDraw": true,
                },
                "scroll": {
                    "stable": true,
                    "preserveOnActiveUpdate": true,
                    "preserveDuringManualScroll": true,
                    "manualScrollPolicy": "hold_noncritical_widget_region_update",
                    "manualScrollPendingPolicy": "replace_pending_with_latest",
                    "manualScrollBypass": false,
                    "historyPolicy": "update_widget_region",
                },
            })
        };

        runtime.set_manual_scroll_active(true);
        let older_plan = scheduler.render_patch_value(&widget_patch(41, "older widget patch"));
        let older = runtime
            .submit_draw_plan_at(&older_plan, 100, &mut bytes)
            .expect("older widget hold");
        assert!(older.queued);
        assert_eq!(older.pending_sequence, Some(41));
        assert_eq!(older.dropped_pending_count, 0);

        let latest_plan = scheduler.render_patch_value(&widget_patch(42, "latest widget patch"));
        let latest = runtime
            .submit_draw_plan_at(&latest_plan, 110, &mut bytes)
            .expect("latest widget hold");
        assert!(latest.queued);
        assert_eq!(latest.pending_sequence, Some(42));
        assert_eq!(latest.dropped_pending_count, 1);
        assert!(bytes.is_empty());

        runtime.set_manual_scroll_active(false);
        let released = runtime
            .flush_pending_at(150, &mut bytes)
            .expect("released latest widget patch");
        assert!(released.applied);
        let ansi = String::from_utf8(bytes).expect("ansi");
        assert!(ansi.contains("latest widget patch"));
        assert!(!ansi.contains("older widget patch"));
    }
}
