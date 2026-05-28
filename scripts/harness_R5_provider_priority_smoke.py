#!/usr/bin/env python3
"""
R5 — Mossen config provider 优先级安全网测试 (G2-2b).

按 GrowthBook迁移计划.md §1.3 + G0-5 测试矩阵设计.

守护契约:
  4 层 override (env > project > user > default) 真按优先级生效:
  - case A (只 default): caller fallback (probe key 不在 builtin defaults)
  - case B (+ user):     user override
  - case C (+ project):  project override (覆盖 user)
  - case D (+ env):      env override (覆盖 project + user)
  - case E (无 user, 有 project + env): env (覆盖 project)

反测信号:
  - G1-3 把 env provider 错挂在 default 之后 → case D fail
  - G1-2 项目 settings 路径写错 → case C 退化到 user → fail
  - G1-2 user/project 优先级颠倒 → case C 读到 user → fail
  - G1-2 缺失级联 → case E 缺 env 时回退 default 而非 project → fail (反向)
  - facade 优先级表常量错 → 多 case 同时 fail
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
    write_project_settings,
    write_user_settings,
)


PROBE_KEY = "mossen.test.r5_probe"
USER_VAL = 2222
PROJECT_VAL = 3333
ENV_VAL = 4444
FALLBACK_VAL = "R5_DEFAULT_FALLBACK"


def _make_env(ctx, env_overrides: dict | None = None) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"
    if env_overrides:
        env["MOSSEN_CONFIG_OVERRIDES"] = json.dumps(env_overrides)
    else:
        env.pop("MOSSEN_CONFIG_OVERRIDES", None)
        env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    return env


def _read_resolved(env: dict, proj_dir: Path) -> tuple[int, str]:
    """Run mossen --get-mossen-config <PROBE_KEY>; return (rc, stdout)."""
    proc = subprocess.run(
        [str(ROOT / "scripts" / "start-mossen.sh"), "--get-mossen-config", PROBE_KEY],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(proj_dir),
    )
    return proc.returncode, proc.stdout.strip(), proc.stderr


def _case(ctx, scenario: str, *, user_val=None, project_val=None,
          env_val=None, expected) -> dict:
    proj = ctx.root_dir / "fake_project"
    proj.mkdir(parents=True, exist_ok=True)

    clear_all_overrides(ctx.mossen_config_home, proj)
    if user_val is not None:
        write_user_settings(ctx.mossen_config_home, PROBE_KEY, user_val)
    if project_val is not None:
        write_project_settings(proj, PROBE_KEY, project_val)
    env = _make_env(ctx, {PROBE_KEY: env_val} if env_val is not None else None)

    rc, out, err = _read_resolved(env, proj)
    write_command_log(
        ctx, ["mossen", "--get-mossen-config", PROBE_KEY, f"(scen={scenario})"],
        out, err, rc,
    )

    try:
        actual = json.loads(out)
    except json.JSONDecodeError:
        actual = out

    ok = rc == 0 and actual == expected
    return {
        "scenario": scenario,
        "ok": ok,
        "exit_code": rc,
        "expected": expected,
        "actual": actual,
        "user_val": user_val,
        "project_val": project_val,
        "env_val": env_val,
    }


def _assertion(name: str, c: dict) -> dict:
    return {
        "name": name,
        "expected": c["expected"],
        "actual": c["actual"],
        "passed": bool(c["ok"]),
    }


def main() -> int:
    cases = []

    # case A: 只 default (probe key 不在 builtin defaults) → caller fallback (here null)
    ctx_a = make_fixture("R5_A_default_only")
    cases.append(_case(ctx_a, "A_default_only", expected=None))
    write_assertions(ctx_a,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("default_only_returns_caller_null", cases[-1])])

    # case B: user override
    ctx_b = make_fixture("R5_B_user_overrides_default")
    cases.append(_case(ctx_b, "B_user_overrides_default",
                       user_val=USER_VAL, expected=USER_VAL))
    write_assertions(ctx_b,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("user_overrides_default", cases[-1])])

    # case C: project overrides user
    ctx_c = make_fixture("R5_C_project_overrides_user")
    cases.append(_case(ctx_c, "C_project_overrides_user",
                       user_val=USER_VAL, project_val=PROJECT_VAL,
                       expected=PROJECT_VAL))
    write_assertions(ctx_c,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("project_overrides_user", cases[-1])])

    # case D: env overrides project
    ctx_d = make_fixture("R5_D_env_overrides_project")
    cases.append(_case(ctx_d, "D_env_overrides_project",
                       user_val=USER_VAL, project_val=PROJECT_VAL,
                       env_val=ENV_VAL, expected=ENV_VAL))
    write_assertions(ctx_d,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("env_overrides_project", cases[-1])])

    # case E: 无 user, 有 project + env → env wins
    ctx_e = make_fixture("R5_E_env_without_user")
    cases.append(_case(ctx_e, "E_env_without_user_still_wins",
                       project_val=PROJECT_VAL, env_val=ENV_VAL,
                       expected=ENV_VAL))
    write_assertions(ctx_e,
                     status="passed" if cases[-1]["ok"] else "failed",
                     assertions=[_assertion("env_wins_over_project_no_user", cases[-1])])

    overall_ok = all(c["ok"] for c in cases)
    print(json.dumps({
        "results": cases,
        "passed": sum(c["ok"] for c in cases),
        "total": len(cases),
        "design_note": "R5: 5 case 验证 default<user<project<env 优先级",
    }, default=str, indent=2, ensure_ascii=False))
    return 0 if overall_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
