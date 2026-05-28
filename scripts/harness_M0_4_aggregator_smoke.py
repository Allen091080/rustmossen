#!/usr/bin/env python3
"""
M0.4 — 验 harness_assertions_aggregator 真聚合 + 产出 markdown + json。

按 §C.7 要求验收:
  Case 1: 建 mock fixture root + 写 3 个 fake assertions.json → aggregator 真扫到 3 个
  Case 2: aggregator 真产出 .json 和 .md 两个文件
  Case 3: json 报告含 total_tests / by_status_counts / by_module / tests 字段
  Case 4: md 报告含 "测试总数" / "按状态分组" / 测试 id 表格
  Case 5: 含 failed 时, aggregator exit code 非 0; 全 passed 时 exit 0

反测信号 (mutation):
  改 discover_assertions 让它跳过某 assertions → case 1 数量不对 → fail
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


def setup_mock_harness_root() -> Path:
    """造一个独立的 mock harness root, 含 3 个 fake test assertions.json."""
    mock_root = Path("/tmp/mossen-harness-M0_4-mock")
    if mock_root.exists():
        shutil.rmtree(mock_root)
    for test_id, status in [("MOCK1.1", "passed"),
                             ("MOCK1.2", "passed"),
                             ("MOCK2.1", "failed")]:
        artifacts = mock_root / test_id / "artifacts"
        artifacts.mkdir(parents=True)
        (artifacts / "assertions.json").write_text(json.dumps({
            "test_id": test_id,
            "status": status,
            "timestamp": "2026-04-25T00:00:00",
            "assertions": [
                {"name": "demo", "expected": True, "actual": True, "passed": True}
            ],
            "artifacts": {
                "stdout": str(artifacts / "stdout.txt"),
            },
        }))
    return mock_root


def case_aggregator_finds_all() -> dict:
    mock_root = setup_mock_harness_root()
    out_dir = Path("/tmp/mossen-harness-M0_4-output")
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)

    proc = subprocess.run(
        ["python3", str(ROOT / "scripts" / "harness_assertions_aggregator.py"),
         "--harness-root", str(mock_root),
         "--output-dir", str(out_dir),
         "--skip-capability-matrix",
         "--quiet"],
        capture_output=True,
        text=True,
        timeout=30,
    )

    json_report_path = out_dir / "harness-final-report.json"
    md_report_path = out_dir / "harness-final-report.md"
    json_exists = json_report_path.exists()
    md_exists = md_report_path.exists()

    if json_exists:
        report = json.loads(json_report_path.read_text())
    else:
        report = {}

    return {
        "name": "aggregator_finds_all",
        "ok": (
            json_exists
            and md_exists
            and report.get("total_tests") == 3
            and report.get("by_status_counts", {}).get("passed") == 2
            and report.get("by_status_counts", {}).get("failed") == 1
        ),
        "exit_code": proc.returncode,
        "json_exists": json_exists,
        "md_exists": md_exists,
        "report_total": report.get("total_tests"),
        "by_status": report.get("by_status_counts"),
    }


def case_json_report_shape() -> dict:
    """json 报告必须含规定字段."""
    out_dir = Path("/tmp/mossen-harness-M0_4-output")
    json_report_path = out_dir / "harness-final-report.json"
    if not json_report_path.exists():
        return {"name": "json_report_shape", "ok": False, "error": "no json output"}
    report = json.loads(json_report_path.read_text())
    required_fields = ("total_tests", "total_assertions", "by_status_counts",
                       "by_module", "tests", "generated_at")
    missing = [f for f in required_fields if f not in report]
    return {
        "name": "json_report_shape",
        "ok": len(missing) == 0,
        "missing": missing,
        "tests_is_list": isinstance(report.get("tests"), list),
    }


def case_md_report_content() -> dict:
    """md 报告必须含规定章节字面."""
    out_dir = Path("/tmp/mossen-harness-M0_4-output")
    md_report_path = out_dir / "harness-final-report.md"
    if not md_report_path.exists():
        return {"name": "md_report_content", "ok": False, "error": "no md output"}
    md_text = md_report_path.read_text()
    required_strings = (
        "Mossen Harness 最终聚合报告",
        "总览",
        "测试总数",
        "按状态分组",
        "按模块分组",
        "测试明细",
        "MOCK1.1",
        "MOCK2.1",
    )
    missing = [s for s in required_strings if s not in md_text]
    return {
        "name": "md_report_content",
        "ok": len(missing) == 0,
        "missing": missing,
        "md_size": len(md_text),
    }


def case_exit_code_reflects_failures() -> dict:
    """有 failed test 时 exit_code != 0; 全 passed 时 = 0."""
    # 当前 mock 含 1 failed → exit != 0
    mock_root = Path("/tmp/mossen-harness-M0_4-mock")  # 已建
    out_dir = Path("/tmp/mossen-harness-M0_4-output")
    proc1 = subprocess.run(
        ["python3", str(ROOT / "scripts" / "harness_assertions_aggregator.py"),
         "--harness-root", str(mock_root),
         "--output-dir", str(out_dir),
         "--skip-capability-matrix",
         "--quiet"],
        capture_output=True,
        text=True,
        timeout=30,
    )
    has_failed_exit_nonzero = proc1.returncode != 0

    # 全 passed mock
    mock2 = Path("/tmp/mossen-harness-M0_4-mock-allpass")
    if mock2.exists():
        shutil.rmtree(mock2)
    artifacts = mock2 / "OK1.1" / "artifacts"
    artifacts.mkdir(parents=True)
    (artifacts / "assertions.json").write_text(json.dumps({
        "test_id": "OK1.1", "status": "passed",
        "assertions": [{"name": "demo", "passed": True}],
        "artifacts": {},
    }))
    proc2 = subprocess.run(
        ["python3", str(ROOT / "scripts" / "harness_assertions_aggregator.py"),
         "--harness-root", str(mock2),
         "--output-dir", str(out_dir),
         "--skip-capability-matrix",
         "--quiet"],
        capture_output=True,
        text=True,
        timeout=30,
    )
    all_passed_exit_zero = proc2.returncode == 0

    return {
        "name": "exit_code_reflects_failures",
        "ok": has_failed_exit_nonzero and all_passed_exit_zero,
        "with_failed_exit": proc1.returncode,
        "all_passed_exit": proc2.returncode,
    }


def main() -> int:
    ctx = make_fixture("M0.4")
    results = [
        case_aggregator_finds_all(),
        case_json_report_shape(),
        case_md_report_content(),
        case_exit_code_reflects_failures(),
    ]

    write_command_log(ctx, ["python3", str(Path(__file__).name)],
                      json.dumps(results, ensure_ascii=False), "",
                      0 if all(r.get("ok") for r in results) else 1)
    write_assertions(ctx,
                     status="passed" if all(r.get("ok") for r in results) else "failed",
                     assertions=[
                         {"name": r["name"], "expected": True,
                          "actual": r.get("ok"), "passed": r.get("ok")}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M0.4 aggregator 验: 真扫 mock harness root → 真产出 .json + .md → "
            "shape / content / exit code 全验"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
