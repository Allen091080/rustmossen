#!/usr/bin/env python3
"""
M4.4 — statusline ctx 显示与 /context 同源 (P0)。

按 harness全链路测试.md §3.4 / §C.1 M4.4 契约:
  统称: statusline 的 ctx% 必须与 /context 命令显示的 token / window 同源
  (允许误差 <5%, 但 base 必须是同一个 model 的同一 window 推导)。

  代码事实 (调研结果):
    - statusline (src/components/StatusLine.tsx:49) 用:
        getContextWindowForModel(runtimeModel, getSdkBetas())
    - /context view (src/utils/status.tsx:58, /context 走 status.tsx) 用:
        getEffectiveContextWindowSize(mainLoopModel)
        其中 effectiveWindow = getContextWindowForModel(model, getSdkBetas())
                              - min(maxOutput, MAX_OUTPUT_TOKENS_FOR_SUMMARY=20_000)
    - 两者上游同 model 同 betas → 同 raw window
    - effective = raw - reservedForSummary (deterministic 推导)

  本测策略:
    bun -e 调真 getContextWindowForModel + getEffectiveContextWindowSize on
    同一个 model name (用现网常用 mossen-balanced-4-6 / max-4-7), 验:
      a) raw_window > 0
      b) effective_window > 0
      c) effective < raw (因 reserve 减去 max output)
      d) raw - effective ∈ [1, MAX_OUTPUT_TOKENS_FOR_SUMMARY=20000]
         —— 这个是两源"同源 + 一致 derive"的 deterministic sentinel

  反测信号 (mutation 抓力):
    - src/services/compact/autoCompact.ts:33 改 effective = 永远 raw / 2
      → effective != raw - reservedTokens → 上限关系破 → fail
    - src/utils/context.ts:118 改 default window 写死 0
      → raw == 0 或 effective <= 0 → fail
    - 改 getEffectiveContextWindowSize 让它独立调一个不同的 source
      → effective 与 raw 关系破裂 → fail (验"同源"的真意)

  补充: 单 model 的契约关系本身保证了"同源"; 不依赖 LLM 输出, 不需打满 ctx。
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_BUN = str(ROOT / "run-bun-featured.sh")

# 现网用过的 mossen 模型 alias —— 任一都能 resolve 到 provider context window
PROBE_MODELS = (
    "mossen-balanced-4-6",
    "mossen-max-4-7",
)

MAX_OUTPUT_TOKENS_FOR_SUMMARY = 20_000  # autoCompact.ts:30 字面常量


def _bun_probe_windows(env: dict, models: tuple[str, ...]) -> tuple[int, str, str]:
    """
    一次 bun -e 调用里, 对每个 model 同时调 statusline 源 + /context 源,
    输出 JSON list 含 raw / effective / model。
    """
    models_json = json.dumps(list(models))
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const { getContextWindowForModel } = await import('./utils/context.ts');"
        "const { getEffectiveContextWindowSize } = "
        "  await import('./services/compact/autoCompact.ts');"
        f"const models = {models_json};"
        "const out = models.map((m) => ({"
        "  model: m,"
        "  raw_window_statusline_source: getContextWindowForModel(m),"
        "  effective_window_context_source: getEffectiveContextWindowSize(m),"
        "}));"
        "process.stdout.write(JSON.stringify({ probes: out }) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_last_json(stdout: str) -> dict | None:
    for line in reversed((stdout or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def case_statusline_ctx_window_same_source() -> dict:
    ctx = make_fixture("M4.4")

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    rc, stdout, stderr = _bun_probe_windows(env, PROBE_MODELS)
    parsed = _parse_last_json(stdout)

    write_command_log(
        ctx,
        [RUN_BUN, "-e", "<getContextWindowForModel + getEffectiveContextWindowSize probe>"],
        stdout, stderr, rc,
    )

    if parsed is None:
        return {
            "name": "M4_4_statusline_ctx_window_same_source",
            "ok": False,
            "stage": "parse",
            "exit_code": rc,
            "stdout_excerpt": stdout[:600],
            "stderr_excerpt": stderr[:600],
            "fixture_root": str(ctx.root_dir),
            "_ctx": ctx,
        }

    probes = parsed.get("probes") or []
    per_model_results = []
    all_ok = True
    for probe in probes:
        raw = probe.get("raw_window_statusline_source")
        eff = probe.get("effective_window_context_source")
        if not isinstance(raw, int) or not isinstance(eff, int):
            per_model_results.append({**probe, "ok": False, "reason": "non_int"})
            all_ok = False
            continue

        diff = raw - eff
        relation_ok = (
            raw > 0
            and eff > 0
            and eff < raw
            and 1 <= diff <= MAX_OUTPUT_TOKENS_FOR_SUMMARY
        )
        per_model_results.append({
            **probe,
            "diff_tokens": diff,
            "diff_within_reserved": 1 <= diff <= MAX_OUTPUT_TOKENS_FOR_SUMMARY,
            "ok": relation_ok,
        })
        if not relation_ok:
            all_ok = False

    ok = rc == 0 and len(probes) == len(PROBE_MODELS) and all_ok

    return {
        "name": "M4_4_statusline_ctx_window_same_source",
        "ok": ok,
        "exit_code": rc,
        "models_probed": list(PROBE_MODELS),
        "per_model": per_model_results,
        "max_output_tokens_for_summary": MAX_OUTPUT_TOKENS_FOR_SUMMARY,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = case_statusline_ctx_window_same_source()
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
                    f"exit={r.get('exit_code')} "
                    f"per_model={r.get('per_model')}"
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
            "M4.4: statusline 用 getContextWindowForModel (raw), /context 用 "
            "getEffectiveContextWindowSize (= raw - reservedForSummary)。"
            "两源同上游 → 关系 1 <= raw-eff <= 20000 必须成立 (deterministic "
            "sentinel, 不依赖 LLM)。改 effective derive 算法 → 关系破 → fail。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
