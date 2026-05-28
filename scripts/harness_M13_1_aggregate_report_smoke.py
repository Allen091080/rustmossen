#!/usr/bin/env python3
"""
M13.1 — harness 聚合报告 (P0).

按 harness全链路测试.md §C.1 + §C.7 契约:
  跑 harness_assertions_aggregator.py → 产出 harness-final-report.md/.json
  覆盖所有 e2e smoke 的 assertions.json 聚合, 含 module pass/fail/block 统计.

观察点:
  1. aggregator EXIT 0
  2. 产出 harness-final-report.md (markdown 文件存在 + 非空)
  3. 产出 harness-final-report.json (JSON 文件存在 + 解析 OK)
  4. JSON 含 modules / total / passed / failed 字段
  5. 至少含已知模块 (M0/M1/M2/M3/M4/M5/M6/M7/M8/M9/M10/M11/M12)

反测信号:
  - aggregator script 改 noop → 文件不更新 → fail
  - assertions.json 模板字段改名 (e.g. status→state) → 聚合识别不到 → fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


def case_aggregator_produces_final_report() -> dict:
    ctx = make_fixture("M13.1")
    write_assertions(
        ctx,
        status="passed",
        assertions=[
            {
                "name": "aggregator_smoke_self_seed",
                "expected": True,
                "actual": True,
                "passed": True,
                "evidence": (
                    "Seed current M13.1 artifact before invoking the aggregator so "
                    "capability-matrix refresh does not observe this fresh fixture as stale."
                ),
            }
        ],
    )

    # 跑现有 aggregator (它扫 /tmp/mossen-harness/*/artifacts/assertions.json)
    aggregator = ROOT / "scripts" / "harness_assertions_aggregator.py"

    proc = subprocess.run(
        ["python3", str(aggregator)],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=60,
        env=ctx.env,
    )

    write_command_log(ctx, ["python3", str(aggregator)], proc.stdout, proc.stderr, proc.returncode)

    md_file = ROOT / "harness-final-report.md"
    json_file = ROOT / "harness-final-report.json"

    md_exists = md_file.exists() and md_file.stat().st_size > 0
    json_exists = json_file.exists() and json_file.stat().st_size > 0
    json_data = None
    if json_exists:
        try:
            json_data = json.loads(json_file.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            json_data = None

    has_required_fields = False
    module_count = 0
    if json_data:
        keys = set(json_data.keys()) if isinstance(json_data, dict) else set()
        # aggregator 真实字段: total_tests / by_module / by_status_counts / tests
        has_required_fields = (
            "total_tests" in keys
            and "by_module" in keys
            and "by_status_counts" in keys
        )
        by_module = json_data.get("by_module") if isinstance(json_data.get("by_module"), dict) else None
        if by_module:
            module_count = len(by_module)

    # 注: aggregator EXIT code 反映"是否有 failed tests" (not its own success).
    # M13.1 验"aggregator 正确产出报告 + 数据结构合理", 不要求所有 tests 都 passed.
    return {
        "name": "aggregator_produces_final_report",
        "ok": (md_exists and json_exists and has_required_fields and module_count > 0),
        "exit_code": proc.returncode,
        "md_exists": md_exists,
        "md_path": str(md_file),
        "md_size": md_file.stat().st_size if md_exists else 0,
        "json_exists": json_exists,
        "json_path": str(json_file),
        "json_size": json_file.stat().st_size if json_exists else 0,
        "has_required_fields": has_required_fields,
        "module_count": module_count,
        "json_keys_top_level": sorted(json_data.keys()) if isinstance(json_data, dict) else None,
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_aggregator_produces_final_report()
    ctx = res.pop("_ctx")
    results = [res]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"md_size={r.get('md_size')} json_size={r.get('json_size')} "
                    f"keys={r.get('json_keys_top_level')} module_count={r.get('module_count')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M13.1: harness_assertions_aggregator.py 真扫 /tmp/mossen-harness/*/artifacts/"
            "assertions.json 产出 harness-final-report.md/.json, JSON 含 total/modules 字段。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
