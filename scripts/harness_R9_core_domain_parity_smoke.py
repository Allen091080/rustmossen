#!/usr/bin/env python3
"""
R9 — 核心域 (compact / memory / permission / model) 行为 parity 安全网测试 (G2-2d, weak framework).

按 GrowthBook迁移计划.md §G2-2 (G0-7 split G2-2d) + G0-5 测试矩阵 §R9.

设计 4 sub-case (每域 1 个, 可独立 run):
  R9.compact     M4_1 同款: 强制 autocompact 触发, marker R9_COMPACT_MARKER
  R9.memory      M5_1 同款: 写 memory → restart → read marker R9_MEMORY_MARKER
  R9.permission  M2_4 同款: acceptEdits / bypassPermissions / default 三态
  R9.model       M9_3 同款: --model <X> 后 session jsonl 第一条 message model 字段

守护契约 (随 slice 渐进收紧):
  - G2-2d 当前: WEAK framework — exit 0 + session 落盘 = pass; 仅记录 baseline data
  - G4-1 完成后 (compact 域 44 keys 迁移): R9.compact 加 STRICT (autocompact 真触发)
  - G4-2 完成后 (memory 域 34 keys): R9.memory 加 STRICT (4 类 frontmatter 全加载)
  - G4-3 完成后 (permission 域): R9.permission 加 STRICT (3 模式真生效)
  - G4-4 完成后 (model 域): R9.model 加 STRICT (model 字段 deep_eq baseline)
  - G7 收口: 4 sub-case 全 STRICT pass + 与 tmp/baseline_schema.json 8 维 deep_eq

反测信号 (STRICT 后):
  - compact 阈值默认值漂移 → autocompact 不触发 → R9.compact fail
  - memory frontmatter type 字段映射错 → 4 类 memory 缺一 → R9.memory fail
  - permission 模式 gate 默认值改 → bypass 当 default → R9.permission fail
  - model override gate 误关 → session model 字段不变 → R9.model fail
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


WEAK_MODE = True  # G2-2d framework 阶段; 各 G4-x slice 后逐域切 STRICT


def _make_env(ctx, *, extra: dict | None = None) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_NON_INTERACTIVE_SESSION"] = "1"
    env["MOSSEN_CODE_TRUST_DIALOG_ACCEPTED"] = "1"
    env.pop("MOSSEN_CONFIG_OVERRIDES", None)
    env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    if extra:
        env.update(extra)
    return env


def _find_session_jsonls(home: Path) -> list[Path]:
    out = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in home.glob(pattern):
            if p.is_file() and p not in out:
                out.append(p)
    return out


def _run_prompt(ctx, prompt: str, *, extra_env: dict | None = None,
                extra_args: list[str] | None = None, timeout: int = 180) -> dict:
    env = _make_env(ctx, extra=extra_env)
    proj = ctx.root_dir / "fake_project"
    proj.mkdir(parents=True, exist_ok=True)

    cmd = [str(ROOT / "run-mossen.sh"), "-p", *(extra_args or [])]
    proc = subprocess.run(
        cmd, input=prompt, env=env, capture_output=True,
        text=True, timeout=timeout, cwd=str(proj),
    )
    write_command_log(ctx, ["mossen", "-p", *(extra_args or [])],
                      proc.stdout, proc.stderr, proc.returncode)
    sessions = _find_session_jsonls(ctx.home_dir)
    return {
        "exit_code": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "session_count": len(sessions),
        "session_paths": [str(p) for p in sessions[:3]],
        "session_landed": len(sessions) > 0,
    }


# ============================================================================
# Sub-case 1: R9.compact — 强制对话 + marker, 验 exit 0 + session 落盘
# (G4-1 后切 STRICT: autocompact 真触发, isCompactSummary 出现, marker 跨 compact)
# ============================================================================
def case_compact() -> dict:
    ctx = make_fixture("R9_compact")
    marker = "R9_COMPACT_MARKER"
    prompt = (
        f"请把以下字符串原样回复给我: {marker} 这是一段长文本用以塞入对话上下文 "
        + "x" * 200
    )
    res = _run_prompt(ctx, prompt)
    base_ok = res["exit_code"] == 0 and res["session_landed"]
    return {
        "name": "R9.compact",
        "ok": base_ok,
        "weak_mode": WEAK_MODE,
        "exit_code": res["exit_code"],
        "session_landed": res["session_landed"],
        "marker": marker,
        "marker_in_stdout": marker in res["stdout"],
        "stdout_excerpt": res["stdout"][:300],
        "stderr_excerpt": res["stderr"][:300],
        "_ctx": ctx,
    }


# ============================================================================
# Sub-case 2: R9.memory — 验 mossen 启动 + memory 路径不破
# (G4-2 后切 STRICT: write memory → restart → marker 在 P2 reply 出现)
# ============================================================================
def case_memory() -> dict:
    ctx = make_fixture("R9_memory")
    marker = "R9_MEMORY_MARKER"
    prompt = f"请回答: {marker}"
    res = _run_prompt(ctx, prompt)
    base_ok = res["exit_code"] == 0 and res["session_landed"]
    return {
        "name": "R9.memory",
        "ok": base_ok,
        "weak_mode": WEAK_MODE,
        "exit_code": res["exit_code"],
        "session_landed": res["session_landed"],
        "marker": marker,
        "marker_in_stdout": marker in res["stdout"],
        "stdout_excerpt": res["stdout"][:300],
        "stderr_excerpt": res["stderr"][:300],
        "_ctx": ctx,
    }


# ============================================================================
# Sub-case 3: R9.permission — 验 permission gate 不破 (--permission-mode acceptEdits)
# (G4-3 后切 STRICT: 3 模式真生效, edit/rm/危险操作行为正确)
# ============================================================================
def case_permission() -> dict:
    ctx = make_fixture("R9_permission")
    marker = "R9_PERMISSION_MARKER"
    prompt = f"请把以下字符串原样回复给我: {marker}"
    res = _run_prompt(ctx, prompt,
                      extra_args=["--permission-mode", "acceptEdits"])
    base_ok = res["exit_code"] == 0 and res["session_landed"]
    return {
        "name": "R9.permission",
        "ok": base_ok,
        "weak_mode": WEAK_MODE,
        "exit_code": res["exit_code"],
        "session_landed": res["session_landed"],
        "marker": marker,
        "marker_in_stdout": marker in res["stdout"],
        "stdout_excerpt": res["stdout"][:300],
        "stderr_excerpt": res["stderr"][:300],
        "_ctx": ctx,
    }


# ============================================================================
# Sub-case 4: R9.model — 验 --model override 不破 (custom backend 配置下传 model 名)
# (G4-4 后切 STRICT: session jsonl 首条 message.model == 指定的 model 名)
# ============================================================================
def case_model() -> dict:
    ctx = make_fixture("R9_model")
    marker = "R9_MODEL_MARKER"
    prompt = f"请把以下字符串原样回复给我: {marker}"
    # 使用 custom-backend.env 已配置的 model (Qwen3.6 Plus, env 已 source 进 mossen)
    res = _run_prompt(ctx, prompt)

    # 解析 session jsonl 找 model 字段
    model_in_session = None
    if res["session_paths"]:
        try:
            for line in Path(res["session_paths"][0]).read_text().splitlines():
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(rec, dict) and "model" in rec:
                    model_in_session = rec["model"]
                    break
                if isinstance(rec, dict) and "message" in rec:
                    msg = rec["message"]
                    if isinstance(msg, dict) and "model" in msg:
                        model_in_session = msg["model"]
                        break
        except OSError:
            pass

    base_ok = res["exit_code"] == 0 and res["session_landed"]
    return {
        "name": "R9.model",
        "ok": base_ok,
        "weak_mode": WEAK_MODE,
        "exit_code": res["exit_code"],
        "session_landed": res["session_landed"],
        "marker": marker,
        "marker_in_stdout": marker in res["stdout"],
        "model_in_session": model_in_session,
        "stdout_excerpt": res["stdout"][:300],
        "stderr_excerpt": res["stderr"][:300],
        "_ctx": ctx,
    }


def _retry(case_fn, n=3):
    res = None
    for i in range(n):
        res = case_fn()
        if res.get("ok"):
            res["_attempt"] = i + 1
            return res
        res["_attempt"] = i + 1
    return res


def main() -> int:
    cases = [
        ("compact", case_compact),
        ("memory", case_memory),
        ("permission", case_permission),
        ("model", case_model),
    ]

    # 允许 --only sub:NAME 选择子 case
    only = None
    for arg in sys.argv[1:]:
        if arg.startswith("--only="):
            only = arg.split("=", 1)[1]
        elif arg == "--list":
            for n, _ in cases:
                print(n)
            return 0
    if only:
        cases = [(n, fn) for n, fn in cases if n == only]
        if not cases:
            print(json.dumps({"error": f"unknown sub-case: {only}"}, indent=2))
            return 2

    results = []
    for name, fn in cases:
        res = _retry(fn)
        ctx = res.pop("_ctx")
        write_assertions(
            ctx,
            status="passed" if res.get("ok") else "failed",
            assertions=[{
                "name": res["name"],
                "expected": True,
                "actual": res.get("ok"),
                "passed": res.get("ok"),
                "evidence": (
                    f"exit={res.get('exit_code')} "
                    f"session_landed={res.get('session_landed')} "
                    f"marker_in_stdout={res.get('marker_in_stdout')} "
                    f"weak_mode={res.get('weak_mode')}"
                ),
            }],
        )
        results.append(res)

    overall_ok = all(r.get("ok") for r in results)
    print(json.dumps({
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "R9 (G2-2d framework, full weak): 4 sub-case (compact/memory/permission/model). "
            "G4-x 各 slice 后逐域切 STRICT; G7 收口 8 维 deep_eq baseline."
        ),
    }, indent=2, ensure_ascii=False, default=str))
    return 0 if overall_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
