#!/usr/bin/env python3
"""
R6 — Mossen config local/project override 持久化安全网测试 (G2-2b).

按 GrowthBook迁移计划.md §1.3 + G0-5 测试矩阵设计.

守护契约:
  --set-mossen-config / --clear-mossen-config 真把值写到 settings.json:
  - case A: --scope user 写到 <MOSSEN_CONFIG_DIR>/settings.json (不是 project)
  - case B: --scope project 写到 <cwd>/.mossen/settings.json (不是 user)
  - case C: --get 在 set 之后读到值
  - case D: --clear 后文件里不再有该 key, --get 回退到 caller default
  - case E: 多 key 共存 — clear 一个不影响另一个

反测信号:
  - SettingsProviderBase.set 把 user 值写到 project 路径 → case A 文件验证 fail
  - clear 把整个 settings.json 删了 (本应只删一个 key) → case E 共存 fail
  - --scope 解析坏掉, 默认 fallback 到 override (in-memory) → 文件不存在 → case A fail
  - read 不读 MOSSEN_CONFIG_DIR env → user 值写到真实 ~/.mossen → 隔离失败 (fixture 兜底)
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
from lib.mossen_settings_fixture import (
    clear_all_overrides,
    read_project_settings,
    read_user_settings,
)


PROBE_KEY_A = "mossen.test.r6_probe_a"
PROBE_KEY_B = "mossen.test.r6_probe_b"
USER_VAL = "R6_USER_VAL"
PROJECT_VAL = "R6_PROJECT_VAL"
SECONDARY_VAL = 9999


def _make_env(ctx) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"
    env.pop("MOSSEN_CONFIG_OVERRIDES", None)
    env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    return env


def _run_mossen(env: dict, proj_dir: Path, *args: str) -> tuple[int, str, str]:
    proc = subprocess.run(
        [str(ROOT / "scripts" / "start-mossen.sh"), *args],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(proj_dir),
    )
    return proc.returncode, proc.stdout.strip(), proc.stderr


def _setup_proj(ctx) -> Path:
    proj = ctx.root_dir / "fake_project"
    proj.mkdir(parents=True, exist_ok=True)
    clear_all_overrides(ctx.mossen_config_home, proj)
    return proj


def case_user_set_persists(ctx) -> dict:
    """A: --set --scope user → 写到 user settings.json, --get 读到值."""
    proj = _setup_proj(ctx)
    env = _make_env(ctx)

    rc_set, _, err_set = _run_mossen(
        env, proj,
        "--set-mossen-config", PROBE_KEY_A, json.dumps(USER_VAL),
        "--scope", "user",
    )
    write_command_log(ctx, ["mossen", "--set", PROBE_KEY_A, USER_VAL, "user"],
                      "", err_set, rc_set)

    user_data = read_user_settings(ctx.mossen_config_home) or {}
    proj_data = read_project_settings(proj) or {}

    rc_get, out_get, err_get = _run_mossen(
        env, proj, "--get-mossen-config", PROBE_KEY_A,
    )
    write_command_log(ctx, ["mossen", "--get", PROBE_KEY_A], out_get, err_get, rc_get)

    try:
        actual = json.loads(out_get)
    except json.JSONDecodeError:
        actual = out_get

    file_ok = user_data.get(PROBE_KEY_A) == USER_VAL
    project_clean = PROBE_KEY_A not in proj_data
    get_ok = rc_get == 0 and actual == USER_VAL

    return {
        "scenario": "A_user_set_persists",
        "ok": rc_set == 0 and file_ok and project_clean and get_ok,
        "set_rc": rc_set,
        "user_settings_dump": user_data,
        "project_settings_dump": proj_data,
        "get_actual": actual,
        "checks": {
            "user_file_has_key": file_ok,
            "project_file_clean": project_clean,
            "get_returns_value": get_ok,
        },
    }


def case_project_set_persists(ctx) -> dict:
    """B: --set --scope project → 写到 project settings.json, --get 读到值, user 不污染."""
    proj = _setup_proj(ctx)
    env = _make_env(ctx)

    rc_set, _, err_set = _run_mossen(
        env, proj,
        "--set-mossen-config", PROBE_KEY_A, json.dumps(PROJECT_VAL),
        "--scope", "project",
    )
    write_command_log(ctx, ["mossen", "--set", PROBE_KEY_A, PROJECT_VAL, "project"],
                      "", err_set, rc_set)

    user_data = read_user_settings(ctx.mossen_config_home) or {}
    proj_data = read_project_settings(proj) or {}

    rc_get, out_get, _ = _run_mossen(env, proj, "--get-mossen-config", PROBE_KEY_A)
    try:
        actual = json.loads(out_get)
    except json.JSONDecodeError:
        actual = out_get

    file_ok = proj_data.get(PROBE_KEY_A) == PROJECT_VAL
    user_clean = PROBE_KEY_A not in user_data
    get_ok = rc_get == 0 and actual == PROJECT_VAL

    return {
        "scenario": "B_project_set_persists",
        "ok": rc_set == 0 and file_ok and user_clean and get_ok,
        "set_rc": rc_set,
        "user_settings_dump": user_data,
        "project_settings_dump": proj_data,
        "get_actual": actual,
        "checks": {
            "project_file_has_key": file_ok,
            "user_file_clean": user_clean,
            "get_returns_value": get_ok,
        },
    }


def case_user_clear_removes_key(ctx) -> dict:
    """C: set + clear (user scope) → user 文件不再有该 key, --get 回退 null."""
    proj = _setup_proj(ctx)
    env = _make_env(ctx)

    _run_mossen(env, proj, "--set-mossen-config", PROBE_KEY_A,
                json.dumps(USER_VAL), "--scope", "user")
    rc_clr, _, err_clr = _run_mossen(env, proj, "--clear-mossen-config",
                                     PROBE_KEY_A, "--scope", "user")
    write_command_log(ctx, ["mossen", "--clear", PROBE_KEY_A, "user"],
                      "", err_clr, rc_clr)

    user_data = read_user_settings(ctx.mossen_config_home)
    rc_get, out_get, _ = _run_mossen(env, proj, "--get-mossen-config", PROBE_KEY_A)
    try:
        actual = json.loads(out_get)
    except json.JSONDecodeError:
        actual = out_get

    file_clean = user_data is None or PROBE_KEY_A not in user_data
    get_null = rc_get == 0 and actual is None

    return {
        "scenario": "C_user_clear_removes_key",
        "ok": rc_clr == 0 and file_clean and get_null,
        "clear_rc": rc_clr,
        "user_settings_dump": user_data,
        "get_actual": actual,
        "checks": {
            "user_file_no_key_after_clear": file_clean,
            "get_returns_null": get_null,
        },
    }


def case_project_clear_removes_key(ctx) -> dict:
    """D: set + clear (project scope) → project 文件不再有该 key, --get 回退 null."""
    proj = _setup_proj(ctx)
    env = _make_env(ctx)

    _run_mossen(env, proj, "--set-mossen-config", PROBE_KEY_A,
                json.dumps(PROJECT_VAL), "--scope", "project")
    rc_clr, _, err_clr = _run_mossen(env, proj, "--clear-mossen-config",
                                     PROBE_KEY_A, "--scope", "project")
    write_command_log(ctx, ["mossen", "--clear", PROBE_KEY_A, "project"],
                      "", err_clr, rc_clr)

    proj_data = read_project_settings(proj)
    rc_get, out_get, _ = _run_mossen(env, proj, "--get-mossen-config", PROBE_KEY_A)
    try:
        actual = json.loads(out_get)
    except json.JSONDecodeError:
        actual = out_get

    file_clean = proj_data is None or PROBE_KEY_A not in proj_data
    get_null = rc_get == 0 and actual is None

    return {
        "scenario": "D_project_clear_removes_key",
        "ok": rc_clr == 0 and file_clean and get_null,
        "clear_rc": rc_clr,
        "project_settings_dump": proj_data,
        "get_actual": actual,
        "checks": {
            "project_file_no_key_after_clear": file_clean,
            "get_returns_null": get_null,
        },
    }


def case_clear_one_keeps_other(ctx) -> dict:
    """E: 同 user file 写 2 个 key, clear 一个, 另一个仍在."""
    proj = _setup_proj(ctx)
    env = _make_env(ctx)

    _run_mossen(env, proj, "--set-mossen-config", PROBE_KEY_A,
                json.dumps(USER_VAL), "--scope", "user")
    _run_mossen(env, proj, "--set-mossen-config", PROBE_KEY_B,
                json.dumps(SECONDARY_VAL), "--scope", "user")

    user_data_pre = read_user_settings(ctx.mossen_config_home) or {}
    has_both_pre = (
        user_data_pre.get(PROBE_KEY_A) == USER_VAL
        and user_data_pre.get(PROBE_KEY_B) == SECONDARY_VAL
    )

    rc_clr, _, err_clr = _run_mossen(env, proj, "--clear-mossen-config",
                                     PROBE_KEY_A, "--scope", "user")
    write_command_log(ctx, ["mossen", "--clear", PROBE_KEY_A, "user (only A)"],
                      "", err_clr, rc_clr)

    user_data_post = read_user_settings(ctx.mossen_config_home) or {}
    a_gone = PROBE_KEY_A not in user_data_post
    b_kept = user_data_post.get(PROBE_KEY_B) == SECONDARY_VAL

    rc_get_b, out_get_b, _ = _run_mossen(env, proj, "--get-mossen-config", PROBE_KEY_B)
    try:
        actual_b = json.loads(out_get_b)
    except json.JSONDecodeError:
        actual_b = out_get_b
    get_b_ok = rc_get_b == 0 and actual_b == SECONDARY_VAL

    return {
        "scenario": "E_clear_one_keeps_other",
        "ok": has_both_pre and rc_clr == 0 and a_gone and b_kept and get_b_ok,
        "user_settings_pre": user_data_pre,
        "user_settings_post": user_data_post,
        "get_b_actual": actual_b,
        "checks": {
            "both_keys_persisted_initially": has_both_pre,
            "key_a_removed_after_clear": a_gone,
            "key_b_still_present": b_kept,
            "get_b_still_returns_value": get_b_ok,
        },
    }


def _assertion(name: str, c: dict) -> dict:
    """把 R6 case 转成 SOP §1.1.3 assertion 格式 (checks 字典转为 expected/actual)."""
    return {
        "name": name,
        "expected": {k: True for k in c.get("checks", {})},
        "actual": c.get("checks", {}),
        "passed": bool(c["ok"]),
    }


def main() -> int:
    cases = []

    ctx_a = make_fixture("R6_A_user_set")
    cases.append(case_user_set_persists(ctx_a))
    write_assertions(ctx_a,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("user_set_persists_to_user_file", cases[-1])])

    ctx_b = make_fixture("R6_B_project_set")
    cases.append(case_project_set_persists(ctx_b))
    write_assertions(ctx_b,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("project_set_persists_to_project_file", cases[-1])])

    ctx_c = make_fixture("R6_C_user_clear")
    cases.append(case_user_clear_removes_key(ctx_c))
    write_assertions(ctx_c,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("user_clear_removes_key", cases[-1])])

    ctx_d = make_fixture("R6_D_project_clear")
    cases.append(case_project_clear_removes_key(ctx_d))
    write_assertions(ctx_d,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("project_clear_removes_key", cases[-1])])

    ctx_e = make_fixture("R6_E_clear_one_keeps_other")
    cases.append(case_clear_one_keeps_other(ctx_e))
    write_assertions(ctx_e,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("clear_one_does_not_clobber_other", cases[-1])])

    overall_ok = all(c["ok"] for c in cases)
    print(json.dumps({
        "results": cases,
        "passed": sum(c["ok"] for c in cases),
        "total": len(cases),
        "design_note": "R6: 5 case 验证 set/clear 真持久化到正确的 settings.json",
    }, default=str, indent=2, ensure_ascii=False))
    return 0 if overall_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
