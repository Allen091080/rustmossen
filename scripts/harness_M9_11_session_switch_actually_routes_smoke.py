#!/usr/bin/env python3
"""
M9.11 — `/model <name>` session 切换必须真接入 LLM 请求路径.

S1-09 回归 bug (Allen 2026-04-28 报):
  /model glm 显示切换成功, 但 mainLoopModel 仍是 startup 时 setMainLoopModelOverride 设的
  旧值 (qwen3.6-plus). 请求落到 glm baseURL 但 model 字段是 qwen 名 → 后端拒绝/串.

修复: commands/model/model.tsx setSessionActiveProfile 之后追加 setMainLoopModelOverride(result.profile.model).

守护契约:
  case 1 — startup mainLoop=qwen + /model glm
           → mainLoopModel=glm-5.1, customBackend.baseUrl=glm
  case 2 — case 1 之后 /model qwen → mainLoopModel=qwen3.6-plus, baseUrl=qwen (回归)
  case 3 — /model minimax → mainLoopModel=MiniMax-M2.7, baseUrl=minimax
  case 4 — /model glm 后, 新进程 (= "新会话") 启动仍 mainLoopModel=qwen (全局默认未动)
  case 5 — settings.activeProfile 仍是 qwen (session 切换不写 settings)
  case 6 — apiKey 不在 stdout/stderr 任何位置
  case 7 — context.setAppState 提供时, /model glm 必须调用它并把
           mainLoopModelForSession 设为 result.profile.model.
           (Allen 2026-04-28 二次回归: 前两层补了 statusline 仍显示旧 model,
            因为 useMainLoopModel 读 React AppState, 是第三层 source of truth.)

反测信号:
  - setMainLoopModelOverride 没被调 → case 1/2/3 mainLoopModel 不变 fail
  - setSessionActiveProfile 把 settings 写脏 → case 5 fail
  - 新进程读到 session override → case 4 fail (overrun process boundary)
  - setAppState 没调用 → case 7 captured.length==0 fail
  - setAppState 写错字段 (mainLoopModel 而不是 mainLoopModelForSession) → case 7 fail
    (写 mainLoopModel 会触发 onChangeAppState 把 session-only 切换写到 settings.json)
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


QWEN_BASE = "https://fake-qwen.example/v1"
QWEN_KEY = "sk-fake-qwen-test-AAAAAAAAAAAAAAAAAAAAAAAAAAA"
QWEN_MODEL = "qwen3.6-plus"

GLM_BASE = "https://fake-glm.example/v1"
GLM_KEY = "sk-fake-glm-test-BBBBBBBBBBBBBBBBBBBBBBBBBBB"
GLM_MODEL = "glm-5.1"

MINIMAX_BASE = "https://fake-minimax.example/v1"
MINIMAX_KEY = "sk-fake-minimax-test-CCCCCCCCCCCCCCCCCCCCCCC"
MINIMAX_MODEL = "MiniMax-M2.7"


def _make_env(ctx) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    for k in (
        "MOSSEN_CODE_USE_CUSTOM_BACKEND",
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_CODE_CUSTOM_NAME",
        "MOSSEN_CONFIG_OVERRIDES",
        "MOSSEN_INTERNAL_FC_OVERRIDES",
    ):
        env.pop(k, None)
    return env


def _seed_three_profiles(home: Path, active: str = "qwen") -> None:
    home.mkdir(parents=True, exist_ok=True)
    payload = {
        "mossen.profiles": {
            "qwen": {
                "provider": "openai-compatible",
                "baseURL": QWEN_BASE, "model": QWEN_MODEL, "apiKey": QWEN_KEY,
            },
            "glm": {
                "provider": "openai-compatible",
                "baseURL": GLM_BASE, "model": GLM_MODEL, "apiKey": GLM_KEY,
            },
            "minimax": {
                "provider": "openai-compatible",
                "baseURL": MINIMAX_BASE, "model": MINIMAX_MODEL, "apiKey": MINIMAX_KEY,
            },
        },
        "mossen.activeProfile": active,
    }
    (home / "settings.json").write_text(json.dumps(payload, indent=2) + "\n")


def _bun_session_switch(env: dict, switches: list[str]) -> tuple[int, str, str]:
    """
    单个 bun 进程模拟 REPL 内多次 /model 切换. 每次 switch 后 dump:
      - mainLoopModel (utils/model/model.ts:getMainLoopModel)
      - customBackend.baseUrl
      - customBackend.apiKey 长度 (不 dump 真值)
    """
    snippet = (
        "const m = await import('./commands/model/model.tsx');"
        "const modelM = await import('./utils/model/model.ts');"
        "const cb = await import('./utils/customBackend.ts');"
        "const stateM = await import('./bootstrap/state.ts');"
        "const profilesM = await import('./services/config/profiles.ts');"
        # 模拟 startup setMainLoopModelOverride: 取 customBackend.model (即 active profile model)
        "stateM.setMainLoopModelOverride(cb.getCustomBackendModel());"
        "const trace = [];"
        "trace.push({phase: 'startup', mainLoop: modelM.getMainLoopModel(),"
        "  baseUrl: cb.getCustomBackendBaseUrl(), apiKeyLen: (cb.getCustomBackendApiKey()||'').length,"
        "  current: profilesM.getCurrentProfile()?.name});"
        + "".join(
            f"await m.call({json.dumps(name)}, {{}});"
            f"trace.push({{phase: 'after_/model_{name}', mainLoop: modelM.getMainLoopModel(),"
            f"  baseUrl: cb.getCustomBackendBaseUrl(), apiKeyLen: (cb.getCustomBackendApiKey()||'').length,"
            f"  current: profilesM.getCurrentProfile()?.name}});"
            for name in switches
        )
        + "process.stdout.write(JSON.stringify(trace));"
    )
    proc = subprocess.run(
        ["bun", "-e", snippet],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    return proc.returncode, proc.stdout, proc.stderr


def case_1_2_3_switch_glm_qwen_minimax(ctx) -> list[dict]:
    proj = ctx.root_dir / "case123"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_three_profiles(home, active="qwen")
    env = _make_env(ctx)

    rc, out, err = _bun_session_switch(env, ["glm", "qwen", "minimax"])
    write_command_log(ctx, ["bun", "session switch glm/qwen/minimax", "(case 1-3)"], out, err, rc)

    try:
        trace = json.loads(out)
    except Exception:
        trace = None

    apikey_leak = (QWEN_KEY in out) or (GLM_KEY in out) or (MINIMAX_KEY in out)
    if not trace or len(trace) != 4:
        return [{"name": "case1_2_3_switch_routes", "ok": False, "exit_code": rc,
                 "stdout_excerpt": out[:400], "trace_len": len(trace) if trace else None}]

    startup, after_glm, after_qwen, after_minimax = trace
    return [
        {
            "name": "case1_switch_glm_routes_to_glm",
            "ok": (
                rc == 0
                and after_glm["mainLoop"] == GLM_MODEL
                and after_glm["baseUrl"] == GLM_BASE
                and after_glm["current"] == "glm"
                and after_glm["apiKeyLen"] == len(GLM_KEY)
                and not apikey_leak
            ),
            "trace": after_glm,
            "expected_mainLoop": GLM_MODEL,
            "expected_baseUrl": GLM_BASE,
        },
        {
            "name": "case2_switch_back_to_qwen_routes_to_qwen",
            "ok": (
                rc == 0
                and after_qwen["mainLoop"] == QWEN_MODEL
                and after_qwen["baseUrl"] == QWEN_BASE
                and after_qwen["current"] == "qwen"
                and after_qwen["apiKeyLen"] == len(QWEN_KEY)
                and not apikey_leak
            ),
            "trace": after_qwen,
        },
        {
            "name": "case3_switch_minimax_routes_to_minimax",
            "ok": (
                rc == 0
                and after_minimax["mainLoop"] == MINIMAX_MODEL
                and after_minimax["baseUrl"] == MINIMAX_BASE
                and after_minimax["current"] == "minimax"
                and after_minimax["apiKeyLen"] == len(MINIMAX_KEY)
                and not apikey_leak
            ),
            "trace": after_minimax,
        },
    ]


def case_4_new_process_uses_global_default(ctx) -> dict:
    """A 进程切到 glm; B 新进程启动 → 应读 settings.activeProfile=qwen, 不知道 A 切过 glm."""
    proj = ctx.root_dir / "case4"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_three_profiles(home, active="qwen")
    env = _make_env(ctx)

    # A 进程切到 glm, dump
    rc_a, out_a, err_a = _bun_session_switch(env, ["glm"])
    # B 进程启动, 不切, 直接 dump (模拟"新会话")
    rc_b, out_b, err_b = _bun_session_switch(env, [])
    write_command_log(ctx, ["bun", "A switch glm + B fresh", "(case 4)"], out_a + "\n---\n" + out_b, err_a + err_b, rc_b)

    try:
        b_trace = json.loads(out_b)
    except Exception:
        b_trace = None

    apikey_leak = (QWEN_KEY in out_b) or (GLM_KEY in out_b)
    if not b_trace or len(b_trace) != 1:
        return {"name": "case4_new_process_uses_global_default", "ok": False,
                "exit_code_b": rc_b, "stdout_excerpt": out_b[:400]}

    b_startup = b_trace[0]
    return {
        "name": "case4_new_process_uses_global_default",
        "ok": (
            rc_a == 0 and rc_b == 0
            and b_startup["mainLoop"] == QWEN_MODEL  # 新进程仍 qwen, 不受 A 进程切 glm 影响
            and b_startup["baseUrl"] == QWEN_BASE
            and b_startup["current"] == "qwen"
            and not apikey_leak
        ),
        "b_startup": b_startup,
        "expected_mainLoop": QWEN_MODEL,
        "expected_baseUrl": QWEN_BASE,
    }


def case_5_settings_activeProfile_unchanged_after_switch(ctx) -> dict:
    """session 切换不写 settings.json; activeProfile 字段仍是 qwen."""
    proj = ctx.root_dir / "case5"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_three_profiles(home, active="qwen")
    env = _make_env(ctx)

    pre = json.loads((home / "settings.json").read_text())
    rc, out, err = _bun_session_switch(env, ["glm", "minimax"])
    post = json.loads((home / "settings.json").read_text())
    write_command_log(ctx, ["bun", "session switch + check settings", "(case 5)"], out, err, rc)

    return {
        "name": "case5_settings_activeProfile_unchanged_after_session_switch",
        "ok": (
            rc == 0
            and post.get("mossen.activeProfile") == "qwen"
            and post.get("mossen.profiles", {}).keys() == pre.get("mossen.profiles", {}).keys()
        ),
        "active_pre": pre.get("mossen.activeProfile"),
        "active_post": post.get("mossen.activeProfile"),
    }


def case_6_apikey_never_in_output_across_session_switches(ctx) -> dict:
    proj = ctx.root_dir / "case6"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_three_profiles(home, active="qwen")
    env = _make_env(ctx)

    rc, out, err = _bun_session_switch(env, ["glm", "qwen", "minimax", "qwen"])
    write_command_log(ctx, ["bun", "switch x4", "(case 6 leak check)"], out, err, rc)
    leaked = QWEN_KEY in out or QWEN_KEY in err or GLM_KEY in out or GLM_KEY in err or MINIMAX_KEY in out or MINIMAX_KEY in err
    return {
        "name": "case6_apikey_never_in_output_across_session_switches",
        "ok": rc == 0 and not leaked,
        "leaked": leaked,
        "out_len": len(out),
        "err_len": len(err),
    }


def case_7_setAppState_called_with_session_model(ctx) -> dict:
    """
    Allen 2026-04-28 二次回归: /model glm 显示成功 + customBackend 切了, 但 statusline
    仍 qwen. 根因: useMainLoopModel 读 React AppState.mainLoopModelForSession, 是
    第三层 source of truth, /model 必须也更新它.

    本 case 用 stub setAppState 抓所有调用, 验:
      1) /model glm 之后 setAppState 至少被调 1 次
      2) reducer 输出包含 mainLoopModelForSession=glm-5.1
      3) **不能**写 mainLoopModel (那是 startup-time / settings 持久字段,
         写它会触发 onChangeAppState 把 session 切换持久化到 settings.json)
    """
    proj = ctx.root_dir / "case7"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_three_profiles(home, active="qwen")
    env = _make_env(ctx)

    snippet = (
        "const m = await import('./commands/model/model.tsx');"
        "const captured = [];"
        # stub AppState reducer: 每次 setAppState 调用都跑 reducer(prev) 把结果存到 captured
        "const fakePrev = {"
        "  mainLoopModel: 'qwen3.6-plus',"
        "  mainLoopModelForSession: null,"
        "};"
        "const ctx = {"
        "  setAppState: (fn) => { const next = fn(fakePrev); captured.push(next); }"
        "};"
        "await m.call('glm', ctx);"
        "process.stdout.write(JSON.stringify({"
        "  callCount: captured.length,"
        "  lastNext: captured[captured.length - 1] || null,"
        "}));"
    )
    proc = subprocess.run(
        ["bun", "-e", snippet],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    write_command_log(ctx, ["bun", "stub setAppState capture", "(case 7)"], proc.stdout, proc.stderr, proc.returncode)

    try:
        data = json.loads(proc.stdout)
    except Exception:
        data = None

    apikey_leak = (QWEN_KEY in proc.stdout) or (GLM_KEY in proc.stdout)
    if not data:
        return {"name": "case7_setAppState_called_with_session_model", "ok": False,
                "exit_code": proc.returncode, "stdout_excerpt": proc.stdout[:400],
                "stderr_excerpt": proc.stderr[:400]}

    last = data.get("lastNext") or {}
    return {
        "name": "case7_setAppState_called_with_session_model",
        "ok": (
            proc.returncode == 0
            and data.get("callCount", 0) >= 1
            and last.get("mainLoopModelForSession") == GLM_MODEL
            # 反测: 不能写 mainLoopModel (持久字段); 必须仍是 fakePrev 原值
            and last.get("mainLoopModel") == "qwen3.6-plus"
            and not apikey_leak
        ),
        "callCount": data.get("callCount"),
        "lastNext": last,
        "expected_mainLoopForSession": GLM_MODEL,
    }


def main() -> int:
    ctx = make_fixture("M9.11_session_switch_actually_routes")
    results = case_1_2_3_switch_glm_qwen_minimax(ctx) + [
        case_4_new_process_uses_global_default(ctx),
        case_5_settings_activeProfile_unchanged_after_switch(ctx),
        case_6_apikey_never_in_output_across_session_switches(ctx),
        case_7_setAppState_called_with_session_model(ctx),
    ]
    all_ok = all(r["ok"] for r in results)
    write_assertions(ctx, status="passed" if all_ok else "failed", assertions=results)
    print(json.dumps({
        "results": results,
        "passed": sum(1 for r in results if r["ok"]),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M9.11 (S1-09 回归修复): /model <name> 必须同时 setMainLoopModelOverride, 让 LLM 请求路径真用新 profile.",
    }, indent=2, ensure_ascii=False))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
