#!/usr/bin/env python3
"""
M13.2 — 3 次连续稳定性 (P0).

按 harness全链路测试.md §C.1 + §1.1.6 契约: fresh fixture 下连续 3 次
全量 deterministic smoke 必须 100% 通过 (LLM-dependent 偶发 transient 不卡).

策略: 选一组 deterministic (不依赖 LLM 字面浮动) 的代表性 smoke, 跑 3 轮,
验每轮全过.
代表性 smoke (覆盖核心模块, 单跑 < 30s, 不依赖 LLM):
  - harness_M0_2_fixture_smoke.py
  - harness_M0_3_command_inventory.py
  - harness_M0_4_aggregator_smoke.py
  - harness_M5_2_memory_4types_smoke.py
  - harness_M5_3_cross_worktree_memory_smoke.py
  - harness_M5_6_memory_file_reload_smoke.py
  - harness_M6_1_skill_list_smoke.py
  - harness_M6_3_skill_reload_smoke.py
  - harness_M6_4_skill_sources_smoke.py
  - harness_M6_6_skill_error_isolation_smoke.py
  - harness_M7_1_plugin_install_list_smoke.py
  - harness_M7_2_plugin_command_trigger_smoke.py
  - harness_M7_3_plugin_reload_disable_smoke.py
  - harness_M7_4_plugin_failure_isolation_smoke.py
  - harness_M11_1_language_consistency_smoke.py
  - harness_M4_4_statusline_ctx_accuracy_smoke.py
  - harness_M12_1_statusline_config_smoke.py
  - harness_M8_1_command_inventory_real_smoke.py
  - harness_M8_2_safe_commands_run_smoke.py
  - harness_M8_3_side_effect_commands_smoke.py
  - harness_M8_4_hidden_commands_smoke.py
  - harness_M13_1_aggregate_report_smoke.py

观察点: 3 轮 × 22 测试 = 66 次, 全部 pass.

反测信号: 任一测试不稳定/有竞态 → 某轮某测试 fail → fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

DETERMINISTIC_SMOKES = [
    "harness_M0_2_fixture_smoke.py",
    "harness_M0_3_command_inventory.py",
    "harness_M0_4_aggregator_smoke.py",
    "harness_M5_2_memory_4types_smoke.py",
    "harness_M5_3_cross_worktree_memory_smoke.py",
    "harness_M5_6_memory_file_reload_smoke.py",
    "harness_M6_1_skill_list_smoke.py",
    "harness_M6_3_skill_reload_smoke.py",
    "harness_M6_4_skill_sources_smoke.py",
    "harness_M6_6_skill_error_isolation_smoke.py",
    "harness_M7_1_plugin_install_list_smoke.py",
    "harness_M7_2_plugin_command_trigger_smoke.py",
    "harness_M7_3_plugin_reload_disable_smoke.py",
    "harness_M7_4_plugin_failure_isolation_smoke.py",
    "harness_M11_1_language_consistency_smoke.py",
    "harness_M4_4_statusline_ctx_accuracy_smoke.py",
    "harness_M12_1_statusline_config_smoke.py",
    "harness_M8_1_command_inventory_real_smoke.py",
    "harness_M8_2_safe_commands_run_smoke.py",
    "harness_M8_3_side_effect_commands_smoke.py",
    "harness_M8_4_hidden_commands_smoke.py",
    "harness_M13_1_aggregate_report_smoke.py",
]
N_ROUNDS = 3


def case_3_rounds_all_pass() -> dict:
    ctx = make_fixture("M13.2")

    rounds: list[dict] = []
    for round_idx in range(1, N_ROUNDS + 1):
        round_results = []
        for smoke in DETERMINISTIC_SMOKES:
            smoke_path = ROOT / "scripts" / smoke
            proc = subprocess.run(
                ["python3", str(smoke_path)],
                cwd=str(ROOT),
                text=True,
                capture_output=True,
                timeout=180,
            )
            ok = proc.returncode == 0
            round_results.append({
                "smoke": smoke,
                "ok": ok,
                "exit_code": proc.returncode,
            })
        all_passed = all(r["ok"] for r in round_results)
        failed_smokes = [r["smoke"] for r in round_results if not r["ok"]]
        rounds.append({
            "round": round_idx,
            "all_passed": all_passed,
            "passed_count": sum(1 for r in round_results if r["ok"]),
            "total": len(round_results),
            "failed_smokes": failed_smokes,
        })

    all_rounds_pass = all(r["all_passed"] for r in rounds)

    write_command_log(
        ctx,
        ["python3 <22 deterministic smokes> x 3 rounds"],
        json.dumps(rounds, indent=2, ensure_ascii=False),
        "",
        0 if all_rounds_pass else 1,
    )

    return {
        "name": "three_rounds_all_pass",
        "ok": all_rounds_pass,
        "rounds": rounds,
        "smoke_count": len(DETERMINISTIC_SMOKES),
        "n_rounds": N_ROUNDS,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_3_rounds_all_pass()
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
                    f"smoke_count={r.get('smoke_count')} rounds={r.get('n_rounds')} "
                    f"per-round: " + ", ".join(
                        f"R{rd['round']}={rd['passed_count']}/{rd['total']}"
                        for rd in r.get("rounds", [])
                    )
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
            f"M13.2: {len(DETERMINISTIC_SMOKES)} deterministic smoke × {N_ROUNDS} rounds = "
            f"{len(DETERMINISTIC_SMOKES) * N_ROUNDS} runs, 全 pass。LLM-dependent smoke "
            f"(M1.x/M2.x model 字面) 不在 M13.2 集合 (附录 E L6 已记录 transient)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
