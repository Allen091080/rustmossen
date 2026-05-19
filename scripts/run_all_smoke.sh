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
