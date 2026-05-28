#!/usr/bin/env python3
"""
V-2: Layer 0 baseline 快照生成器.

按 OpenTelemetry删除计划.md §3 Layer 0 (重设计版) + D agent §0.4.5.

跑 N 个核心场景, 每场景:
  1. 启动 mossen (带 fixture 隔离)
  2. 跑结束后用 V-1 (validate_structural_equivalence) 跑 8 维断言
  3. 落盘 JSON schema

输出: tmp/baseline_schema.json (含 5-8 个场景的 8 维结果 + 关键稳定字段)

使用例:
    python3 scripts/capture_baseline_schema.py --output /tmp/baseline_schema.json

后续 slice 完成时跑同一脚本 → diff 两份 schema → 找 regression.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture
from validate_structural_equivalence import validate


def _find_session_jsonl(home_dir: Path) -> Path | None:
    """找 fixture HOME 下最新的 session jsonl."""
    candidates: list[Path] = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        candidates.extend(home_dir.glob(pattern))
    candidates = [p for p in candidates if p.is_file()]
    if not candidates:
        return None
    candidates.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return candidates[0]


def _make_env(ctx) -> dict:
    """补 MOSSEN_CONFIG_DIR (已知 bug R-018)."""
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    return env


def run_scenario(scenario_id: str, prompt: str, expected: dict, timeout: int = 180) -> dict:
    """跑一个场景, 返回 8 维 + meta."""
    ctx = make_fixture(f"baseline_{scenario_id}")
    env = _make_env(ctx)
    cwd = ctx.root_dir / "fake_project"
    cwd.mkdir(parents=True, exist_ok=True)

    cli = str(ROOT / "run-mossen.sh")
    cmd = [cli, "-p"]
    if expected.get("allowedTools"):
        cmd.extend(["--allowedTools", expected["allowedTools"]])

    started = time.time()
    proc = subprocess.run(
        cmd,
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=str(cwd),
    )
    duration = time.time() - started

    session_jsonl = _find_session_jsonl(ctx.home_dir)
    session_jsonl_str = str(session_jsonl) if session_jsonl else ""

    val = validate(
        scenario=scenario_id,
        session_jsonl=session_jsonl_str or "/__missing__",
        stdout=proc.stdout,
        stderr=proc.stderr,
        exit_code=proc.returncode,
        expected=expected,
    )

    return {
        "scenario": scenario_id,
        "prompt": prompt[:200],
        "exit_code": proc.returncode,
        "duration_s": round(duration, 2),
        "session_jsonl": session_jsonl_str,
        "session_jsonl_exists": bool(session_jsonl),
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "validation": val.to_dict(),
    }


SCENARIOS = [
    {
        "id": "S1_simple_echo",
        "prompt": "请说: BASELINE_TEST_S1",
        "expected": {"expected_exit_code": 0},
        "timeout": 120,
    },
    {
        "id": "S2_bash_tool",
        "prompt": "请用 Bash 工具执行 echo BASELINE_TEST_S2_MARKER",
        "expected": {
            "expected_exit_code": 0,
            "require_tool_pairing": True,
            "allowedTools": "Bash",
        },
        "timeout": 180,
    },
    {
        "id": "S3_simple_qa",
        "prompt": "请把以下字符串原样回复给我: BASELINE_S3_OK_TOKEN_xyz",
        "expected": {"expected_exit_code": 0},
        "timeout": 120,
    },
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--output", required=True)
    ap.add_argument("--scenarios", nargs="*", help="只跑指定 id (默认全跑)")
    ap.add_argument("--quick", action="store_true", help="只跑 S1 (快速 smoke)")
    args = ap.parse_args()

    scenarios = SCENARIOS
    if args.quick:
        scenarios = [s for s in SCENARIOS if s["id"] == "S1_simple_echo"]
    elif args.scenarios:
        scenarios = [s for s in SCENARIOS if s["id"] in args.scenarios]

    results = []
    for s in scenarios:
        print(f"=== Running {s['id']} ===", flush=True)
        try:
            r = run_scenario(s["id"], s["prompt"], s["expected"], s.get("timeout", 180))
            results.append(r)
            v = r["validation"]
            print(
                f"  → exit={r['exit_code']} duration={r['duration_s']}s "
                f"validation={v['passed_count']}/{v['total_count']} "
                f"({'PASS' if v['passed'] else 'FAIL'})"
            )
        except subprocess.TimeoutExpired as e:
            results.append({
                "scenario": s["id"],
                "error": "timeout",
                "timeout_s": s.get("timeout", 180),
            })
            print(f"  → TIMEOUT after {s.get('timeout', 180)}s")

    summary = {
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "total_scenarios": len(scenarios),
        "passed_scenarios": sum(
            1 for r in results
            if "validation" in r and r["validation"]["passed"]
        ),
        "results": results,
    }

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")

    print()
    print(f"Baseline schema saved: {out_path}")
    print(f"Passed scenarios: {summary['passed_scenarios']}/{summary['total_scenarios']}")
    return 0 if summary["passed_scenarios"] == summary["total_scenarios"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
