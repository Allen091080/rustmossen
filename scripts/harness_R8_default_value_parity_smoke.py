#!/usr/bin/env python3
"""
R8 — Mossen 门面默认值与 GrowthBook 旧默认值 parity 安全网测试 (G2-2c, weak framework).

按 GrowthBook迁移计划.md §1.3 + G0-5 测试矩阵 §R8 + D-G05-D 决策.

守护契约 (随 slice 渐进收紧):
  - G2-2c 当前: 全 weak — 仅产 parity / drift / not_migrated 报告; 不 fail
  - G3-1 起每个 slice 把对应 key 加入 STRICT_KEYS; 该集合内 drift > 0 即 fail
  - D-G05-D = a: STRICT_KEYS 不含 default_source_file == "unknown" 的 key (静态审计不可信)
  - G7 收口: not_migrated_yet == 0 (除主动豁免) + drift == 0 across STRICT_KEYS

设计:
  - 一次 `bun -e` 把全部 62 key 跑完, 避免 62 次 subprocess 启动
  - 每个 key 用 MAGIC_FALLBACK 区分 "门面真返回 default" vs "门面没数据"
  - deep_equal 对 nested object (eg. {scheduledDelayMillis, maxExportBatchSize, ...}) 做严格比对

输出 status (每 key):
  - parity:           门面返回值 == proposed_default_value (deep eq)
  - drift:            门面有值但 != proposed (G-R001 风险, slice 内 fail)
  - not_migrated_yet: 门面返回 MAGIC_FALLBACK (说明该 key 未注入 LocalDefaultProvider)

反测信号:
  - LocalDefaultProvider 写错 nested 字段 (maxExportBatchSize 100 vs 512) → drift fail (slice 内)
  - 迁移过程中误改默认值 → drift fail
  - slice 完了但 LocalDefaultProvider 漏了某 key → not_migrated_yet 数 ↑ → G7 验收失败
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


KEYS_JSON = ROOT / "tmp" / "growthbook-audit" / "keys.json"
MAGIC_FALLBACK = "__R8_MAGIC_NOT_MIGRATED__"

# Slices add their key here on completion.
# Format: {proposed_mossen_key: {"slice": "G3-1", ...}}
STRICT_KEYS: dict[str, dict] = {
    'mossen.analytics.eventBatchConfig': {'slice': 'G3-1',
                                            'old_key': 'tengu_1p_event_batch_config'},
    'mossen.permission.channelsEnabled': {'slice': 'G4-6',
                                            'old_key': 'tengu_harbor'},
    'mossen.ui.autoModeConfig': {'slice': 'G4-6',
                                  'old_key': 'tengu_auto_mode_config'},
}


BUN_PROBE_SCRIPT = r"""
import { resolveMossenConfig } from './services/config/index.ts'

const MAGIC = process.env.R8_MAGIC_FALLBACK
const keysPayload = JSON.parse(process.env.R8_KEYS_JSON)

const out = []
for (const k of keysPayload) {
  let resolved
  let err = null
  try {
    resolved = resolveMossenConfig(k.proposed_mossen_key, MAGIC)
  } catch (e) {
    err = e instanceof Error ? e.message : String(e)
  }
  out.push({
    proposed_mossen_key: k.proposed_mossen_key,
    old_key: k.old_key,
    expected_default: k.proposed_default_value,
    actual_value: resolved?.value ?? null,
    actual_source: resolved?.source ?? null,
    error: err,
  })
}
process.stdout.write(JSON.stringify(out))
"""


def _deep_eq(a, b) -> bool:
    """JSON-shaped deep equality (handles dicts/lists/primitives)."""
    if type(a) != type(b):
        # JSON: int/float should compare numerically
        if isinstance(a, (int, float)) and isinstance(b, (int, float)):
            return a == b
        return False
    if isinstance(a, dict):
        if set(a.keys()) != set(b.keys()):
            return False
        return all(_deep_eq(a[k], b[k]) for k in a)
    if isinstance(a, list):
        if len(a) != len(b):
            return False
        return all(_deep_eq(x, y) for x, y in zip(a, b))
    return a == b


def _classify(entry: dict) -> str:
    if entry.get("error"):
        return "error"
    actual = entry["actual_value"]
    expected = entry["expected_default"]
    if actual == MAGIC_FALLBACK:
        return "not_migrated_yet"
    if _deep_eq(actual, expected):
        return "parity"
    return "drift"


def main() -> int:
    ctx = make_fixture("R8_default_value_parity")

    if not KEYS_JSON.exists():
        write_command_log(ctx, ["R8"], "", f"keys.json not found: {KEYS_JSON}", 2)
        write_assertions(ctx, status="blocked", assertions=[{
            "name": "keys_json_present", "expected": True,
            "actual": False, "passed": False,
        }])
        print(json.dumps({"error": f"keys.json missing: {KEYS_JSON}"}, indent=2))
        return 2

    catalog = json.loads(KEYS_JSON.read_text())
    keys = catalog["keys"]

    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env.pop("MOSSEN_CONFIG_OVERRIDES", None)
    env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    env["R8_MAGIC_FALLBACK"] = MAGIC_FALLBACK
    env["R8_KEYS_JSON"] = json.dumps(keys)

    proc = subprocess.run(
        ["bun", "-e", BUN_PROBE_SCRIPT],
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
        cwd=str(ROOT),
    )
    write_command_log(ctx, ["bun", "-e", "(R8 probe script)"],
                      proc.stdout[:2000], proc.stderr[:2000], proc.returncode)

    if proc.returncode != 0:
        write_assertions(ctx, status="failed", assertions=[{
            "name": "bun_probe_exit_zero",
            "expected": 0, "actual": proc.returncode, "passed": False,
            "evidence": proc.stderr[:500],
        }])
        print(json.dumps({
            "error": "bun probe failed",
            "exit_code": proc.returncode,
            "stderr_excerpt": proc.stderr[:1000],
        }, indent=2))
        return 1

    try:
        entries = json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        write_assertions(ctx, status="failed", assertions=[{
            "name": "bun_probe_json_parse",
            "expected": True, "actual": False, "passed": False,
            "evidence": f"{e}; stdout={proc.stdout[:500]}",
        }])
        return 1

    # Classify
    by_status: dict[str, list] = {"parity": [], "drift": [], "not_migrated_yet": [],
                                   "error": []}
    by_status_unknown_src: dict[str, int] = {"parity": 0, "drift": 0,
                                              "not_migrated_yet": 0, "error": 0}
    by_status_known_src: dict[str, int] = {"parity": 0, "drift": 0,
                                            "not_migrated_yet": 0, "error": 0}
    drift_in_strict: list[dict] = []
    not_migrated_in_strict: list[dict] = []

    src_lookup = {k["proposed_mossen_key"]: k["default_source_file"] for k in keys}

    for e in entries:
        status = _classify(e)
        e["status"] = status
        by_status[status].append({
            "key": e["proposed_mossen_key"],
            "old_key": e["old_key"],
            "actual": e["actual_value"],
            "expected": e["expected_default"],
            "actual_source": e["actual_source"],
        })
        is_unknown_src = src_lookup.get(e["proposed_mossen_key"]) == "unknown"
        (by_status_unknown_src if is_unknown_src else by_status_known_src)[status] += 1

        if e["proposed_mossen_key"] in STRICT_KEYS:
            if status == "drift":
                drift_in_strict.append(e)
            elif status == "not_migrated_yet":
                not_migrated_in_strict.append(e)

    summary = {
        "total": len(entries),
        "by_status": {k: len(v) for k, v in by_status.items()},
        "by_status_known_source": by_status_known_src,
        "by_status_unknown_source": by_status_unknown_src,
        "strict_keys_count": len(STRICT_KEYS),
        "drift_in_strict_count": len(drift_in_strict),
        "not_migrated_in_strict_count": len(not_migrated_in_strict),
    }

    # Pass criteria:
    #   - WEAK (G2-2c, STRICT_KEYS empty): any drift OK, just produce report
    #   - STRICT (per-slice): drift_in_strict == 0 AND not_migrated_in_strict == 0
    ok = len(drift_in_strict) == 0 and len(not_migrated_in_strict) == 0

    write_assertions(
        ctx,
        status="passed" if ok else "failed",
        assertions=[
            {
                "name": "no_drift_in_strict_keys",
                "expected": 0, "actual": len(drift_in_strict),
                "passed": len(drift_in_strict) == 0,
                "evidence": json.dumps(summary),
            },
            {
                "name": "no_not_migrated_in_strict_keys",
                "expected": 0, "actual": len(not_migrated_in_strict),
                "passed": len(not_migrated_in_strict) == 0,
            },
        ],
        extra_artifacts={"r8_full_report": str(ctx.artifacts_dir / "r8_report.json")},
    )

    full_report = {
        "summary": summary,
        "strict_keys": list(STRICT_KEYS.keys()),
        "drift_in_strict": drift_in_strict,
        "not_migrated_in_strict": not_migrated_in_strict,
        "by_status_lists": by_status,
        "design_note": (
            "R8 (G2-2c framework, full weak): STRICT_KEYS=[] → 报告型, "
            "drift/not_migrated 仅记录. G3-1 起逐 slice 加入 strict 集合."
        ),
    }
    (ctx.artifacts_dir / "r8_report.json").write_text(
        json.dumps(full_report, indent=2, ensure_ascii=False)
    )

    print(json.dumps({
        "summary": summary,
        "drift_in_strict": [d["proposed_mossen_key"] for d in drift_in_strict],
        "not_migrated_in_strict": [d["proposed_mossen_key"]
                                    for d in not_migrated_in_strict],
        "report_path": str(ctx.artifacts_dir / "r8_report.json"),
        "design_note": full_report["design_note"],
    }, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
