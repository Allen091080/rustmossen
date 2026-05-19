#!/usr/bin/env python3
"""
M9.10 — `mossen --migrate-fallback-profile` 把 env fallback 升级为正式 settings profile.

S1-09 收口: qwen 从 fallback 兜底 → 正式 modelProfiles.qwen, 与 glm/minimax 平等.

守护契约:
  case 1 — 0 settings + env qwen + migrate (default)
           → settings.profiles.qwen 存在 + activeProfile=qwen + activeProfileSet=true
  case 2 — settings 已有 qwen + migrate (no force)
           → migrated=false reason=already-exists; settings 不动
  case 3 — settings 已有 qwen + migrate --force
           → migrated=true; profile 被覆盖为 env 值
  case 4 — settings 有 active=glm + migrate (auto)
           → migrated=true; activeProfile 仍是 glm (auto 模式不改动已显式 active)
  case 5 — settings 有 active=glm + migrate --activate=always
           → migrated=true; activeProfile 改为 qwen
  case 6 — 0 env fallback + migrate
           → migrated=false reason=no-fallback (no-op)
  case 7 — 迁移后 /model 列表 qwen 标 [settings] 不再带 [fallback]
  case 8 — 迁移后 settings.json 权限 0600 (R10 hotfix 守护)
  case 9 — apiKey 全脱敏不泄露 (任何 stdout/stderr)
  case 10 — 迁移后再 /model glm → /model qwen 切回正式 qwen profile (不是 fallback)

反测信号:
  - migrateFallbackProfile 漏写 setProfile → case 1 settings 无 qwen
  - auto 模式误覆盖现有 active → case 4 active 变 qwen
  - --force 不生效 → case 3 profile 未变
  - LocalSettingsProvider chmod 600 失效 → case 8 fail (R10 回归)
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


FAKE_ENV_BASEURL = "https://fake-qwen.example/v1"
FAKE_ENV_APIKEY = "sk-fake-qwen-test-AAAAAAAAAAAAAAAAAAAAAAAAAAA"
FAKE_ENV_MODEL = "qwen3.6-plus"

FAKE_GLM_BASEURL = "https://fake-glm.example/v1"
FAKE_GLM_APIKEY = "sk-fake-glm-test-BBBBBBBBBBBBBBBBBBBBBBBBBBB"
FAKE_GLM_MODEL = "glm-test"

FAKE_QWEN_REAL_BASEURL = "https://fake-qwen-existing.example/v1"
FAKE_QWEN_REAL_APIKEY = "sk-fake-qwen-existing-CCCCCCCCCCCCCCCCCCCC"
FAKE_QWEN_REAL_MODEL = "qwen-existing"


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


def _seed_settings(home: Path, profiles: dict, active: str | None) -> None:
    home.mkdir(parents=True, exist_ok=True)
    payload: dict = {"mossen.profiles": profiles}
    if active:
        payload["mossen.activeProfile"] = active
    settings = home / "settings.json"
    settings.write_text(json.dumps(payload, indent=2) + "\n")


def _read_settings(home: Path) -> dict:
    settings = home / "settings.json"
    if not settings.exists():
        return {}
    return json.loads(settings.read_text())


def _run_migrate(env: dict, cwd: Path, *extra_args: str) -> tuple[int, str, str]:
    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "--migrate-fallback-profile", *extra_args],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(cwd),
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_json(out: str) -> dict | None:
    try:
        i = out.find("{")
        return json.loads(out[i:]) if i >= 0 else None
    except Exception:
        return None


def _file_mode(path: Path) -> int | None:
    if not path.exists():
        return None
    return stat.S_IMODE(path.stat().st_mode)


def case_1_default_migrate_empty_settings(ctx) -> dict:
    proj = ctx.root_dir / "case1"
    proj.mkdir(parents=True, exist_ok=True)
    settings_dir = ctx.mossen_config_home
    if (settings_dir / "settings.json").exists():
        (settings_dir / "settings.json").unlink()
    env = _make_env(ctx, with_fallback=True)

    rc, out, err = _run_migrate(env, proj)
    write_command_log(ctx, ["mossen", "--migrate-fallback-profile", "(case 1 default)"], out, err, rc)
    data = _parse_json(out)
    after = _read_settings(settings_dir)
    qwen_in_settings = "qwen" in after.get("mossen.profiles", {})
    qwen_data = after.get("mossen.profiles", {}).get("qwen", {})
    raw_key_match = qwen_data.get("apiKey") == FAKE_ENV_APIKEY
    apikey_leaked = FAKE_ENV_APIKEY in out
    return {
        "name": "case1_default_migrate_empty_settings",
        "ok": (
            rc == 0
            and data is not None
            and data.get("ok") is True
            and data.get("migrated") is True
            and data.get("profileName") == "qwen"
            and data.get("activeProfileSet") is True
            and qwen_in_settings
            and raw_key_match
            and after.get("mossen.activeProfile") == "qwen"
            and not apikey_leaked
        ),
        "exit_code": rc,
        "json_migrated": (data or {}).get("migrated"),
        "json_activeProfileSet": (data or {}).get("activeProfileSet"),
        "qwen_in_settings": qwen_in_settings,
        "raw_key_persisted": raw_key_match,
        "active_in_settings": after.get("mossen.activeProfile"),
        "apikey_leaked_in_stdout": apikey_leaked,
    }


def case_2_already_exists_no_force(ctx) -> dict:
    proj = ctx.root_dir / "case2"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_settings(home, {
        "qwen": {
            "provider": "openai-compatible",
            "baseURL": FAKE_QWEN_REAL_BASEURL,
            "model": FAKE_QWEN_REAL_MODEL,
            "apiKey": FAKE_QWEN_REAL_APIKEY,
        },
    }, active=None)
    env = _make_env(ctx, with_fallback=True)

    rc, out, err = _run_migrate(env, proj)
    write_command_log(ctx, ["mossen", "--migrate-fallback-profile", "(case 2 already-exists)"], out, err, rc)
    data = _parse_json(out)
    after = _read_settings(home)
    qwen_unchanged = after.get("mossen.profiles", {}).get("qwen", {}).get("apiKey") == FAKE_QWEN_REAL_APIKEY
    apikey_leaked = FAKE_QWEN_REAL_APIKEY in out or FAKE_ENV_APIKEY in out
    return {
        "name": "case2_already_exists_no_force",
        "ok": (
            rc == 0
            and data is not None
            and data.get("migrated") is False
            and data.get("reason") == "already-exists"
            and qwen_unchanged
            and not apikey_leaked
        ),
        "exit_code": rc,
        "json_migrated": (data or {}).get("migrated"),
        "json_reason": (data or {}).get("reason"),
        "qwen_apiKey_unchanged": qwen_unchanged,
        "apikey_leaked": apikey_leaked,
    }


def case_3_force_overwrites(ctx) -> dict:
    proj = ctx.root_dir / "case3"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_settings(home, {
        "qwen": {
            "provider": "openai-compatible",
            "baseURL": FAKE_QWEN_REAL_BASEURL,
            "model": FAKE_QWEN_REAL_MODEL,
            "apiKey": FAKE_QWEN_REAL_APIKEY,
        },
    }, active=None)
    env = _make_env(ctx, with_fallback=True)

    rc, out, err = _run_migrate(env, proj, "--force")
    write_command_log(ctx, ["mossen", "--migrate-fallback-profile", "--force", "(case 3)"], out, err, rc)
    data = _parse_json(out)
    after = _read_settings(home)
    qwen = after.get("mossen.profiles", {}).get("qwen", {})
    overwritten = qwen.get("apiKey") == FAKE_ENV_APIKEY and qwen.get("baseURL") == FAKE_ENV_BASEURL
    apikey_leaked = FAKE_QWEN_REAL_APIKEY in out or FAKE_ENV_APIKEY in out
    return {
        "name": "case3_force_overwrites",
        "ok": rc == 0 and (data or {}).get("migrated") is True and overwritten and not apikey_leaked,
        "exit_code": rc,
        "json_migrated": (data or {}).get("migrated"),
        "qwen_overwritten_to_env_values": overwritten,
        "apikey_leaked": apikey_leaked,
    }


def case_4_auto_keeps_existing_active_glm(ctx) -> dict:
    proj = ctx.root_dir / "case4"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_settings(home, {
        "glm": {
            "provider": "openai-compatible",
            "baseURL": FAKE_GLM_BASEURL,
            "model": FAKE_GLM_MODEL,
            "apiKey": FAKE_GLM_APIKEY,
        },
    }, active="glm")
    env = _make_env(ctx, with_fallback=True)

    rc, out, err = _run_migrate(env, proj)
    write_command_log(ctx, ["mossen", "--migrate-fallback-profile", "(case 4 auto keep glm)"], out, err, rc)
    data = _parse_json(out)
    after = _read_settings(home)
    return {
        "name": "case4_auto_keeps_existing_active_glm",
        "ok": (
            rc == 0
            and (data or {}).get("migrated") is True
            and (data or {}).get("activeProfileSet") is False  # auto: keep glm
            and after.get("mossen.activeProfile") == "glm"
            and "qwen" in after.get("mossen.profiles", {})
            and "glm" in after.get("mossen.profiles", {})
        ),
        "exit_code": rc,
        "json_activeProfileSet": (data or {}).get("activeProfileSet"),
        "active_after_migrate": after.get("mossen.activeProfile"),
        "qwen_present": "qwen" in after.get("mossen.profiles", {}),
        "glm_present": "glm" in after.get("mossen.profiles", {}),
    }


def case_5_activate_always_overrides_glm(ctx) -> dict:
    proj = ctx.root_dir / "case5"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    _seed_settings(home, {
        "glm": {
            "provider": "openai-compatible",
            "baseURL": FAKE_GLM_BASEURL,
            "model": FAKE_GLM_MODEL,
            "apiKey": FAKE_GLM_APIKEY,
        },
    }, active="glm")
    env = _make_env(ctx, with_fallback=True)

    rc, out, err = _run_migrate(env, proj, "--activate", "always")
    write_command_log(ctx, ["mossen", "--migrate-fallback-profile", "--activate", "always", "(case 5)"], out, err, rc)
    data = _parse_json(out)
    after = _read_settings(home)
    return {
        "name": "case5_activate_always_overrides_glm",
        "ok": (
            rc == 0
            and (data or {}).get("migrated") is True
            and (data or {}).get("activeProfileSet") is True
            and after.get("mossen.activeProfile") == "qwen"
        ),
        "exit_code": rc,
        "json_activeProfileSet": (data or {}).get("activeProfileSet"),
        "active_after_migrate": after.get("mossen.activeProfile"),
    }


def case_6_no_fallback_no_op(ctx) -> dict:
    """0 env + migrate → migrated=false reason=no-fallback. 用 raw bun 绕开 wrapper env 注入."""
    proj = ctx.root_dir / "case6"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    if (home / "settings.json").exists():
        (home / "settings.json").unlink()
    env = _make_env(ctx, with_fallback=False)

    proc = subprocess.run(
        ["bun", "run", "entrypoints/cli.tsx", "--migrate-fallback-profile"],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    write_command_log(ctx, ["bun", "entrypoints/cli.tsx", "--migrate-fallback-profile", "(case 6)"], proc.stdout, proc.stderr, proc.returncode)
    data = _parse_json(proc.stdout)
    settings_still_absent = not (home / "settings.json").exists()
    return {
        "name": "case6_no_fallback_no_op",
        "ok": (
            proc.returncode == 0
            and (data or {}).get("migrated") is False
            and (data or {}).get("reason") == "no-fallback"
            and settings_still_absent
        ),
        "exit_code": proc.returncode,
        "json_migrated": (data or {}).get("migrated"),
        "json_reason": (data or {}).get("reason"),
        "settings_still_absent": settings_still_absent,
    }


def case_7_post_migrate_model_list_no_fallback_tag(ctx) -> dict:
    """case 1 后续: /model 列表 qwen 不带 [fallback], allProfiles[] 都是 settings 来源."""
    proj = ctx.root_dir / "case7"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    if (home / "settings.json").exists():
        (home / "settings.json").unlink()
    env = _make_env(ctx, with_fallback=True)
    # 先 migrate
    rc1, out1, err1 = _run_migrate(env, proj)
    if rc1 != 0:
        return {"name": "case7_post_migrate_model_list_no_fallback_tag", "ok": False,
                "exit_code": rc1, "stdout_excerpt": out1[:200]}

    # 调 /model
    code = (
        "const m = await import('./commands/model/model.tsx');"
        "const r = await m.call('', {});"
        "process.stdout.write(r.value);"
    )
    proc = subprocess.run(
        [str(ROOT / "run-bun-featured.sh"), "-e", code],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    write_command_log(ctx, ["bun", "/model after migrate", "(case 7)"], proc.stdout, proc.stderr, proc.returncode)
    out = proc.stdout
    has_qwen = "qwen" in out
    has_fallback_tag = "[fallback" in out or "fallback]" in out
    has_settings_source = "source:   settings.json" in out
    apikey_leaked = FAKE_ENV_APIKEY in out
    return {
        "name": "case7_post_migrate_model_list_no_fallback_tag",
        "ok": proc.returncode == 0 and has_qwen and not has_fallback_tag and has_settings_source and not apikey_leaked,
        "exit_code": proc.returncode,
        "has_qwen": has_qwen,
        "no_fallback_tag": not has_fallback_tag,
        "has_settings_source": has_settings_source,
        "apikey_leaked": apikey_leaked,
        "stdout_excerpt": out[:600],
    }


def case_8_settings_perm_600_after_migrate(ctx) -> dict:
    """迁移写入后 settings.json 必须为 0o600 (R10 hotfix 守)."""
    proj = ctx.root_dir / "case8"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    if (home / "settings.json").exists():
        (home / "settings.json").unlink()
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run_migrate(env, proj)
    write_command_log(ctx, ["mossen", "--migrate-fallback-profile", "(case 8 perm)"], out, err, rc)
    mode = _file_mode(home / "settings.json")
    return {
        "name": "case8_settings_perm_600_after_migrate",
        "ok": rc == 0 and mode == 0o600,
        "exit_code": rc,
        "mode_octal": f"0o{oct(mode)[2:]}" if mode is not None else None,
    }


def case_9_apikey_never_in_stdout_or_stderr_across_calls(ctx) -> dict:
    proj = ctx.root_dir / "case9"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    if (home / "settings.json").exists():
        (home / "settings.json").unlink()
    env = _make_env(ctx, with_fallback=True)

    all_text = ""
    for cmd in (
        ["--migrate-fallback-profile"],
        ["--list-model-profiles"],
        ["--get-model-profile", "qwen"],
    ):
        proc = subprocess.run(
            [str(ROOT / "run-mossen.sh"), *cmd],
            env=env, capture_output=True, text=True, timeout=60, cwd=str(proj),
        )
        all_text += proc.stdout + proc.stderr
    write_command_log(ctx, ["mossen", "<3 cmds>"], all_text, "", 0)
    leaked = FAKE_ENV_APIKEY in all_text
    expected_mask = f"{FAKE_ENV_APIKEY[:6]}...{FAKE_ENV_APIKEY[-4:]}"
    masked_present = expected_mask in all_text
    return {
        "name": "case9_apikey_never_in_stdout_or_stderr_across_calls",
        "ok": not leaked and masked_present,
        "leaked_count": all_text.count(FAKE_ENV_APIKEY),
        "masked_form_present": masked_present,
        "all_text_len": len(all_text),
    }


def case_10_post_migrate_switch_back_via_real_qwen(ctx) -> dict:
    """case 1 + glm settings → /model glm → /model qwen → 切回正式 qwen profile (非 fallback)."""
    proj = ctx.root_dir / "case10"
    proj.mkdir(parents=True, exist_ok=True)
    home = ctx.mossen_config_home
    # 先 seed glm
    _seed_settings(home, {
        "glm": {
            "provider": "openai-compatible",
            "baseURL": FAKE_GLM_BASEURL,
            "model": FAKE_GLM_MODEL,
            "apiKey": FAKE_GLM_APIKEY,
        },
    }, active="glm")
    env = _make_env(ctx, with_fallback=True)
    # migrate (auto: 保留 glm active)
    rc1, out1, err1 = _run_migrate(env, proj)
    if rc1 != 0:
        return {"name": "case10_post_migrate_switch_back_via_real_qwen", "ok": False,
                "exit_code": rc1, "stdout_excerpt": out1[:200]}
    # /model glm → glm session, then /model qwen → 真 qwen (settings)
    code = (
        "const m = await import('./commands/model/model.tsx');"
        "const profilesM = await import('./services/config/profiles.ts');"
        "await m.call('glm', {});"
        "const a = profilesM.getCurrentProfile();"
        "await m.call('qwen', {});"
        "const b = profilesM.getCurrentProfile();"
        "process.stdout.write(JSON.stringify({"
        "  a_name: a ? a.name : null, a_source: a ? a.source : null,"
        "  b_name: b ? b.name : null, b_source: b ? b.source : null,"
        "}));"
    )
    proc = subprocess.run(
        [str(ROOT / "run-bun-featured.sh"), "-e", code],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    write_command_log(ctx, ["bun", "switch glm → qwen real", "(case 10)"], proc.stdout, proc.stderr, proc.returncode)
    try:
        data = json.loads(proc.stdout)
    except Exception:
        return {"name": "case10_post_migrate_switch_back_via_real_qwen", "ok": False,
                "exit_code": proc.returncode, "stdout_excerpt": proc.stdout[:200]}
    apikey_leaked = FAKE_ENV_APIKEY in proc.stdout or FAKE_GLM_APIKEY in proc.stdout
    return {
        "name": "case10_post_migrate_switch_back_via_real_qwen",
        "ok": (
            proc.returncode == 0
            and data["a_name"] == "glm" and data["a_source"] == "settings"
            and data["b_name"] == "qwen" and data["b_source"] == "settings"  # 关键: 不是 fallback-env
            and not apikey_leaked
        ),
        "exit_code": proc.returncode,
        "after_glm_switch": {"name": data["a_name"], "source": data["a_source"]},
        "after_qwen_switch": {"name": data["b_name"], "source": data["b_source"]},
        "apikey_leaked": apikey_leaked,
    }


def main() -> int:
    ctx = make_fixture("M9.10_migrate_fallback")
    results = [
        case_1_default_migrate_empty_settings(ctx),
        case_2_already_exists_no_force(ctx),
        case_3_force_overwrites(ctx),
        case_4_auto_keeps_existing_active_glm(ctx),
        case_5_activate_always_overrides_glm(ctx),
        case_6_no_fallback_no_op(ctx),
        case_7_post_migrate_model_list_no_fallback_tag(ctx),
        case_8_settings_perm_600_after_migrate(ctx),
        case_9_apikey_never_in_stdout_or_stderr_across_calls(ctx),
        case_10_post_migrate_switch_back_via_real_qwen(ctx),
    ]
    all_ok = all(r["ok"] for r in results)
    write_assertions(ctx, status="passed" if all_ok else "failed", assertions=results)
    print(json.dumps({
        "results": results,
        "passed": sum(1 for r in results if r["ok"]),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M9.10 (S1-09 收口): --migrate-fallback-profile 把 env fallback 升级为正式 settings profile, qwen/glm/minimax 三 profile 平等管理.",
    }, indent=2, ensure_ascii=False))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
