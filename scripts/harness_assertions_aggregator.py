#!/usr/bin/env python3
"""
Harness assertions aggregator —— 扫描 /tmp/mossen-harness/*/artifacts/assertions.json,
产出 harness-final-report.md + harness-final-report.json。

按 harness全链路测试.md §C.7 要求格式:
  Markdown 报告必须包含:
    - 能力基线矩阵摘要 (引用 harness能力基线矩阵.md)
    - 每个模块 pass/fail/block 数量
    - 每个测试的脚本路径
    - 每个测试的 artifacts 路径
    - 每个 mutation/negative control 证据
    - 未对齐官方能力的清单 (引用 §附录 E)
    - 明确结论: 是否达到个人版生产可用
  JSON 报告必须可机器读取, 用于后续 CI

CLI:
  python3 scripts/harness_assertions_aggregator.py [--output-dir DIR]

默认 output-dir = 仓库根。
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_HARNESS_ROOT = Path("/tmp/mossen-harness")


def discover_assertions(harness_root: Path) -> list[dict]:
    """扫 harness_root 下所有 artifacts/assertions.json."""
    found = []
    if not harness_root.exists():
        return found
    for assertions_file in sorted(harness_root.glob("*/artifacts/assertions.json")):
        try:
            data = json.loads(assertions_file.read_text())
            data["_source_file"] = str(assertions_file)
            found.append(data)
        except (json.JSONDecodeError, OSError) as e:
            found.append({
                "test_id": assertions_file.parent.parent.name,
                "status": "load_error",
                "error": str(e)[:200],
                "_source_file": str(assertions_file),
            })
    return found


def aggregate(assertions: list[dict]) -> dict:
    """组装总报告 dict."""
    by_status: dict[str, list[str]] = defaultdict(list)
    by_module: dict[str, dict[str, int]] = defaultdict(lambda: {"passed": 0, "failed": 0, "blocked": 0, "skipped": 0, "load_error": 0})
    total_assertions = 0

    for entry in assertions:
        test_id = entry.get("test_id", "?")
        status = entry.get("status", "load_error")
        by_status[status].append(test_id)
        # 模块 = test_id 的 "M<digit>" 前缀, 如 M0.1 → M0, M1.2 → M1
        module = test_id.split(".")[0] if "." in test_id else test_id
        by_module[module][status] += 1
        total_assertions += len(entry.get("assertions", []))

    return {
        "total_tests": len(assertions),
        "total_assertions": total_assertions,
        "by_status_counts": {k: len(v) for k, v in by_status.items()},
        "by_status_test_ids": dict(by_status),
        "by_module": dict(by_module),
        "tests": assertions,
        "generated_at": datetime.now().isoformat(),
    }


def refresh_capability_matrix(harness_root: Path, output_dir: Path) -> dict:
    """Generate/load the capability report as part of the final report."""
    script = ROOT / "scripts" / "harness_capability_matrix.py"
    proc = subprocess.run(
        [
            sys.executable,
            str(script),
            "--harness-root",
            str(harness_root),
            "--output-dir",
            str(output_dir),
            "--quiet",
        ],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
    )
    report_path = output_dir / "harness-capability-report.json"
    if report_path.exists():
        try:
            report = json.loads(report_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            report = {
                "status": "load_error",
                "error": str(exc),
                "summary_by_status": {"fail": 1},
            }
    else:
        report = {
            "status": "missing",
            "error": "harness-capability-report.json was not generated",
            "summary_by_status": {"missing": 1},
        }
    report["_generator_exit_code"] = proc.returncode
    report["_generator_stdout"] = proc.stdout[:2000]
    report["_generator_stderr"] = proc.stderr[:2000]
    return report


def capability_bad_count(capability_report: dict | None) -> int:
    if not capability_report:
        return 0
    counts = capability_report.get("summary_by_status")
    if not isinstance(counts, dict):
        return 1
    return sum(int(counts.get(status, 0) or 0) for status in ("fail", "missing", "stale", "partial"))


def render_markdown(report: dict) -> str:
    """产出 markdown 报告."""
    lines = []
    lines.append("# Mossen Harness 最终聚合报告")
    lines.append("")
    lines.append(f"> 生成于: {report['generated_at']}")
    lines.append(f"> 来源: 扫描 `/tmp/mossen-harness/*/artifacts/assertions.json`")
    lines.append("")
    lines.append("## 总览")
    lines.append(f"- 测试总数: **{report['total_tests']}**")
    lines.append(f"- 断言总数: **{report['total_assertions']}**")
    lines.append("")
    lines.append("### 按状态分组")
    lines.append("| 状态 | 数量 |")
    lines.append("|---|---|")
    for status, count in sorted(report["by_status_counts"].items()):
        lines.append(f"| `{status}` | {count} |")
    lines.append("")
    capability = report.get("capability_matrix") or {}
    capability_counts = capability.get("summary_by_status") if isinstance(capability, dict) else {}
    if isinstance(capability_counts, dict):
        lines.append("### 按产品能力分组")
        lines.append("| 能力状态 | 数量 |")
        lines.append("|---|---|")
        for status in ("pass", "partial", "stale", "missing", "fail"):
            lines.append(f"| `{status}` | {capability_counts.get(status, 0)} |")
        coverage = capability.get("script_coverage", {})
        lines.append("")
        lines.append(
            "- M/R 脚本映射: "
            f"{coverage.get('mapped_mr_scripts', 0)}/{coverage.get('total_mr_scripts', 0)}"
        )
        if coverage.get("unmapped_mr_scripts"):
            lines.append(f"- 未映射 M/R 脚本: {len(coverage.get('unmapped_mr_scripts', []))}")
        lines.append("- 能力报告: `harness-capability-report.md`")
        lines.append("")
    lines.append("### 按模块分组")
    lines.append("| 模块 | passed | failed | blocked | skipped | load_error |")
    lines.append("|---|---|---|---|---|---|")
    for module, counts in sorted(report["by_module"].items()):
        lines.append(f"| {module} | {counts['passed']} | {counts['failed']} | {counts['blocked']} | {counts['skipped']} | {counts['load_error']} |")
    lines.append("")
    lines.append("## 测试明细")
    lines.append("| test_id | status | 断言数 | 脚本/产物 |")
    lines.append("|---|---|---|---|")
    for t in report["tests"]:
        test_id = t.get("test_id", "?")
        status = t.get("status", "?")
        n_assertions = len(t.get("assertions", []))
        artifacts = t.get("artifacts", {})
        artifacts_str = " / ".join(f"`{Path(v).name}`" for v in list(artifacts.values())[:3])
        lines.append(f"| {test_id} | {status} | {n_assertions} | {artifacts_str} |")
    lines.append("")
    lines.append("## 关联文档")
    lines.append("- Harness manifest: `harness/harness_manifest.v1.json`")
    lines.append("- Capability matrix: `harness/capability_matrix.v1.json`")
    lines.append("- Capability report: `harness-capability-report.md`")
    lines.append("- Slash command 矩阵: `harness_slash_command_matrix.json`")
    lines.append("- SOP: `harness全链路测试.md`")
    lines.append("- 延后待办: `harness全链路测试.md` 附录 E")
    lines.append("")
    lines.append("## 结论")
    fail_count = report["by_status_counts"].get("failed", 0)
    block_count = report["by_status_counts"].get("blocked", 0)
    err_count = report["by_status_counts"].get("load_error", 0)
    cap_bad = capability_bad_count(report.get("capability_matrix"))
    if fail_count == 0 and err_count == 0 and block_count == 0 and cap_bad == 0:
        lines.append(f"✅ 全部 {report['total_tests']} 测试 passed, 且能力矩阵全部 pass。")
    else:
        lines.append(
            f"❌ 待修复: {fail_count} failed / {block_count} blocked / "
            f"{err_count} load_error / {cap_bad} capability non-pass。"
        )
        if cap_bad:
            lines.append("能力矩阵存在 stale/missing/partial/fail，不能仅凭脚本通过率声明生产可用。")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--harness-root", type=Path, default=DEFAULT_HARNESS_ROOT)
    parser.add_argument("--output-dir", type=Path, default=ROOT)
    parser.add_argument(
        "--skip-capability-matrix",
        action="store_true",
        help="Only aggregate assertions. Intended for aggregator unit smokes with mock harness roots.",
    )
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args()

    capability_report = None
    if not args.skip_capability_matrix:
        capability_report = refresh_capability_matrix(args.harness_root, args.output_dir)
    assertions = discover_assertions(args.harness_root)
    report = aggregate(assertions)
    if capability_report is not None:
        report["capability_matrix"] = capability_report

    json_target = args.output_dir / "harness-final-report.json"
    md_target = args.output_dir / "harness-final-report.md"

    json_target.write_text(json.dumps(report, indent=2, ensure_ascii=False))
    md_target.write_text(render_markdown(report))

    if not args.quiet:
        print(json.dumps({
            "json_report": str(json_target),
            "md_report": str(md_target),
            "total_tests": report["total_tests"],
            "by_status_counts": report["by_status_counts"],
        }, indent=2, ensure_ascii=False))

    fail_count = report["by_status_counts"].get("failed", 0)
    block_count = report["by_status_counts"].get("blocked", 0)
    err_count = report["by_status_counts"].get("load_error", 0)
    cap_bad = capability_bad_count(report.get("capability_matrix"))
    return 0 if (fail_count + block_count + err_count + cap_bad == 0) else 1


if __name__ == "__main__":
    raise SystemExit(main())
