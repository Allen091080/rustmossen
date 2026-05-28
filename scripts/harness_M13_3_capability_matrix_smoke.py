#!/usr/bin/env python3
"""M13.3 - capability matrix report smoke.

This verifies the harness_manifest/capability_matrix layer itself. It does not
require every product capability to pass; it requires the matrix to map every
M/R harness script and to report non-pass evidence explicitly.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


def main() -> int:
    ctx = make_fixture("M13.3")
    command = [
        "python3",
        str(ROOT / "scripts" / "harness_capability_matrix.py"),
        "--output-dir",
        str(ctx.artifacts_dir),
    ]
    proc = subprocess.run(
        command,
        cwd=str(ROOT),
        env=ctx.env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=30,
    )
    write_command_log(ctx, command, proc.stdout, proc.stderr, proc.returncode)

    report_path = ctx.artifacts_dir / "harness-capability-report.json"
    md_path = ctx.artifacts_dir / "harness-capability-report.md"
    report = json.loads(report_path.read_text(encoding="utf-8")) if report_path.exists() else {}
    coverage = report.get("script_coverage", {})
    status_counts = report.get("summary_by_status", {})
    capabilities = report.get("capabilities", [])
    non_pass_caps = [cap for cap in capabilities if cap.get("status") != "pass"]

    checks = [
        {
            "name": "matrix_command_exits_zero",
            "ok": proc.returncode == 0,
            "evidence": f"exit={proc.returncode}",
        },
        {
            "name": "json_and_markdown_reports_exist",
            "ok": report_path.exists() and md_path.exists(),
            "evidence": f"json={report_path.exists()} md={md_path.exists()}",
        },
        {
            "name": "all_mr_scripts_are_mapped",
            "ok": coverage.get("total_mr_scripts", 0) > 0
            and coverage.get("mapped_mr_scripts") == coverage.get("total_mr_scripts")
            and not coverage.get("unmapped_mr_scripts"),
            "evidence": json.dumps(coverage, ensure_ascii=False),
        },
        {
            "name": "capabilities_have_allowed_statuses",
            "ok": all(
                cap.get("status") in {"pass", "fail", "partial", "stale", "missing"}
                for cap in capabilities
            ),
            "evidence": json.dumps(status_counts, ensure_ascii=False),
        },
        {
            "name": "non_pass_evidence_is_reported_explicitly",
            "ok": all(cap.get("failed_invariants") for cap in non_pass_caps),
            "evidence": json.dumps(
                {
                    "non_pass": [
                        {"id": cap["id"], "status": cap.get("status")}
                        for cap in non_pass_caps
                    ]
                },
                ensure_ascii=False,
            ),
        },
    ]

    status = "passed" if all(item["ok"] for item in checks) else "failed"
    write_assertions(
        ctx,
        status=status,
        assertions=[
            {
                "name": item["name"],
                "expected": True,
                "actual": item["ok"],
                "passed": item["ok"],
                "evidence": item["evidence"],
            }
            for item in checks
        ],
        extra_artifacts={
            "capability_report_json": str(report_path),
            "capability_report_md": str(md_path),
        },
    )

    summary = {
        "test_id": ctx.test_id,
        "status": status,
        "passed": sum(1 for item in checks if item["ok"]),
        "total": len(checks),
        "fixture_root": str(ctx.root_dir),
        "report": str(report_path),
        "design_note": "M13.3 proves the capability matrix maps every M/R harness and reports non-pass evidence.",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
