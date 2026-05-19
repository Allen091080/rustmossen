#!/usr/bin/env python3
"""
M9.9 — fallback profile (env-based qwen) 在 /model + --list-model-profiles 中可见 + 可切回.

S1-09 闭环: 旧 .mossensrc/custom-backend.env / MOSSEN_CODE_CUSTOM_* fallback 之前不在
profile 列表里. 用户切到 glm/minimax 后无法从 /model 看到并切回 qwen. 本测试守护:
  1. fallback qwen 出现在 /model 列表 (带 [fallback] tag)
  2. fallback qwen 出现在 --list-model-profiles JSON (allProfiles + fallbackProfile 字段)
  3. /model qwen 能从 glm 切回 qwen (session-only)
  4. 切回后 currentProfile 不再是 null/glm, 是 qwen + source=fallback-env
  5. 切回后 customBackend 实际数据流走 env (不动 customBackend.ts)
  6. apiKey 在 stdout 全脱敏 (mask 前 6 + ... + 后 4)

反测信号:
  - profiles.ts listAllProfiles 漏 fallback → case 1/2 fail
  - setSessionActiveProfile 不识别 fallback name → case 3 抛异常
  - getCurrentProfile 不 fallthrough → case 4 显示 null
  - 误把 fallback profile 写进 settings.json → case 5 next session 仍读到
  - apiKey 未脱敏 → case 6 fail (raw key 出现在 stdout)
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


FAKE_ENV_BASEURL = "https://fake-qwen.example/v1"
FAKE_ENV_APIKEY = "sk-fake-qwen-test-AAAAAAAAAAAAAAAAAAAAAAAAAAA"
FAKE_ENV_MODEL = "qwen3.6-plus"

FAKE_GLM_BASEURL = "https://fake-glm.example/v1"
FAKE_GLM_APIKEY = "sk-fake-glm-test-BBBBBBBBBBBBBBBBBBBBBBBBBBB"
FAKE_GLM_MODEL = "glm-test"


def _make_env(ctx, *, with_fallback: bool = True) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env.pop("MOSSEN_CONFIG_OVERRIDES", None)
    env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    if with_fallback:
        env["MOSSEN_CODE_USE_CUSTOM_BACKEND"] = "true"
        env["MOSSEN_CODE_CUSTOM_BASE_URL"] = FAKE_ENV_BASEURL
        env["MOSSEN_CODE_CUSTOM_API_KEY"] = FAKE_ENV_APIKEY
        env["MOSSEN_CODE_CUSTOM_MODEL"] = FAKE_ENV_MODEL
    else:
        for k in (
            "MOSSEN_CODE_USE_CUSTOM_BACKEND",
            "MOSSEN_CODE_CUSTOM_BASE_URL",
            "MOSSEN_CODE_CUSTOM_API_KEY",
            "MOSSEN_CODE_CUSTOM_MODEL",
            "MOSSEN_CODE_CUSTOM_NAME",
        ):
            env.pop(k, None)
    return env


def _run_bun(env: dict, code: str, cwd: Path | None = None) -> tuple[int, str, str]:
    proc = subprocess.run(
        ["bun", "-e", code],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(cwd or ROOT),
    )
    return proc.returncode, proc.stdout, proc.stderr


def _run_list_cli(env: dict, cwd: Path) -> tuple[int, str, str]:
    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "--list-model-profiles"],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(cwd),
    )
    return proc.returncode, proc.stdout, proc.stderr


def _seed_glm_profile(home: Path) -> None:
    """直接写 settings.json 模拟用户已通过 --add-model-profile 加了 glm."""
    settings = home / "settings.json"
    settings.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "mossen.profiles": {
            "glm": {
                "provider": "openai-compatible",
                "baseURL": FAKE_GLM_BASEURL,
                "model": FAKE_GLM_MODEL,
                "apiKey": FAKE_GLM_APIKEY,
            }
        },
        "mossen.activeProfile": "glm",
    }
    settings.write_text(json.dumps(payload, indent=2) + "\n")


def case_1_fallback_in_model_list(ctx) -> dict:
    """fallback qwen 出现在 /model 列表, 带 [fallback] tag, 0 settings profiles."""
    proj = ctx.root_dir / "case1"
    proj.mkdir(parents=True, exist_ok=True)
    env = _make_env(ctx, with_fallback=True)

    code = (
        "const m = await import('./commands/model/model.tsx');"
        "const r = await m.call('', {});"
        "process.stdout.write(r.value);"
    )
    rc, out, err = _run_bun(env, code, cwd=ROOT)
    write_command_log(ctx, ["bun", "/model_list", "(case 1 only fallback)"], out, err, rc)

    has_qwen = "qwen" in out
    has_fallback_tag = "[fallback" in out or "fallback]" in out
    has_session_tag_on_qwen = bool(re.search(r"qwen \[.*session.*\]", out))
    apikey_leaked = FAKE_ENV_APIKEY in out
    return {
        "name": "case1_fallback_in_model_list",
        "ok": rc == 0 and has_qwen and has_fallback_tag and has_session_tag_on_qwen and not apikey_leaked,
        "exit_code": rc,
        "has_qwen": has_qwen,
        "has_fallback_tag": has_fallback_tag,
        "has_session_tag_on_qwen": has_session_tag_on_qwen,
        "apikey_leaked": apikey_leaked,
        "stdout_excerpt": out[:600],
    }


def case_2_fallback_in_list_cli_json(ctx) -> dict:
    """--list-model-profiles JSON 包含 fallbackProfile + allProfiles 含 fallback."""
    proj = ctx.root_dir / "case2"
    proj.mkdir(parents=True, exist_ok=True)
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run_list_cli(env, cwd=proj)
    write_command_log(ctx, ["mossen", "--list-model-profiles", "(case 2 fallback only)"], out, err, rc)

    try:
        # mossen 可能 emit warning 行在 JSON 前; 抓最后一段 JSON
        json_start = out.find("{")
        data = json.loads(out[json_start:]) if json_start >= 0 else None
    except Exception:
        data = None

    if data is None:
        return {
            "name": "case2_fallback_in_list_cli_json",
            "ok": False, "exit_code": rc, "reason": "could not parse JSON",
            "stdout_excerpt": out[:400],
        }
    fb = data.get("fallbackProfile")
    cur = data.get("currentProfile")
    all_p = data.get("allProfiles", [])
    fb_in_all = any(p.get("source") == "fallback-env" for p in all_p)
    apikey_leaked = FAKE_ENV_APIKEY in out
    fb_apikey = (fb or {}).get("profile", {}).get("apiKey", "")
    fb_apikey_masked = bool(fb_apikey and fb_apikey != FAKE_ENV_APIKEY and "..." in fb_apikey)
    return {
        "name": "case2_fallback_in_list_cli_json",
        "ok": (
            rc == 0
            and fb is not None
            and fb.get("name") == "qwen"
            and fb.get("source") == "fallback-env"
            and cur is not None
            and cur.get("name") == "qwen"
            and cur.get("source") == "fallback-env"
            and fb_in_all
            and not apikey_leaked
            and fb_apikey_masked
        ),
        "exit_code": rc,
        "fb_present": fb is not None,
        "fb_name": (fb or {}).get("name"),
        "fb_source": (fb or {}).get("source"),
        "current_name": (cur or {}).get("name"),
        "current_source": (cur or {}).get("source"),
        "fb_in_all_profiles": fb_in_all,
        "fb_apikey_masked": fb_apikey_masked,
        "apikey_leaked": apikey_leaked,
    }


def case_3_switch_glm_to_fallback_qwen(ctx) -> dict:
    """settings 里 active=glm, /model qwen 切回 fallback; getCurrentProfile 应是 qwen."""
    proj = ctx.root_dir / "case3"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    home.mkdir(parents=True, exist_ok=True)
    _seed_glm_profile(home)
    env = _make_env(ctx, with_fallback=True)

    code = (
        "const m = await import('./commands/model/model.tsx');"
        "const profilesM = await import('./services/config/profiles.ts');"
        "const before = profilesM.getCurrentProfile();"
        "const r = await m.call('qwen', {});"
        "const after = profilesM.getCurrentProfile();"
        "const out = {"
        "  switch_text: r.value,"
        "  before_name: before ? before.name : null,"
        "  before_source: before ? before.source : null,"
        "  after_name: after ? after.name : null,"
        "  after_source: after ? after.source : null,"
        "};"
        "process.stdout.write(JSON.stringify(out));"
    )
    rc, out, err = _run_bun(env, code, cwd=ROOT)
    write_command_log(ctx, ["bun", "/model qwen", "(case 3 switch back)"], out, err, rc)

    try:
        data = json.loads(out)
    except Exception:
        return {"name": "case3_switch_glm_to_fallback_qwen", "ok": False, "exit_code": rc, "stdout_excerpt": out[:400]}

    apikey_leaked = FAKE_ENV_APIKEY in out
    return {
        "name": "case3_switch_glm_to_fallback_qwen",
        "ok": (
            rc == 0
            and data["before_name"] == "glm"
            and data["before_source"] == "settings"
            and data["after_name"] == "qwen"
            and data["after_source"] == "fallback-env"
            and "Switched session profile to" in data["switch_text"]
            and not apikey_leaked
        ),
        "exit_code": rc,
        "before_name": data.get("before_name"),
        "before_source": data.get("before_source"),
        "after_name": data.get("after_name"),
        "after_source": data.get("after_source"),
        "apikey_leaked": apikey_leaked,
        "switch_excerpt": data.get("switch_text", "")[:400],
    }


def case_4_customBackend_falls_to_env_after_switch(ctx) -> dict:
    """切回 fallback 后, customBackend.getCustomBackendBaseUrl/ApiKey/Model 走 env, 不是 glm."""
    proj = ctx.root_dir / "case4"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_glm_profile(home)
    env = _make_env(ctx, with_fallback=True)

    code = (
        "const profilesM = await import('./services/config/profiles.ts');"
        "const cb = await import('./utils/customBackend.ts');"
        "const before = {"
        "  base: cb.getCustomBackendBaseUrl(),"
        "  model: cb.getCustomBackendModel(),"
        "  key_present: !!cb.getCustomBackendApiKey(),"
        "};"
        "profilesM.setSessionActiveProfile('qwen');"
        "const after = {"
        "  base: cb.getCustomBackendBaseUrl(),"
        "  model: cb.getCustomBackendModel(),"
        "  key_present: !!cb.getCustomBackendApiKey(),"
        "};"
        "process.stdout.write(JSON.stringify({before, after}));"
    )
    rc, out, err = _run_bun(env, code, cwd=ROOT)
    write_command_log(ctx, ["bun", "customBackend after switch", "(case 4)"], out, err, rc)

    try:
        data = json.loads(out)
    except Exception:
        return {"name": "case4_customBackend_falls_to_env", "ok": False, "exit_code": rc, "stdout_excerpt": out[:400]}

    apikey_leaked = FAKE_ENV_APIKEY in out or FAKE_GLM_APIKEY in out
    before_was_glm = data["before"]["base"] == FAKE_GLM_BASEURL and data["before"]["model"] == FAKE_GLM_MODEL
    after_is_env = data["after"]["base"] == FAKE_ENV_BASEURL and data["after"]["model"] == FAKE_ENV_MODEL
    return {
        "name": "case4_customBackend_falls_to_env",
        "ok": rc == 0 and before_was_glm and after_is_env and data["after"]["key_present"] and not apikey_leaked,
        "exit_code": rc,
        "before": data.get("before"),
        "after": data.get("after"),
        "before_was_glm": before_was_glm,
        "after_is_env": after_is_env,
        "apikey_leaked": apikey_leaked,
    }


def case_5_set_default_to_fallback_clears_user_active(ctx) -> dict:
    """mossen --set-model-profile qwen (fallback name) → 清掉 user-scope active.

    旧 active=glm, 设置后 settings.json 里 mossen.activeProfile 应为 null
    (这样下次启动 customBackend 落到 fallback). settings.json 上的 glm profile 不动.
    """
    proj = ctx.root_dir / "case5"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_glm_profile(home)
    env = _make_env(ctx, with_fallback=True)

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "--set-model-profile", "qwen"],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(proj),
    )
    write_command_log(ctx, ["mossen", "--set-model-profile", "qwen", "(case 5)"], proc.stdout, proc.stderr, proc.returncode)

    settings = home / "settings.json"
    after = json.loads(settings.read_text())
    glm_still_present = "glm" in (after.get("mossen.profiles") or {})
    active_now = after.get("mossen.activeProfile")
    apikey_leaked = FAKE_ENV_APIKEY in proc.stdout or FAKE_GLM_APIKEY in proc.stdout

    # JSON 输出验证
    try:
        json_start = proc.stdout.find("{")
        data = json.loads(proc.stdout[json_start:]) if json_start >= 0 else None
    except Exception:
        data = None

    return {
        "name": "case5_set_default_to_fallback",
        "ok": (
            proc.returncode == 0
            and active_now is None  # 已清空 (fallback active)
            and glm_still_present  # glm profile 没被删
            and data is not None
            and data.get("source") == "fallback-env"
            and data.get("activeProfile") == "qwen"
            and not apikey_leaked
        ),
        "exit_code": proc.returncode,
        "active_after_set": active_now,
        "glm_still_present": glm_still_present,
        "json_source": (data or {}).get("source"),
        "json_active": (data or {}).get("activeProfile"),
        "apikey_leaked": apikey_leaked,
        "stdout_excerpt": proc.stdout[:400],
    }


def case_6_no_fallback_when_env_unset(ctx) -> dict:
    """env 未设置时, fallbackProfile / allProfiles fallback entry 都不出现.

    NOTE: 不能走 run-mossen.sh 包装 (会 source .mossensrc/custom-backend.env 注入真 qwen env).
    直接走 bun 启动 entrypoints/cli.tsx.
    """
    proj = ctx.root_dir / "case6"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    settings = home / "settings.json"
    if settings.exists():
        settings.unlink()
    env = _make_env(ctx, with_fallback=False)

    proc = subprocess.run(
        ["bun", "run", "entrypoints/cli.tsx", "--list-model-profiles"],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(ROOT),
    )
    out, err, rc = proc.stdout, proc.stderr, proc.returncode
    write_command_log(ctx, ["bun", "entrypoints/cli.tsx", "--list-model-profiles", "(case 6 no env)"], out, err, rc)

    try:
        json_start = out.find("{")
        data = json.loads(out[json_start:]) if json_start >= 0 else None
    except Exception:
        data = None

    return {
        "name": "case6_no_fallback_when_env_unset",
        "ok": (
            rc == 0
            and data is not None
            and data.get("fallbackProfile") is None
            and data.get("currentProfile") is None
            and data.get("countAll") == 0
        ),
        "exit_code": rc,
        "fallbackProfile": (data or {}).get("fallbackProfile"),
        "currentProfile": (data or {}).get("currentProfile"),
        "countAll": (data or {}).get("countAll"),
    }


def case_7_fallback_hidden_when_settings_nonempty(ctx) -> dict:
    """env + glm settings → allProfiles[] 不含 fallback (S1-09 收口政策).

    fallbackProfile 字段仍反映 env 真实存在 (供 UI 检测迁移机会),
    但不进 allProfiles[] 主列表, 避免 fallback 成为正常主路径.
    """
    proj = ctx.root_dir / "case7"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_glm_profile(home)
    env = _make_env(ctx, with_fallback=True)

    rc, out, err = _run_list_cli(env, cwd=proj)
    write_command_log(ctx, ["mossen", "--list-model-profiles", "(case 7 hide fallback)"], out, err, rc)

    try:
        json_start = out.find("{")
        data = json.loads(out[json_start:]) if json_start >= 0 else None
    except Exception:
        data = None

    if data is None:
        return {"name": "case7_fallback_hidden_when_settings_nonempty", "ok": False,
                "exit_code": rc, "stdout_excerpt": out[:400]}

    all_p = data.get("allProfiles", [])
    fb_in_all = any(p.get("source") == "fallback-env" for p in all_p)
    fb_in_field = data.get("fallbackProfile") is not None
    only_glm = len(all_p) == 1 and all_p[0].get("name") == "glm" and all_p[0].get("source") == "settings"
    return {
        "name": "case7_fallback_hidden_when_settings_nonempty",
        "ok": rc == 0 and not fb_in_all and fb_in_field and only_glm,
        "exit_code": rc,
        "fb_in_allProfiles": fb_in_all,
        "fb_in_fallbackProfile_field": fb_in_field,
        "allProfiles_only_glm": only_glm,
        "all_count": len(all_p),
    }


def main() -> int:
    ctx = make_fixture("M9.9_fallback_visibility")
    results = [
        case_1_fallback_in_model_list(ctx),
        case_2_fallback_in_list_cli_json(ctx),
        case_3_switch_glm_to_fallback_qwen(ctx),
        case_4_customBackend_falls_to_env_after_switch(ctx),
        case_5_set_default_to_fallback_clears_user_active(ctx),
        case_6_no_fallback_when_env_unset(ctx),
        case_7_fallback_hidden_when_settings_nonempty(ctx),
    ]
    all_ok = all(r["ok"] for r in results)
    write_assertions(
        ctx,
        status="passed" if all_ok else "failed",
        assertions=results,
    )
    print(json.dumps({
        "results": results,
        "passed": sum(1 for r in results if r["ok"]),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M9.9 (S1-09 闭环): fallback profile 在 /model + --list-model-profiles 可见, 可切回, customBackend 实际数据流不变.",
    }, indent=2, ensure_ascii=False))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
