#!/usr/bin/env python3
"""
long_task_scenarios.py — P1-1 长任务不中断 6/7 场景压测骨架。

Sandbox 默认 SKIP 所有场景（每个场景需要 5-30 min 真实 LLM）。用户本机跑：

  MOSSEN_LONG_TASK_REAL=1 python3 scripts/long_task_scenarios.py
  MOSSEN_LONG_TASK_REAL=1 python3 scripts/long_task_scenarios.py --scenario s2

场景清单（来自 复盘报告 §3.14 / 7.1）：

  s1: 30+ 分钟真实开发任务（单 prompt，验主 agent loop 不中断）
  s2: 多子任务并发（3+ 个 SendMessage 同时跑）
  s3: 子任务中断后恢复（杀一个 agent，验证恢复路径）
  s4: 工具超时后继续（注入 60s+ 工具延迟）
  s5: 测试失败后自动修复（make test 失败 → 让 agent 修 → 通过）
  s6: 主任务结束时检查仍有 open/in-progress 子任务（防 UI 误导）
  s7 (可选): UI 状态正确性 — 主任务结束时不应显示"可输入"如果有子任务

本脚本只搭骨架：
- 提供 `run_scenario(name)` 路由
- 注册到 smoke_check.py 后默认 skip
- 用户本机跑时按场景独立报告 transcript / 时长 / 中断点
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_DIR = ROOT / "docs" / "long-task-evidence"


def _is_real_run() -> bool:
    return os.environ.get("MOSSEN_LONG_TASK_REAL", "").lower() in ("1", "true", "yes")


def scenario_s1_long_dev() -> dict:
    """30+ 分钟真实开发任务。

    设计：单 prompt 让 agent 写一个中等复杂度功能（~5 个文件改动），
    验主 loop 不在中途回到可输入状态。

    指标：
      - 完成时长（应接近 prompt 设定的实际工作量）
      - 子任务全部完成
      - 0 中途中断（用户没干预）
    """
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s1 真跑实现待用户本机：调用 mossen --print + 长 prompt")


def scenario_s2_concurrent_subtasks() -> dict:
    """多子任务并发。"""
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s2 真跑：让 leader spawn 3 个 SendMessage teammates")


def scenario_s3_subtask_recovery() -> dict:
    """子任务中断恢复。"""
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s3 真跑：spawn teammate → kill PID → 验证 leader 恢复")


def scenario_s4_tool_timeout() -> dict:
    """工具超时继续。"""
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s4 真跑：注入 sleep 60 的 Bash 调用，验证不卡死整 loop")


def scenario_s5_test_fail_recovery() -> dict:
    """测试失败自动修复。"""
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s5 真跑：插入故障 → 让 agent 跑 test → 修 → 复测")


def scenario_s6_open_subtasks_check() -> dict:
    """主任务结束时 open 子任务检查。"""
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s6 真跑：spawn long-running teammate → leader return → 验 leader 没标记已完成")


def scenario_s7_ui_state_correctness() -> dict:
    """UI 状态正确性（可选）。"""
    if not _is_real_run():
        return {"status": "skipped", "reason": "需要 MOSSEN_LONG_TASK_REAL=1"}
    raise NotImplementedError("s7 真跑：expect-1 抓状态栏，验证 leader 完成时若有 in-progress 子任务，UI 不应回到 prompt-ready")


SCENARIOS = {
    "s1": scenario_s1_long_dev,
    "s2": scenario_s2_concurrent_subtasks,
    "s3": scenario_s3_subtask_recovery,
    "s4": scenario_s4_tool_timeout,
    "s5": scenario_s5_test_fail_recovery,
    "s6": scenario_s6_open_subtasks_check,
    "s7": scenario_s7_ui_state_correctness,
}


def main() -> int:
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)

    selected = None
    if "--scenario" in sys.argv:
        idx = sys.argv.index("--scenario")
        if idx + 1 < len(sys.argv):
            selected = sys.argv[idx + 1]

    targets = [selected] if selected else list(SCENARIOS.keys())
    results = {}
    for name in targets:
        if name not in SCENARIOS:
            results[name] = {"status": "unknown_scenario"}
            continue
        try:
            results[name] = SCENARIOS[name]()
        except NotImplementedError as e:
            results[name] = {"status": "todo", "message": str(e)}
        except Exception as e:
            results[name] = {"status": "error", "message": str(e)}

    summary = {
        "real_run": _is_real_run(),
        "scenarios": results,
        "passed": sum(1 for r in results.values() if r.get("status") == "ok"),
        "skipped": sum(1 for r in results.values() if r.get("status") == "skipped"),
        "todo": sum(1 for r in results.values() if r.get("status") == "todo"),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    # In sandbox (skip mode): always exit 0
    # In real run: only fail on "error", skip and todo are OK
    failed = sum(1 for r in results.values() if r.get("status") == "error")
    return 1 if failed > 0 else 0


if __name__ == "__main__":
    raise SystemExit(main())
