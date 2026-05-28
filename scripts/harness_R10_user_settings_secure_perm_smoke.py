#!/usr/bin/env python3
"""
R10 — User settings.json 写入后强制 chmod 0600 安全网测试 (Stage1 hotfix).

背景:
  S1-09 落地后, ~/.mossen/settings.json 内嵌 multi-profile apiKey (D-S09-1=A).
  原 LocalSettingsProvider.set() 仅 writeFileSync, 不强制 chmod, 文件首次创建后
  权限取决于 OS umask (常为 022 → 0644 world-readable). 本地环境实测 644.

  Allen 2026-04-28 拍板 hotfix: 写入后必须 statSync + chmodSync 0o600 if mode != 0o600.

守护契约:
  case A: user settings 写入后 mode == 0o600
  case B: 即使预先 chmod 0o644, 再次写入仍 reset 到 0o600
  case C: project settings 写入不被 chmod (保留 OS 默认, 因 project settings 通常 share/commit)

反测信号:
  - LocalSettingsProvider.set 漏调 enforceSecurePermission → case A/B fail
  - UserSettingsProvider 忘 override getSecurePermissionMode → case A/B fail
  - 误把 enforceSecurePermission 加到 base default 0o600 → case C fail
"""

from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


PROBE_KEY_USER = "mossen.test.r10_user_probe"
PROBE_KEY_PROJECT = "mossen.test.r10_project_probe"
PROBE_VAL = "R10_PROBE_VALUE"


def _make_env(ctx, project_cwd: Path | None = None) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"
    env.pop("MOSSEN_CONFIG_OVERRIDES", None)
    env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    return env


def _run_set(env: dict, scope: str, key: str, value, cwd: Path) -> tuple[int, str, str]:
    proc = subprocess.run(
        [
            str(ROOT / "scripts" / "start-mossen.sh"),
            "--set-mossen-config",
            key,
            json.dumps(value),
            "--scope",
            scope,
        ],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(cwd),
    )
    return proc.returncode, proc.stdout, proc.stderr


def _file_mode(path: Path) -> int | None:
    if not path.exists():
        return None
    return stat.S_IMODE(path.stat().st_mode)


def case_a_user_first_write_chmod_600(ctx) -> dict:
    """case A: 首次写入 user settings → 必须是 0o600."""
    user_path = ctx.mossen_config_home / "settings.json"
    if user_path.exists():
        user_path.unlink()

    proj = ctx.root_dir / "fake_project_a"
    proj.mkdir(parents=True, exist_ok=True)

    env = _make_env(ctx)
    rc, out, err = _run_set(env, "user", PROBE_KEY_USER, PROBE_VAL, proj)
    write_command_log(ctx, ["mossen", "--set-mossen-config", PROBE_KEY_USER, "(case A)"], out, err, rc)

    mode = _file_mode(user_path)
    file_exists = user_path.exists()
    return {
        "name": "user_first_write_is_0600",
        "ok": rc == 0 and file_exists and mode == 0o600,
        "exit_code": rc,
        "file_exists": file_exists,
        "mode_octal": f"0o{oct(mode)[2:]}" if mode is not None else None,
        "expected_octal": "0o600",
    }


def case_b_user_rewrite_resets_644_to_600(ctx) -> dict:
    """case B: 故意 chmod 0o644, 再次写入 → 必须 reset 到 0o600."""
    user_path = ctx.mossen_config_home / "settings.json"
    if not user_path.exists():
        # 复用 case A 已写, 否则单独写一遍
        user_path.parent.mkdir(parents=True, exist_ok=True)
        user_path.write_text(json.dumps({PROBE_KEY_USER: PROBE_VAL}, indent=2) + "\n")

    os.chmod(user_path, 0o644)
    pre_mode = _file_mode(user_path)

    proj = ctx.root_dir / "fake_project_b"
    proj.mkdir(parents=True, exist_ok=True)

    env = _make_env(ctx)
    rc, out, err = _run_set(env, "user", PROBE_KEY_USER, "R10_REWRITE", proj)
    write_command_log(ctx, ["mossen", "--set-mossen-config", PROBE_KEY_USER, "(case B rewrite)"], out, err, rc)

    post_mode = _file_mode(user_path)
    return {
        "name": "user_rewrite_resets_644_to_600",
        "ok": rc == 0 and pre_mode == 0o644 and post_mode == 0o600,
        "exit_code": rc,
        "pre_mode_octal": f"0o{oct(pre_mode)[2:]}" if pre_mode is not None else None,
        "post_mode_octal": f"0o{oct(post_mode)[2:]}" if post_mode is not None else None,
    }


def case_c_project_settings_not_chmod(ctx) -> dict:
    """case C: project settings 写入不强制 chmod (保留 OS 默认)."""
    proj = ctx.root_dir / "fake_project_c"
    proj.mkdir(parents=True, exist_ok=True)
    proj_path = proj / ".mossen" / "settings.json"
    if proj_path.exists():
        proj_path.unlink()

    env = _make_env(ctx)
    rc, out, err = _run_set(env, "project", PROBE_KEY_PROJECT, PROBE_VAL, proj)
    write_command_log(ctx, ["mossen", "--set-mossen-config", PROBE_KEY_PROJECT, "(case C project)"], out, err, rc)

    mode = _file_mode(proj_path)
    # project 不强制 chmod 600. 一般 OS umask 022 → 0o644.
    # 我们守 "不是 0o600" (不被 user 路径误伤即 PASS).
    ok = rc == 0 and proj_path.exists() and mode != 0o600
    return {
        "name": "project_settings_not_forced_to_600",
        "ok": ok,
        "exit_code": rc,
        "file_exists": proj_path.exists(),
        "mode_octal": f"0o{oct(mode)[2:]}" if mode is not None else None,
        "note": "project settings 通常 share/commit, 不应被 user-scope 安全策略误伤",
    }


def main() -> int:
    ctx = make_fixture("R10_user_settings_perm")
    results = [
        case_a_user_first_write_chmod_600(ctx),
        case_b_user_rewrite_resets_644_to_600(ctx),
        case_c_project_settings_not_chmod(ctx),
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
        "design_note": "R10 (Stage1 hotfix): UserSettingsProvider.set 写入后强制 chmod 0o600; ProjectSettingsProvider 不受影响.",
    }, indent=2, ensure_ascii=False))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
