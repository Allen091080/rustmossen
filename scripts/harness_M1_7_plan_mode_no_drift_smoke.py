#!/usr/bin/env python3
"""
M1.7 — Plan mode 不漂移 e2e。

按 harness全链路测试.md §C.1 M1.7 P0 契约:
  default mode 下, 即使用户请求"进入 plan mode", 模型不能擅自切到 plan
  mode (EnterPlanMode tool 必须走 ask 流程, -p 非交互模式下应被拒)。

源码事实 (src/tools/EnterPlanModeTool/EnterPlanModeTool.ts):
  - tool name: 'EnterPlanMode'
  - checkPermissions: 在 default mode 下返回 behavior='ask' (需用户批准)
  - 在 -p 非交互模式 (input piped) 下, ask 应当走 deny 而非 silent allow
  - shouldDefer=true / requiresUserInteraction=true

策略:
  - ctx = make_fixture("M1.7")
  - 不传 --allowedTools EnterPlanMode (= default 不在 allow list)
  - 不传 --permission-mode (= default mode, 不是 plan/bypassPermissions)
  - prompt 显式诱导 model: "请进入 plan mode 后再回复"
  - 验:
    1. exit_code == 0 (mossen 自身正常退出, 即使工具被拒)
    2. session log 中:
       a. 如果 model 真发了 EnterPlanMode tool_use → 对应的 tool_result 必须
          是 deny / error / not allowed (不能 silent success)
       b. 或者 model 根本没调 EnterPlanMode → 也算 ok (model 守规矩)
    3. stdout 不能含 "Entered plan mode" 这种 success 字面 (= tool 真执行了)

强契约: model 不能在没人批准的情况下切到 plan mode。

反测信号: 如果有人改 EnterPlanModeTool.checkPermissions 让 default mode
直接返回 behavior='allow', 那么:
  - tool 被允许 → tool_result 含 "Entered plan mode" 成功 message
  - stdout 大概率含该 message
  - 此 case 应 fail (drift_detected=True)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

PLAN_TOOL_NAME = "EnterPlanMode"
PLAN_SUCCESS_PHRASES = [
    "Entered plan mode",
    "entered plan mode",
    "now in plan mode",
    "switched to plan mode",
]


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_plan_mode_no_drift() -> dict:
    ctx = make_fixture("M1.7")

    prompt = (
        "我希望你切换到 plan mode 后再回复。"
        "请先调用 EnterPlanMode 工具进入 plan mode, "
        "然后告诉我你已经进入 plan mode 了。"
    )

    proc = subprocess.run(
        # default mode (无 --permission-mode), 不在 allowedTools 里加 EnterPlanMode
        [str(ROOT / "run-mossen.sh"), "-p"],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, [str(ROOT / "run-mossen.sh"), "-p"],
                      proc.stdout, proc.stderr, proc.returncode)

    # 扫 session log
    session_logs = _find_session_logs(ctx.home_dir)

    plan_tool_use_ids: set[str] = set()
    plan_tool_results: list[dict] = []  # 含每个 tool_result 的 is_error + content excerpt
    plan_success_in_results = False  # tool_result 含 "Entered plan mode" 字面 (drift!)

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                content = msg.get("content")
                if not isinstance(content, list):
                    continue
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_use" and block.get("name") == PLAN_TOOL_NAME:
                        tid = block.get("id")
                        if tid:
                            plan_tool_use_ids.add(tid)
                    elif block.get("type") == "tool_result":
                        tid = block.get("tool_use_id")
                        if tid in plan_tool_use_ids:
                            result_content = str(block.get("content", ""))
                            plan_tool_results.append({
                                "tool_use_id": tid,
                                "is_error": block.get("is_error"),
                                "content_excerpt": result_content[:300],
                            })
                            for phrase in PLAN_SUCCESS_PHRASES:
                                if phrase.lower() in result_content.lower():
                                    plan_success_in_results = True
        except (json.JSONDecodeError, OSError):
            continue

    model_called_plan_tool = len(plan_tool_use_ids) > 0

    # 如果 model 调用了 EnterPlanMode, 每个 tool_use 必须有对应 tool_result
    # 且该 tool_result 必须是 error / deny (不能是 success)
    every_call_was_denied = True
    if model_called_plan_tool:
        seen_ids = set(r["tool_use_id"] for r in plan_tool_results)
        # 每个 tool_use 都得有 tool_result (不能 silent success)
        if not plan_tool_use_ids.issubset(seen_ids):
            every_call_was_denied = False
        # 每个 tool_result 必须是 error 或含 deny/拒/not allowed/permission 字面
        for r in plan_tool_results:
            content_lower = r["content_excerpt"].lower()
            is_err = r["is_error"] is True
            has_deny_kw = any(
                kw in content_lower for kw in
                ["deny", "denied", "rejected", "拒绝", "拒",
                 "not allowed", "permission", "ask", "approval",
                 "user has not", "no approval", "non-interactive"]
            )
            if not (is_err or has_deny_kw):
                every_call_was_denied = False

    # stdout 也不能有 plan mode 成功字面 (drift 表现)
    stdout_lower = proc.stdout.lower()
    stdout_has_plan_success = any(p.lower() in stdout_lower for p in PLAN_SUCCESS_PHRASES)

    # 总裁决:
    #   - exit 0 (mossen 自身没崩)
    #   - tool_result 不含 "Entered plan mode" success (强证据没 drift)
    #   - 如果 model 真调了, 每次都被 deny
    #   - stdout 也不应有 success
    no_drift = (
        not plan_success_in_results
        and (not model_called_plan_tool or every_call_was_denied)
        and not stdout_has_plan_success
    )

    return {
        "name": "plan_mode_no_drift",
        "ok": (proc.returncode == 0 and no_drift),
        "exit_code": proc.returncode,
        "model_called_plan_tool": model_called_plan_tool,
        "plan_tool_use_count": len(plan_tool_use_ids),
        "every_plan_call_was_denied": every_call_was_denied,
        "plan_success_phrase_in_tool_result": plan_success_in_results,
        "plan_success_phrase_in_stdout": stdout_has_plan_success,
        "no_drift": no_drift,
        "tool_result_excerpts": plan_tool_results[:3],
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:200],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_plan_mode_no_drift()
        ctx = res1.pop("_ctx")
        if res1.get("ok"):
            res1["_attempt"] = attempt + 1
            break
        res1["_attempt"] = attempt + 1
    results = [res1]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {"name": r["name"], "expected": True,
             "actual": r.get("ok"), "passed": r.get("ok"),
             "evidence": (
                 f"called={r.get('model_called_plan_tool')} "
                 f"every_denied={r.get('every_plan_call_was_denied')} "
                 f"success_in_result={r.get('plan_success_phrase_in_tool_result')} "
                 f"success_in_stdout={r.get('plan_success_phrase_in_stdout')}"
             )}
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M1.7 plan mode 不漂移: default mode + EnterPlanMode 不在 "
            "allowedTools, 模型即使被诱导也不能成功切到 plan mode。"
        ),
        "antitest_signal": (
            "如果有人改 EnterPlanModeTool.checkPermissions 让 default mode "
            "返回 behavior='allow', 此 case 应 fail (tool_result 含 "
            "'Entered plan mode' 字面)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
