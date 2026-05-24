#!/usr/bin/env bash
# run_all_smoke.sh — Wave 0~4 沉淀的 audit / smoke 统一 runner
# 用法:
#   ./scripts/run_all_smoke.sh             # 实跑全部 (typecheck/lint/wave2/wave0_perm/i18n/audit/layer/case39)
#   ./scripts/run_all_smoke.sh --dry-run   # 仅列要跑的步骤, 不实际执行
#   ./scripts/run_all_smoke.sh --help      # 显示帮助
#
# 覆盖范围:
#   - bun run typecheck:diff               (TS baseline diff, 0 new 校验)
#   - bun run lint:diff                    (lint baseline diff, 0 new 校验)
#   - scripts/wave2_*_smoke.py             (9 个 wave2 focused smoke)
#   - scripts/wave0_perm*_smoke.py         (2 个 wave0 perm smoke)
#   - scripts/i18n_self_check.py           (i18n 字典对称性 + 命名规范)
#   - scripts/i18n_runtime_smoke.py        (i18n runtime 验证)
#   - scripts/audit_hardcoded_user_text.py (硬编码扫描)
#   - scripts/wave4_r8_feature_flag_smoke.py (R8.2: bunfig MACRO + feature() 白名单 + resolve 一致性)
#   - scripts/stream_json_contract_smoke.py (CWB-3: stream-json 24+21+8+5 schema whitelist + 入口锚点 + only-additive 守卫)
#   - scripts/layer_boundary_audit.py      (Core/CLI/Workbench import 边界)
#   - smoke_check.py case 39 fingerprint  (custom_backend_auth_runtime_audit, 仅校验 fingerprint 稳定)
#
# 不覆盖 (按设计跳过):
#   - smoke_check.py 158 case 全跑         (耗时 10+ 分钟, 单跑用 `python3 scripts/smoke_check.py`)
#   - harness_M[N]_* / harness_R[N]_*     (e2e 测试, 单跑用对应 .py)
#   - LLM 真冒烟                          (需手动启动 mossen + 真模型, 不在本 runner)
#   - 任何 destructive 操作               (push / merge / tag / rebase / reset / stash)
#   - 任何 network 调用                   (除 case 39 内部隐含的 backend probe)
#
# 退出码:
#   0 — 全部通过
#   1 — 任一步骤 fail (列出 fail 的步骤名)
#   2 — case 39 fingerprint 漂移 (永久红线)

set -uo pipefail

DRY_RUN=0
EXPECTED_FINGERPRINT="870f99ed494d3d145ed2eb1368132299"

usage() {
  cat <<'USAGE'
run_all_smoke.sh — Wave 0~4 沉淀的 audit / smoke 统一 runner

用法:
  ./scripts/run_all_smoke.sh             实跑全部
  ./scripts/run_all_smoke.sh --dry-run   仅列步骤
  ./scripts/run_all_smoke.sh --help      显示本帮助

覆盖: typecheck:diff / lint:diff / 9 wave2 smoke / 2 wave0_perm smoke /
      i18n_self_check / i18n_runtime_smoke / audit_hardcoded_user_text /
      wave4 feature flag audit / stream-json contract audit /
      layer boundary audit / case 39 fingerprint 稳定性

不覆盖: smoke_check.py 158 case 全跑 / harness M+R 系列 e2e / LLM 真冒烟 /
        任何 destructive 操作 / 任何 network (除 case 39 backend probe)

详细说明: docs/design/bun-feature-flag-system.md (相关 feature flag 体系)
         docs/reference/layer-boundary-rules.md (Core/CLI/Workbench 边界)
         docs/reference/red-lines.md (case 39 fingerprint 红线)
USAGE
}

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown arg: $arg" >&2; usage; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FAILED=()

step() {
  local name="$1"
  shift
  if [[ "$DRY_RUN" == "1" ]]; then
    printf "  [DRY] %-50s %s\n" "$name" "$*"
    return 0
  fi
  printf "===%s===\n" "$name"
  if "$@"; then
    printf "  [PASS] %s\n" "$name"
    return 0
  else
    printf "  [FAIL] %s\n" "$name"
    FAILED+=("$name")
    return 1
  fi
}

step_glob() {
  local label="$1"
  local glob="$2"
  local count=0
  if [[ "$DRY_RUN" == "1" ]]; then
    printf "  [DRY] %s\n" "$label"
    for f in $glob; do
      [[ -e "$f" ]] || continue
      count=$((count + 1))
      printf "      ↳ python3 %s\n" "$f"
    done
    if [[ "$count" == "0" ]]; then
      printf "      (no matches for $glob)\n"
    fi
    return 0
  fi
  printf "===%s===\n" "$label"
  for f in $glob; do
    [[ -e "$f" ]] || continue
    local name
    name="$(basename "$f" .py)"
    if python3 "$f" >/tmp/_smoke_$$.out 2>&1; then
      printf "  [PASS] %s\n" "$name"
    else
      printf "  [FAIL] %s\n" "$name"
      FAILED+=("$name")
    fi
    count=$((count + 1))
  done
  rm -f /tmp/_smoke_$$.out
  if [[ "$count" == "0" ]]; then
    printf "  (no matches for $glob)\n"
    FAILED+=("$label (no matches)")
  fi
}

run_case39_fingerprint() {
  if [[ "$DRY_RUN" == "1" ]]; then
    printf "  [DRY] case 39 fingerprint check\n"
    printf "      ↳ python3 scripts/smoke_check.py --only custom_backend_auth_runtime_audit\n"
    printf "      ↳ extract JSON, strip status_stdout/stderr/text_output, md5 → expect %s\n" "$EXPECTED_FINGERPRINT"
    return 0
  fi
  printf "===case 39 fingerprint stability===\n"
  local fp
  fp="$(python3 - <<'PY'
import json, hashlib, subprocess, re, sys
proc = subprocess.run(
    ['python3', 'scripts/smoke_check.py', '--only', 'custom_backend_auth_runtime_audit'],
    capture_output=True, text=True, timeout=300, cwd='.'
)
out = proc.stderr + proc.stdout
m = re.search(r'smoke_check failed:\s*(\{.*?\})\s*Traceback', out, re.DOTALL)
if not m:
    print("EXTRACT_FAILED", file=sys.stderr)
    sys.exit(2)
data = json.loads(m.group(1))
def strip(o):
    if isinstance(o, dict):
        return {k: strip(v) for k, v in o.items() if k not in ('status_stdout','status_stderr','text_output')}
    if isinstance(o, list):
        return [strip(x) for x in o]
    return o
canon = json.dumps(strip(data), sort_keys=True, ensure_ascii=False)
print(hashlib.md5(canon.encode()).hexdigest())
PY
)" || {
    printf "  [FAIL] case 39 fingerprint (could not compute, see stderr)\n"
    FAILED+=("case 39 fingerprint")
    return 1
  }
  if [[ "$fp" == "$EXPECTED_FINGERPRINT" ]]; then
    printf "  [PASS] case 39 fingerprint = %s (stable)\n" "$fp"
  else
    printf "  [FAIL] case 39 fingerprint DRIFT — got %s, expected %s\n" "$fp" "$EXPECTED_FINGERPRINT"
    FAILED+=("case 39 fingerprint DRIFT")
    return 2
  fi
}

if [[ "$DRY_RUN" == "1" ]]; then
  printf "DRY-RUN — listing steps without executing.\n\n"
fi

step "typecheck:diff" bun run typecheck:diff
step "brand_clean_mossen_smoke" python3 scripts/harness_brand_clean_mossen_smoke.py
step "lint:diff"      bun run lint:diff
step_glob "9 wave2 focused smoke"  "scripts/wave2_*_smoke.py"
step_glob "wave0 perm smoke"       "scripts/wave0_perm*_smoke.py"
step "i18n_self_check"        python3 scripts/i18n_self_check.py
step "i18n_runtime_smoke"     python3 scripts/i18n_runtime_smoke.py
step "audit_hardcoded_user_text" python3 scripts/audit_hardcoded_user_text.py
step "wave4_r8_feature_flag_smoke" python3 scripts/wave4_r8_feature_flag_smoke.py
step "stream_json_contract_smoke" python3 scripts/stream_json_contract_smoke.py
step "wave_w29b_slash_command_smoke" python3 scripts/wave_w29b_slash_command_smoke.py
step "wave_w42_capability_slash_wrappers_smoke" python3 scripts/wave_w42_capability_slash_wrappers_smoke.py
step "wave_w43_slash_capability_manifest_smoke" python3 scripts/wave_w43_slash_capability_manifest_smoke.py
step "wave_w44_cost_slash_smoke" python3 scripts/wave_w44_cost_slash_smoke.py
step "wave_w45_capability_protocol_matrix_smoke" python3 scripts/wave_w45_capability_protocol_matrix_smoke.py
step "wave_w46_high_value_protocols_smoke" python3 scripts/wave_w46_high_value_protocols_smoke.py
step "wave_w47_real_capability_operations_smoke" python3 scripts/wave_w47_real_capability_operations_smoke.py
step "wave_w48_compact_queued_protocol_smoke" python3 scripts/wave_w48_compact_queued_protocol_smoke.py
step "wave_w49_compact_lifecycle_event_smoke" python3 scripts/wave_w49_compact_lifecycle_event_smoke.py
step "wave_w50_team_memory_gate_smoke" python3 scripts/wave_w50_team_memory_gate_smoke.py
step "wave_w51_memory_diagnostics_smoke" python3 scripts/wave_w51_memory_diagnostics_smoke.py
step "wave_w52_named_plan_files_smoke" python3 scripts/wave_w52_named_plan_files_smoke.py
step "wave_w54_second_tier_low_risk_smoke" python3 scripts/wave_w54_second_tier_low_risk_smoke.py
step "wave_w55_plugin_prune_smoke" python3 scripts/wave_w55_plugin_prune_smoke.py
step "wave_w55_project_purge_smoke" python3 scripts/wave_w55_project_purge_smoke.py
step "wave_w56_readonly_visibility_smoke" python3 scripts/wave_w56_readonly_visibility_smoke.py
step "wave_w57_second_tier_closure_smoke" python3 scripts/wave_w57_second_tier_closure_smoke.py
step "wave_w60_skill_mcp_preinstall_smoke" python3 scripts/wave_w60_skill_mcp_preinstall_smoke.py
step "wave_w61_mcp_add_template_smoke" python3 scripts/wave_w61_mcp_add_template_smoke.py
step "wave_w62_plugin_sources_smoke" python3 scripts/wave_w62_plugin_sources_smoke.py
step "wave_w63_extension_paths_smoke" python3 scripts/wave_w63_extension_paths_smoke.py
step "wave_w64_skills_bundled_visibility_smoke" python3 scripts/wave_w64_skills_bundled_visibility_smoke.py
step "wave_w65_github_skill_install_smoke" python3 scripts/wave_w65_github_skill_install_smoke.py
step "wave_w66_mcp_template_i18n_smoke" python3 scripts/wave_w66_mcp_template_i18n_smoke.py
step "wave_w67_plugin_marketplace_add_plan_smoke" python3 scripts/wave_w67_plugin_marketplace_add_plan_smoke.py
step "wave_w68_mcp_status_smoke" python3 scripts/wave_w68_mcp_status_smoke.py
step "wave_w69_plugin_install_plan_smoke" python3 scripts/wave_w69_plugin_install_plan_smoke.py
step "wave_w70_remote_extension_install_smoke" python3 scripts/wave_w70_remote_extension_install_smoke.py
step "wave_w71_mcp_add_slash_smoke" python3 scripts/wave_w71_mcp_add_slash_smoke.py
step "wave_w72_team_memory_watcher_lifecycle_smoke" python3 scripts/wave_w72_team_memory_watcher_lifecycle_smoke.py
step "wave_w73_team_memory_write_notify_smoke" python3 scripts/wave_w73_team_memory_write_notify_smoke.py
step "wave_w74_team_memory_path_alignment_smoke" python3 scripts/wave_w74_team_memory_path_alignment_smoke.py
step "wave_w75_team_memory_secret_guard_smoke" python3 scripts/wave_w75_team_memory_secret_guard_smoke.py
step "wave_w76_team_memory_detector_alignment_smoke" python3 scripts/wave_w76_team_memory_detector_alignment_smoke.py
step "wave_w77_team_memory_prompt_runtime_smoke" python3 scripts/wave_w77_team_memory_prompt_runtime_smoke.py
step "wave_w78_permission_prompt_fail_closed_smoke" python3 scripts/wave_w78_permission_prompt_fail_closed_smoke.py
step "wave_w79_auto_compact_query_replay_smoke" python3 scripts/wave_w79_auto_compact_query_replay_smoke.py
step "wave_w80_render_stream_throttle_smoke" python3 scripts/wave_w80_render_stream_throttle_smoke.py
step "wave_w81_render_scroll_resize_smoke" python3 scripts/wave_w81_render_scroll_resize_smoke.py
step "wave_w82_render_stream_resize_scroll_smoke" python3 scripts/wave_w82_render_stream_resize_scroll_smoke.py
step "wave_w83_render_keyboard_focus_scroll_smoke" python3 scripts/wave_w83_render_keyboard_focus_scroll_smoke.py
step "wave_w84_render_scroll_noop_sticky_smoke" python3 scripts/wave_w84_render_scroll_noop_sticky_smoke.py
step "wave_w85_render_transcript_page_viewport_smoke" python3 scripts/wave_w85_render_transcript_page_viewport_smoke.py
step "wave_w86_render_scrollbar_overflow_guard_smoke" python3 scripts/wave_w86_render_scrollbar_overflow_guard_smoke.py
step "wave_w87_render_page_key_noop_dirty_smoke" python3 scripts/wave_w87_render_page_key_noop_dirty_smoke.py
step "wave_w88_render_focus_key_noop_dirty_smoke" python3 scripts/wave_w88_render_focus_key_noop_dirty_smoke.py
step "wave_w89_render_external_statusline_inflight_smoke" python3 scripts/wave_w89_render_external_statusline_inflight_smoke.py
step "wave_w90_render_focus_change_noop_dirty_smoke" python3 scripts/wave_w90_render_focus_change_noop_dirty_smoke.py
step "wave_w91_render_resize_noop_dirty_smoke" python3 scripts/wave_w91_render_resize_noop_dirty_smoke.py
step "wave_w92_render_plan_activity_progress_smoke" python3 scripts/wave_w92_render_plan_activity_progress_smoke.py
step "wave_w93_render_plan_timeline_progress_smoke" python3 scripts/wave_w93_render_plan_timeline_progress_smoke.py
step "wave_w94_render_plan_process_status_smoke" python3 scripts/wave_w94_render_plan_process_status_smoke.py
step "wave_w95_render_hot_path_cache_smoke" python3 scripts/wave_w95_render_hot_path_cache_smoke.py
step "wave_w96_render_generic_payload_redaction_smoke" python3 scripts/wave_w96_render_generic_payload_redaction_smoke.py
step "wave_w97_render_statusline_presets_smoke" python3 scripts/wave_w97_render_statusline_presets_smoke.py
step "wave_w98_render_arbitrary_payload_fuzz_smoke" python3 scripts/wave_w98_render_arbitrary_payload_fuzz_smoke.py
step "wave_w99_render_property_fuzz_matrix_smoke" python3 scripts/wave_w99_render_property_fuzz_matrix_smoke.py
step "wave_w100_render_streaming_soak_budget_smoke" python3 scripts/wave_w100_render_streaming_soak_budget_smoke.py
step "wave_w101_render_stream_no_visible_dirty_smoke" python3 scripts/wave_w101_render_stream_no_visible_dirty_smoke.py
step "wave_w102_render_debug_config_scroll_clamp_smoke" python3 scripts/wave_w102_render_debug_config_scroll_clamp_smoke.py
step "wave_w103_render_loop_draw_error_autosave_smoke" python3 scripts/wave_w103_render_loop_draw_error_autosave_smoke.py
step "wave_w104_render_pty_live_streaming_soak" python3 scripts/wave_w104_render_pty_live_streaming_soak.py
step "wave_w105_render_pty_resize_manual_scroll_soak" python3 scripts/wave_w105_render_pty_resize_manual_scroll_soak.py
step "wave_w106_render_pty_mouse_scroll_soak" python3 scripts/wave_w106_render_pty_mouse_scroll_soak.py
step "wave_w107_render_pty_long_matrix_soak" python3 scripts/wave_w107_render_pty_long_matrix_soak.py
step "wave_w108_render_root_command_summary_boundary_smoke" python3 scripts/wave_w108_render_root_command_summary_boundary_smoke.py
step "wave_w109_render_tool_preview_boundary_smoke" python3 scripts/wave_w109_render_tool_preview_boundary_smoke.py
step "wave_w110_render_permission_summary_boundary_smoke" python3 scripts/wave_w110_render_permission_summary_boundary_smoke.py
step "wave_w111_render_compact_plan_preview_boundary_smoke" python3 scripts/wave_w111_render_compact_plan_preview_boundary_smoke.py
step "wave_w112_render_assistant_content_boundary_smoke" python3 scripts/wave_w112_render_assistant_content_boundary_smoke.py
step "wave_w113_render_permission_mode_boundary_smoke" python3 scripts/wave_w113_render_permission_mode_boundary_smoke.py
step "wave_w114_render_tool_summary_boundary_smoke" python3 scripts/wave_w114_render_tool_summary_boundary_smoke.py
step "wave_w115_render_engine_notice_boundary_smoke" python3 scripts/wave_w115_render_engine_notice_boundary_smoke.py
step "wave_w116_render_progress_boundary_smoke" python3 scripts/wave_w116_render_progress_boundary_smoke.py
step "wave_w117_render_assistant_transcript_boundary_smoke" python3 scripts/wave_w117_render_assistant_transcript_boundary_smoke.py
step "wave_w118_render_basic_transcript_boundary_smoke" python3 scripts/wave_w118_render_basic_transcript_boundary_smoke.py
step "wave_w119_render_compact_modal_boundary_smoke" python3 scripts/wave_w119_render_compact_modal_boundary_smoke.py
step "wave_w120_permissions_slash_routing_smoke" python3 scripts/wave_w120_permissions_slash_routing_smoke.py
step "wave_w121_slash_catalog_alias_hint_smoke" python3 scripts/wave_w121_slash_catalog_alias_hint_smoke.py
step "wave_w122_permission_rule_gate_smoke" python3 scripts/wave_w122_permission_rule_gate_smoke.py
step "wave_w123_compact_custom_instructions_smoke" python3 scripts/wave_w123_compact_custom_instructions_smoke.py
step "wave_w124_auto_compact_boundary_metadata_smoke" python3 scripts/wave_w124_auto_compact_boundary_metadata_smoke.py
step "wave_w125_session_memory_compact_bridge_smoke" python3 scripts/wave_w125_session_memory_compact_bridge_smoke.py
	step "wave_w126_compact_lifecycle_boundary_cleanup_smoke" python3 scripts/wave_w126_compact_lifecycle_boundary_cleanup_smoke.py
	step "wave_w127_stream_json_compact_bridge_smoke" python3 scripts/wave_w127_stream_json_compact_bridge_smoke.py
	step "wave_w128_stream_json_runtime_bridge_smoke" python3 scripts/wave_w128_stream_json_runtime_bridge_smoke.py
	step "wave_w129_remote_io_structured_bridge_smoke" python3 scripts/wave_w129_remote_io_structured_bridge_smoke.py
	step "wave_w130_stream_json_slash_runtime_bridge_smoke" python3 scripts/wave_w130_stream_json_slash_runtime_bridge_smoke.py
	step "wave_w131_stream_json_permission_mode_bridge_smoke" python3 scripts/wave_w131_stream_json_permission_mode_bridge_smoke.py
	step "wave_w132_stream_json_runtime_status_snapshot_smoke" python3 scripts/wave_w132_stream_json_runtime_status_snapshot_smoke.py
	step "wave_w133_runtime_tool_permission_status_smoke" python3 scripts/wave_w133_runtime_tool_permission_status_smoke.py
	step "wave_w134_stream_json_render_event_bridge_smoke" python3 scripts/wave_w134_stream_json_render_event_bridge_smoke.py
	step "wave_w135_stream_json_render_event_ordering_smoke" python3 scripts/wave_w135_stream_json_render_event_ordering_smoke.py
	step "wave_w136_stream_json_render_snapshot_reducer_smoke" python3 scripts/wave_w136_stream_json_render_snapshot_reducer_smoke.py
		step "wave_w137_stream_json_terminal_frame_contract_smoke" python3 scripts/wave_w137_stream_json_terminal_frame_contract_smoke.py
		step "wave_w138_stream_json_render_frame_delta_smoke" python3 scripts/wave_w138_stream_json_render_frame_delta_smoke.py
		step "wave_w139_stream_json_terminal_patch_renderer_smoke" python3 scripts/wave_w139_stream_json_terminal_patch_renderer_smoke.py
		step "wave_w140_stream_json_terminal_draw_plan_smoke" python3 scripts/wave_w140_stream_json_terminal_draw_plan_smoke.py
		step "wave_w141_stream_json_terminal_draw_executor_smoke" python3 scripts/wave_w141_stream_json_terminal_draw_executor_smoke.py
		step "wave_w142_stream_json_terminal_draw_runtime_smoke" python3 scripts/wave_w142_stream_json_terminal_draw_runtime_smoke.py
		step "wave_w143_stream_json_terminal_frontend_smoke" python3 scripts/wave_w143_stream_json_terminal_frontend_smoke.py
		step "wave_w144_stream_json_terminal_frontend_pty_smoke" python3 scripts/wave_w144_stream_json_terminal_frontend_pty_smoke.py
		step "wave_w145_stream_json_terminal_log_isolation_smoke" python3 scripts/wave_w145_stream_json_terminal_log_isolation_smoke.py
			step "wave_w146_stream_json_terminal_scrollback_transcript_smoke" python3 scripts/wave_w146_stream_json_terminal_scrollback_transcript_smoke.py
			step "wave_w147_stream_json_terminal_approval_region_smoke" python3 scripts/wave_w147_stream_json_terminal_approval_region_smoke.py
			step "wave_w148_stream_json_terminal_command_diff_widgets_smoke" python3 scripts/wave_w148_stream_json_terminal_command_diff_widgets_smoke.py
				step "wave_w149_stream_json_terminal_error_final_widgets_smoke" python3 scripts/wave_w149_stream_json_terminal_error_final_widgets_smoke.py
				step "wave_w150_stream_json_terminal_viewport_collision_guard_smoke" python3 scripts/wave_w150_stream_json_terminal_viewport_collision_guard_smoke.py
				step "wave_w151_stream_json_terminal_retired_region_clear_smoke" python3 scripts/wave_w151_stream_json_terminal_retired_region_clear_smoke.py
				step "wave_w152_stream_json_terminal_manual_scroll_critical_bypass_smoke" python3 scripts/wave_w152_stream_json_terminal_manual_scroll_critical_bypass_smoke.py
				step "wave_w153_stream_json_terminal_scrollback_soft_wrap_smoke" python3 scripts/wave_w153_stream_json_terminal_scrollback_soft_wrap_smoke.py
				step "wave_w154_stream_json_terminal_dynamic_top_stack_smoke" python3 scripts/wave_w154_stream_json_terminal_dynamic_top_stack_smoke.py
				step "wave_w155_stream_json_terminal_frontend_event_pump_smoke" python3 scripts/wave_w155_stream_json_terminal_frontend_event_pump_smoke.py
				step "wave_w156_stream_json_terminal_semantic_colors_smoke" python3 scripts/wave_w156_stream_json_terminal_semantic_colors_smoke.py
				step "wave_w157_stream_json_terminal_command_diff_preview_smoke" python3 scripts/wave_w157_stream_json_terminal_command_diff_preview_smoke.py
				step "wave_w158_stream_json_terminal_widget_expansion_smoke" python3 scripts/wave_w158_stream_json_terminal_widget_expansion_smoke.py
				step "wave_w159_stream_json_terminal_interaction_footer_smoke" python3 scripts/wave_w159_stream_json_terminal_interaction_footer_smoke.py
				step "wave_w160_stream_json_terminal_approval_action_focus_smoke" python3 scripts/wave_w160_stream_json_terminal_approval_action_focus_smoke.py
				step "wave_w161_stream_json_terminal_approval_action_activation_smoke" python3 scripts/wave_w161_stream_json_terminal_approval_action_activation_smoke.py
				step "wave_w162_stream_json_terminal_approval_decision_bridge_smoke" python3 scripts/wave_w162_stream_json_terminal_approval_decision_bridge_smoke.py
				step "wave_w163_stream_json_terminal_approval_interactive_gate_smoke" python3 scripts/wave_w163_stream_json_terminal_approval_interactive_gate_smoke.py
				step "wave_w164_stream_json_terminal_approval_input_preview_smoke" python3 scripts/wave_w164_stream_json_terminal_approval_input_preview_smoke.py
				step "wave_w165_stream_json_terminal_approval_session_rule_smoke" python3 scripts/wave_w165_stream_json_terminal_approval_session_rule_smoke.py
				step "wave_w166_terminal_interactive_gate_scoped_session_rule_smoke" python3 scripts/wave_w166_terminal_interactive_gate_scoped_session_rule_smoke.py
				step "wave_w167_stream_json_terminal_approval_submitted_region_smoke" python3 scripts/wave_w167_stream_json_terminal_approval_submitted_region_smoke.py
				step "wave_w168_stream_json_terminal_edit_command_bridge_smoke" python3 scripts/wave_w168_stream_json_terminal_edit_command_bridge_smoke.py
				step "wave_w169_terminal_local_edit_command_input_smoke" python3 scripts/wave_w169_terminal_local_edit_command_input_smoke.py
				step "wave_w170_terminal_render_input_capture_smoke" python3 scripts/wave_w170_terminal_render_input_capture_smoke.py
				step "wave_w171_terminal_render_interrupt_smoke" python3 scripts/wave_w171_terminal_render_interrupt_smoke.py
				step "wave_w172_terminal_key_release_filter_smoke" python3 scripts/wave_w172_terminal_key_release_filter_smoke.py
					step "wave_w173_terminal_edit_command_paste_smoke" python3 scripts/wave_w173_terminal_edit_command_paste_smoke.py
					step "wave_w174_terminal_interrupt_tool_cancel_smoke" python3 scripts/wave_w174_terminal_interrupt_tool_cancel_smoke.py
					step "wave_w175_terminal_shell_process_group_cancel_smoke" python3 scripts/wave_w175_terminal_shell_process_group_cancel_smoke.py
					step "wave_w176_terminal_background_bash_task_lifecycle_smoke" python3 scripts/wave_w176_terminal_background_bash_task_lifecycle_smoke.py
					step "wave_w177_terminal_background_task_render_summary_smoke" python3 scripts/wave_w177_terminal_background_task_render_summary_smoke.py
					step "wave_w178_terminal_background_task_status_panel_smoke" python3 scripts/wave_w178_terminal_background_task_status_panel_smoke.py
					step "wave_w179_terminal_critical_region_priority_smoke" python3 scripts/wave_w179_terminal_critical_region_priority_smoke.py
					step "wave_w180_terminal_plan_status_panel_smoke" python3 scripts/wave_w180_terminal_plan_status_panel_smoke.py
					step "wave_w181_terminal_file_change_diff_split_smoke" python3 scripts/wave_w181_terminal_file_change_diff_split_smoke.py
					step "wave_w182_terminal_file_change_expansion_smoke" python3 scripts/wave_w182_terminal_file_change_expansion_smoke.py
					step "wave_w183_terminal_final_summary_change_context_smoke" python3 scripts/wave_w183_terminal_final_summary_change_context_smoke.py
					step "wave_w184_terminal_error_detail_expansion_smoke" python3 scripts/wave_w184_terminal_error_detail_expansion_smoke.py
					step "wave_w185_terminal_background_task_expansion_smoke" python3 scripts/wave_w185_terminal_background_task_expansion_smoke.py
					step "wave_w186_terminal_top_stack_clip_diagnostics_smoke" python3 scripts/wave_w186_terminal_top_stack_clip_diagnostics_smoke.py
					step "wave_w187_terminal_footer_hint_budget_smoke" python3 scripts/wave_w187_terminal_footer_hint_budget_smoke.py
					step "wave_w188_terminal_native_mouse_scroll_smoke" python3 scripts/wave_w188_terminal_native_mouse_scroll_smoke.py
						step "wave_w189_terminal_manual_scroll_deadline_suppression_smoke" python3 scripts/wave_w189_terminal_manual_scroll_deadline_suppression_smoke.py
						step "wave_w190_terminal_sync_update_fail_closed_smoke" python3 scripts/wave_w190_terminal_sync_update_fail_closed_smoke.py
						step "wave_w191_terminal_color_plain_fallback_smoke" python3 scripts/wave_w191_terminal_color_plain_fallback_smoke.py
						step "wave_w192_terminal_unicode_grapheme_guard_smoke" python3 scripts/wave_w192_terminal_unicode_grapheme_guard_smoke.py
						step "wave_w193_terminal_ascii_fallback_smoke" python3 scripts/wave_w193_terminal_ascii_fallback_smoke.py
						step "wave_w194_terminal_control_sequence_strip_smoke" python3 scripts/wave_w194_terminal_control_sequence_strip_smoke.py
						step "wave_w195_terminal_inline_control_normalization_smoke" python3 scripts/wave_w195_terminal_inline_control_normalization_smoke.py
						step "wave_w196_terminal_control_char_normalization_smoke" python3 scripts/wave_w196_terminal_control_char_normalization_smoke.py
						step "wave_w197_terminal_bidi_format_guard_smoke" python3 scripts/wave_w197_terminal_bidi_format_guard_smoke.py
						step "wave_w198_terminal_unified_diff_file_sections_smoke" python3 scripts/wave_w198_terminal_unified_diff_file_sections_smoke.py
						step "wave_w199_terminal_command_stream_tail_smoke" python3 scripts/wave_w199_terminal_command_stream_tail_smoke.py
						step "wave_w200_terminal_status_bar_metadata_smoke" python3 scripts/wave_w200_terminal_status_bar_metadata_smoke.py
						step "wave_w201_terminal_final_summary_verification_smoke" python3 scripts/wave_w201_terminal_final_summary_verification_smoke.py
						step "wave_w202_stream_json_plan_slash_command_smoke" python3 scripts/wave_w202_stream_json_plan_slash_command_smoke.py
							step "wave_w203_stream_json_readonly_slash_inventory_smoke" python3 scripts/wave_w203_stream_json_readonly_slash_inventory_smoke.py
							step "wave_w204_stream_json_model_slash_command_smoke" python3 scripts/wave_w204_stream_json_model_slash_command_smoke.py
							step "wave_w205_stream_json_mcp_slash_command_smoke" python3 scripts/wave_w205_stream_json_mcp_slash_command_smoke.py
							step "wave_w206_stream_json_clear_slash_command_smoke" python3 scripts/wave_w206_stream_json_clear_slash_command_smoke.py
							step "wave_w207_stream_json_permission_mode_choices_smoke" python3 scripts/wave_w207_stream_json_permission_mode_choices_smoke.py
							step "wave_w208_stream_json_diff_slash_command_smoke" python3 scripts/wave_w208_stream_json_diff_slash_command_smoke.py
							step "wave_w209_stream_json_approvals_slash_command_smoke" python3 scripts/wave_w209_stream_json_approvals_slash_command_smoke.py
							step "wave_w210_stream_json_context_slash_command_smoke" python3 scripts/wave_w210_stream_json_context_slash_command_smoke.py
							step "wave_w211_stream_json_config_slash_command_smoke" python3 scripts/wave_w211_stream_json_config_slash_command_smoke.py
							step "wave_w212_stream_json_doctor_slash_command_smoke" python3 scripts/wave_w212_stream_json_doctor_slash_command_smoke.py
							step "wave_w213_stream_json_ide_slash_command_smoke" python3 scripts/wave_w213_stream_json_ide_slash_command_smoke.py
							step "wave_w214_stream_json_profile_slash_command_smoke" python3 scripts/wave_w214_stream_json_profile_slash_command_smoke.py
							step "wave_w215_stream_json_init_slash_command_smoke" python3 scripts/wave_w215_stream_json_init_slash_command_smoke.py
							step "wave_w216_stream_json_auth_slash_command_smoke" python3 scripts/wave_w216_stream_json_auth_slash_command_smoke.py
							step "wave_w217_tool_path_skill_discovery_smoke" python3 scripts/wave_w217_tool_path_skill_discovery_smoke.py
							step "wave_w218_skill_invocation_command_tags_smoke" python3 scripts/wave_w218_skill_invocation_command_tags_smoke.py
							step "wave_w219_stream_json_compact_confirm_guard_smoke" python3 scripts/wave_w219_stream_json_compact_confirm_guard_smoke.py
							step "wave_w220_stream_json_compact_cancel_smoke" python3 scripts/wave_w220_stream_json_compact_cancel_smoke.py
							step "wave_w221_stream_json_permission_semantic_payload_smoke" python3 scripts/wave_w221_stream_json_permission_semantic_payload_smoke.py
							step "wave_w222_agent_permission_alias_parity_smoke" python3 scripts/wave_w222_agent_permission_alias_parity_smoke.py
							step "wave_w223_stream_json_permission_rule_commands_smoke" python3 scripts/wave_w223_stream_json_permission_rule_commands_smoke.py
							step "wave_w224_agent_session_permission_rules_smoke" python3 scripts/wave_w224_agent_session_permission_rules_smoke.py
							step "wave_w225_compact_safe_point_status_smoke" python3 scripts/wave_w225_compact_safe_point_status_smoke.py
							step "wave_w226_clear_safe_point_status_smoke" python3 scripts/wave_w226_clear_safe_point_status_smoke.py
							step "wave_w227_stream_json_slash_result_render_bridge_smoke" python3 scripts/wave_w227_stream_json_slash_result_render_bridge_smoke.py
							step "wave_w228_stream_json_slash_result_terminal_region_smoke" python3 scripts/wave_w228_stream_json_slash_result_terminal_region_smoke.py
							step "wave_w229_stream_json_slash_result_lifecycle_retirement_smoke" python3 scripts/wave_w229_stream_json_slash_result_lifecycle_retirement_smoke.py
							step "wave_w230_stream_json_slash_result_event_preview_smoke" python3 scripts/wave_w230_stream_json_slash_result_event_preview_smoke.py
							step "wave_w231_stream_json_slash_result_region_contract_smoke" python3 scripts/wave_w231_stream_json_slash_result_region_contract_smoke.py
								step "wave_w232_stream_json_slash_result_region_render_payload_smoke" python3 scripts/wave_w232_stream_json_slash_result_region_render_payload_smoke.py
								step "wave_w233_stream_json_slash_result_region_patch_payload_smoke" python3 scripts/wave_w233_stream_json_slash_result_region_patch_payload_smoke.py
								step "wave_w234_stream_json_slash_result_patch_idempotency_smoke" python3 scripts/wave_w234_stream_json_slash_result_patch_idempotency_smoke.py
								step "wave_w235_stream_json_slash_result_patch_line_safety_smoke" python3 scripts/wave_w235_stream_json_slash_result_patch_line_safety_smoke.py
								step "wave_w236_stream_json_slash_result_patch_top_stack_layout_smoke" python3 scripts/wave_w236_stream_json_slash_result_patch_top_stack_layout_smoke.py
								step "wave_w237_stream_json_slash_result_patch_manual_scroll_hold_smoke" python3 scripts/wave_w237_stream_json_slash_result_patch_manual_scroll_hold_smoke.py
								step "wave_w238_terminal_widget_patch_manual_scroll_policy_smoke" python3 scripts/wave_w238_terminal_widget_patch_manual_scroll_policy_smoke.py
								step "wave_w239_terminal_manual_scroll_pending_supersession_smoke" python3 scripts/wave_w239_terminal_manual_scroll_pending_supersession_smoke.py
								step "wave_w240_terminal_viewport_width_adaptation_smoke" python3 scripts/wave_w240_terminal_viewport_width_adaptation_smoke.py
								step "wave_w241_terminal_viewport_line_variant_selection_smoke" python3 scripts/wave_w241_terminal_viewport_line_variant_selection_smoke.py
								step "wave_w242_terminal_region_line_budget_smoke" python3 scripts/wave_w242_terminal_region_line_budget_smoke.py
								step "wave_w243_terminal_retired_region_clear_budget_smoke" python3 scripts/wave_w243_terminal_retired_region_clear_budget_smoke.py
								step "wave_w244_terminal_noncritical_top_line_budget_smoke" python3 scripts/wave_w244_terminal_noncritical_top_line_budget_smoke.py
								step "wave_w245_terminal_cumulative_top_line_budget_smoke" python3 scripts/wave_w245_terminal_cumulative_top_line_budget_smoke.py
								step "wave_w246_terminal_scrollback_physical_line_budget_smoke" python3 scripts/wave_w246_terminal_scrollback_physical_line_budget_smoke.py
									step "wave_w247_terminal_text_byte_budget_smoke" python3 scripts/wave_w247_terminal_text_byte_budget_smoke.py
									step "wave_w248_terminal_op_execution_budget_smoke" python3 scripts/wave_w248_terminal_op_execution_budget_smoke.py
									step "wave_w249_terminal_cursor_restore_failsafe_smoke" python3 scripts/wave_w249_terminal_cursor_restore_failsafe_smoke.py
									step "wave_w250_terminal_sync_update_budget_failsafe_smoke" python3 scripts/wave_w250_terminal_sync_update_budget_failsafe_smoke.py
									step "wave_w251_terminal_style_reset_failsafe_smoke" python3 scripts/wave_w251_terminal_style_reset_failsafe_smoke.py
									step "wave_w252_terminal_cursor_restore_write_error_smoke" python3 scripts/wave_w252_terminal_cursor_restore_write_error_smoke.py
									step "wave_w253_terminal_scrollback_clear_rows_budget_smoke" python3 scripts/wave_w253_terminal_scrollback_clear_rows_budget_smoke.py
									step "wave_w254_terminal_executor_budget_hard_caps_smoke" python3 scripts/wave_w254_terminal_executor_budget_hard_caps_smoke.py
									step "wave_w255_terminal_executor_zero_copy_budgeting_smoke" python3 scripts/wave_w255_terminal_executor_zero_copy_budgeting_smoke.py
										step "wave_w256_terminal_scrollback_soft_wrap_materialization_budget_smoke" python3 scripts/wave_w256_terminal_scrollback_soft_wrap_materialization_budget_smoke.py
										step "wave_w257_terminal_scrollback_soft_wrap_streaming_sanitizer_smoke" python3 scripts/wave_w257_terminal_scrollback_soft_wrap_streaming_sanitizer_smoke.py
										step "wave_w258_terminal_draw_plan_borrowed_patch_inputs_smoke" python3 scripts/wave_w258_terminal_draw_plan_borrowed_patch_inputs_smoke.py
										step "wave_w259_terminal_draw_runtime_owned_pending_submit_smoke" python3 scripts/wave_w259_terminal_draw_runtime_owned_pending_submit_smoke.py
										step "wave_w260_terminal_frontend_draw_plan_only_dispatch_smoke" python3 scripts/wave_w260_terminal_frontend_draw_plan_only_dispatch_smoke.py
										step "wave_w261_terminal_resize_forced_redraw_smoke" python3 scripts/wave_w261_terminal_resize_forced_redraw_smoke.py
										step "wave_w262_terminal_resize_burst_coalescing_smoke" python3 scripts/wave_w262_terminal_resize_burst_coalescing_smoke.py
										step "wave_w263_terminal_scroll_burst_coalescing_smoke" python3 scripts/wave_w263_terminal_scroll_burst_coalescing_smoke.py
										step "wave_w264_terminal_priority_frontend_events_smoke" python3 scripts/wave_w264_terminal_priority_frontend_events_smoke.py
										step "wave_w265_terminal_teardown_pending_flush_smoke" python3 scripts/wave_w265_terminal_teardown_pending_flush_smoke.py
										step "wave_w266_terminal_priority_fairness_budget_smoke" python3 scripts/wave_w266_terminal_priority_fairness_budget_smoke.py
										step "wave_w267_terminal_priority_resize_redraw_smoke" python3 scripts/wave_w267_terminal_priority_resize_redraw_smoke.py
										step "wave_w268_terminal_priority_scroll_end_flush_smoke" python3 scripts/wave_w268_terminal_priority_scroll_end_flush_smoke.py
										step "wave_w269_terminal_manual_scroll_latest_state_smoke" python3 scripts/wave_w269_terminal_manual_scroll_latest_state_smoke.py
										step "wave_w270_terminal_draw_runtime_report_snapshot_smoke" python3 scripts/wave_w270_terminal_draw_runtime_report_snapshot_smoke.py
										step "wave_w271_terminal_draw_runtime_diagnostics_json_smoke" python3 scripts/wave_w271_terminal_draw_runtime_diagnostics_json_smoke.py
										step "wave_w272_terminal_draw_runtime_diagnostics_soak_smoke" python3 scripts/wave_w272_terminal_draw_runtime_diagnostics_soak_smoke.py
										step "wave_w273_terminal_final_diagnostics_export_smoke" python3 scripts/wave_w273_terminal_final_diagnostics_export_smoke.py
										step "wave_w274_terminal_oneshot_diagnostics_pty_smoke" python3 scripts/wave_w274_terminal_oneshot_diagnostics_pty_smoke.py
										step "wave_w275_terminal_oneshot_manual_scroll_diagnostics_pty_smoke" python3 scripts/wave_w275_terminal_oneshot_manual_scroll_diagnostics_pty_smoke.py
											step "wave_w276_terminal_oneshot_resize_scroll_diagnostics_pty_smoke" python3 scripts/wave_w276_terminal_oneshot_resize_scroll_diagnostics_pty_smoke.py
												step "wave_w277_terminal_interrupt_diagnostics_pty_smoke" python3 scripts/wave_w277_terminal_interrupt_diagnostics_pty_smoke.py
													step "wave_w278_terminal_cleanup_balance_pty_contract_smoke" python3 scripts/wave_w278_terminal_cleanup_balance_pty_contract_smoke.py
													step "wave_w279_terminal_slow_first_token_heartbeat_pty_smoke" python3 scripts/wave_w279_terminal_slow_first_token_heartbeat_pty_smoke.py
													step "wave_w280_terminal_slow_first_token_interrupt_pty_smoke" python3 scripts/wave_w280_terminal_slow_first_token_interrupt_pty_smoke.py
													step "wave_w281_terminal_transcript_dedupe_pty_smoke" python3 scripts/wave_w281_terminal_transcript_dedupe_pty_smoke.py
													step "wave_w282_terminal_heartbeat_metadata_no_empty_activity_pty_smoke" python3 scripts/wave_w282_terminal_heartbeat_metadata_no_empty_activity_pty_smoke.py
													step "wave_w283_terminal_initial_model_seed_pty_smoke" python3 scripts/wave_w283_terminal_initial_model_seed_pty_smoke.py
													step "wave_w284_terminal_metadata_waiting_redraw_suppression_pty_smoke" python3 scripts/wave_w284_terminal_metadata_waiting_redraw_suppression_pty_smoke.py
														step "wave_w285_terminal_assistant_activity_text_first_pty_smoke" python3 scripts/wave_w285_terminal_assistant_activity_text_first_pty_smoke.py
														step "wave_w286_terminal_final_assistant_no_byte_flash_pty_smoke" python3 scripts/wave_w286_terminal_final_assistant_no_byte_flash_pty_smoke.py
														step "wave_w287_terminal_assistant_activity_stable_rows_pty_smoke" python3 scripts/wave_w287_terminal_assistant_activity_stable_rows_pty_smoke.py
														step "wave_w288_terminal_manual_scroll_completion_hold_smoke" python3 scripts/wave_w288_terminal_manual_scroll_completion_hold_smoke.py
														step "wave_w289_terminal_manual_scroll_tail_hold_pty_smoke" python3 scripts/wave_w289_terminal_manual_scroll_tail_hold_pty_smoke.py
														step "wave_w290_terminal_manual_scroll_teardown_release_pty_smoke" python3 scripts/wave_w290_terminal_manual_scroll_teardown_release_pty_smoke.py
														step "wave_w291_terminal_manual_scroll_resize_teardown_release_pty_smoke" python3 scripts/wave_w291_terminal_manual_scroll_resize_teardown_release_pty_smoke.py
														step "wave_w292_terminal_manual_scroll_resize_interrupt_pty_smoke" python3 scripts/wave_w292_terminal_manual_scroll_resize_interrupt_pty_smoke.py
														step "wave_w293_terminal_manual_scroll_approval_bypass_pty_smoke" python3 scripts/wave_w293_terminal_manual_scroll_approval_bypass_pty_smoke.py
														step "wave_w294_terminal_manual_scroll_approval_reject_pty_smoke" python3 scripts/wave_w294_terminal_manual_scroll_approval_reject_pty_smoke.py
														step "wave_w295_terminal_manual_scroll_approval_approve_pty_smoke" python3 scripts/wave_w295_terminal_manual_scroll_approval_approve_pty_smoke.py
														step "wave_w296_terminal_manual_scroll_approval_edit_command_pty_smoke" python3 scripts/wave_w296_terminal_manual_scroll_approval_edit_command_pty_smoke.py
														step "wave_w297_terminal_manual_scroll_approval_always_allow_pty_smoke" python3 scripts/wave_w297_terminal_manual_scroll_approval_always_allow_pty_smoke.py
														step "wave_w298_terminal_manual_scroll_approval_edit_cancel_pty_smoke" python3 scripts/wave_w298_terminal_manual_scroll_approval_edit_cancel_pty_smoke.py
														step "wave_w299_terminal_manual_scroll_approval_resize_approve_pty_smoke" python3 scripts/wave_w299_terminal_manual_scroll_approval_resize_approve_pty_smoke.py
														step "wave_w300_terminal_manual_scroll_approval_active_scroll_reject_pty_smoke" python3 scripts/wave_w300_terminal_manual_scroll_approval_active_scroll_reject_pty_smoke.py
														step "wave_w301_terminal_mouse_scroll_approval_reject_pty_smoke" python3 scripts/wave_w301_terminal_mouse_scroll_approval_reject_pty_smoke.py
														step "wave_w302_terminal_mouse_scroll_approval_approve_pty_smoke" python3 scripts/wave_w302_terminal_mouse_scroll_approval_approve_pty_smoke.py
														step "wave_w303_terminal_mouse_scroll_approval_edit_command_pty_smoke" python3 scripts/wave_w303_terminal_mouse_scroll_approval_edit_command_pty_smoke.py
														step "wave_w304_terminal_mouse_scroll_approval_always_allow_pty_smoke" python3 scripts/wave_w304_terminal_mouse_scroll_approval_always_allow_pty_smoke.py
														step "wave_w305_terminal_manual_scroll_command_output_after_approval_pty_smoke" python3 scripts/wave_w305_terminal_manual_scroll_command_output_after_approval_pty_smoke.py
														step "wave_w306_terminal_render_product_acceptance_gate_smoke" python3 scripts/wave_w306_terminal_render_product_acceptance_gate_smoke.py
														step "wave_w307_terminal_mouse_scroll_command_output_after_approval_pty_smoke" python3 scripts/wave_w307_terminal_mouse_scroll_command_output_after_approval_pty_smoke.py
														step "wave_w308_terminal_manual_scroll_command_output_resize_after_approval_pty_smoke" python3 scripts/wave_w308_terminal_manual_scroll_command_output_resize_after_approval_pty_smoke.py
														step "wave_w309_terminal_mouse_scroll_command_output_resize_after_approval_pty_smoke" python3 scripts/wave_w309_terminal_mouse_scroll_command_output_resize_after_approval_pty_smoke.py
														step "wave_w310_terminal_manual_scroll_command_interrupt_after_approval_pty_smoke" python3 scripts/wave_w310_terminal_manual_scroll_command_interrupt_after_approval_pty_smoke.py
														step "wave_w311_terminal_mouse_scroll_command_interrupt_after_approval_pty_smoke" python3 scripts/wave_w311_terminal_mouse_scroll_command_interrupt_after_approval_pty_smoke.py
														step "wave_w312_terminal_manual_scroll_command_resize_interrupt_after_approval_pty_smoke" python3 scripts/wave_w312_terminal_manual_scroll_command_resize_interrupt_after_approval_pty_smoke.py
														step "wave_w313_terminal_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke" python3 scripts/wave_w313_terminal_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke.py
														step "wave_w314_terminal_manual_scroll_command_live_tail_release_after_approval_pty_smoke" python3 scripts/wave_w314_terminal_manual_scroll_command_live_tail_release_after_approval_pty_smoke.py
														step "wave_w315_terminal_mouse_scroll_command_live_tail_release_after_approval_pty_smoke" python3 scripts/wave_w315_terminal_mouse_scroll_command_live_tail_release_after_approval_pty_smoke.py
														step "wave_w316_terminal_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke" python3 scripts/wave_w316_terminal_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke.py
														step "wave_w317_terminal_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke" python3 scripts/wave_w317_terminal_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke.py
															step "wave_w318_terminal_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke" python3 scripts/wave_w318_terminal_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke.py
																step "wave_w319_terminal_command_end_live_tail_matrix_after_approval_pty_smoke" python3 scripts/wave_w319_terminal_command_end_live_tail_matrix_after_approval_pty_smoke.py
																step "wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke" python3 scripts/wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke.py
																step "wave_w321_terminal_command_pagedown_live_tail_matrix_after_approval_pty_smoke" python3 scripts/wave_w321_terminal_command_pagedown_live_tail_matrix_after_approval_pty_smoke.py
																step "wave_w322_terminal_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke" python3 scripts/wave_w322_terminal_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke.py
																step "wave_w323_stream_json_slash_compact_permissions_controls_smoke" python3 scripts/wave_w323_stream_json_slash_compact_permissions_controls_smoke.py
																step "wave_w324_tui_composer_final_summary_noise_smoke" python3 scripts/wave_w324_tui_composer_final_summary_noise_smoke.py
																step "harness_M15_1_full_chain_rust_smoke" python3 scripts/harness_M15_1_full_chain_rust_smoke.py
																step "layer_boundary_audit" python3 scripts/layer_boundary_audit.py
run_case39_fingerprint

if [[ "$DRY_RUN" == "1" ]]; then
  printf "\nDRY-RUN done. No commands executed.\n"
  exit 0
fi

printf "\n===summary===\n"
if [[ "${#FAILED[@]}" == "0" ]]; then
  printf "  ALL PASS\n"
  exit 0
else
  printf "  %d FAIL:\n" "${#FAILED[@]}"
  for n in "${FAILED[@]}"; do
    printf "    - %s\n" "$n"
  done
  for n in "${FAILED[@]}"; do
    if [[ "$n" == *"DRIFT"* ]]; then
      exit 2
    fi
  done
  exit 1
fi
