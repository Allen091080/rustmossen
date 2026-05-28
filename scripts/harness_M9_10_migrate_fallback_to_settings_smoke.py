#!/usr/bin/env python3
"""
M9.10 — migrate local fallback env profile into settings on the current Rust CLI.

`--migrate-fallback-profile` is the personal-edition bridge from legacy
`MOSSEN_CODE_CUSTOM_*` env configuration to first-class `mossen.profiles`.
This smoke uses the current `scripts/start-mossen.sh` launcher and verifies the
real JSON shape emitted by Rust.
"""

from __future__ import annotations

import json
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = str(ROOT / "scripts" / "start-mossen.sh")

FAKE_ENV_BASEURL = "https://fake-custom.example/v1"
FAKE_ENV_APIKEY = "sk-fake-custom-test-AAAAAAAAAAAAAAAAAAAAAAAAAAA"
FAKE_ENV_MODEL = "example-large"

FAKE_EXISTING_BASEURL = "https://fake-existing.example/v1"
FAKE_EXISTING_APIKEY = "sk-fake-existing-test-BBBBBBBBBBBBBBBBBBBBBBBBBBB"
FAKE_EXISTING_MODEL = "existing-test"

EXISTING_CUSTOM_BASEURL = "https://fake-custom-existing.example/v1"
EXISTING_CUSTOM_APIKEY = "sk-fake-custom-existing-CCCCCCCCCCCCCCCCCCCC"
EXISTING_CUSTOM_MODEL = "custom-existing"


def _make_env(ctx: Any, *, with_fallback: bool) -> dict[str, str]:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"
    env.pop("MOSSEN_CONFIG_OVERRIDES", None)
    env.pop("MOSSEN_INTERNAL_FC_OVERRIDES", None)
    for key in list(env.keys()):
        if key.startswith("MOSSEN_CODE_CUSTOM") or key == "MOSSEN_CODE_USE_CUSTOM_BACKEND":
            env.pop(key, None)
    if with_fallback:
        env.update(
            {
                "MOSSEN_CODE_USE_CUSTOM_BACKEND": "true",
                "MOSSEN_CODE_CUSTOM_BASE_URL": FAKE_ENV_BASEURL,
                "MOSSEN_CODE_CUSTOM_API_KEY": FAKE_ENV_APIKEY,
                "MOSSEN_CODE_CUSTOM_MODEL": FAKE_ENV_MODEL,
            }
        )
    return env


def _run(env: dict[str, str], args: list[str]) -> tuple[int, str, str]:
    proc = subprocess.run(
        [RUN_MOSSEN, *args],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=90,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_json(stdout: str) -> dict[str, Any] | None:
    start = stdout.find("{")
    if start < 0:
        return None
    try:
        return json.loads(stdout[start:])
    except json.JSONDecodeError:
        return None


def _read_settings(home: Path) -> dict[str, Any]:
    path = home / "settings.json"
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def _seed_settings(home: Path, profiles: dict[str, Any], active: str | None) -> None:
    home.mkdir(parents=True, exist_ok=True)
    payload: dict[str, Any] = {"mossen.profiles": profiles}
    if active is not None:
        payload["mossen.activeProfile"] = active
    (home / "settings.json").write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def _key_leaked(text: str) -> bool:
    return any(key in text for key in (FAKE_ENV_APIKEY, FAKE_EXISTING_APIKEY, EXISTING_CUSTOM_APIKEY))


def case_default_migrate_empty_settings() -> dict[str, Any]:
    ctx = make_fixture("M9.10.default_migrate")
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--migrate-fallback-profile"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile"], out, err, rc)
    data = _parse_json(out) or {}
    settings = _read_settings(ctx.mossen_config_home)
    custom = (settings.get("mossen.profiles") or {}).get("custom") or {}
    mode = stat.S_IMODE((ctx.mossen_config_home / "settings.json").stat().st_mode)

    ok = (
        rc == 0
        and data.get("status") == "Migrated"
        and data.get("profile_name") == "custom"
        and data.get("active_profile_set") is True
        and settings.get("mossen.activeProfile") == "custom"
        and custom.get("apiKey") == FAKE_ENV_APIKEY
        and custom.get("baseURL") == FAKE_ENV_BASEURL
        and mode == 0o600
        and not _key_leaked(out + err)
    )
    return {
        "name": "default_migrate_empty_settings",
        "ok": ok,
        "status": data.get("status"),
        "active_profile_set": data.get("active_profile_set"),
        "settings_mode": oct(mode),
        "_ctx": ctx,
    }


def case_existing_custom_no_force_is_noop() -> dict[str, Any]:
    ctx = make_fixture("M9.10.existing_no_force")
    _seed_settings(
        ctx.mossen_config_home,
        {
            "custom": {
                "provider": "openai-compatible",
                "baseURL": EXISTING_CUSTOM_BASEURL,
                "model": EXISTING_CUSTOM_MODEL,
                "apiKey": EXISTING_CUSTOM_APIKEY,
            }
        },
        active=None,
    )
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--migrate-fallback-profile"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile"], out, err, rc)
    data = _parse_json(out) or {}
    custom = (_read_settings(ctx.mossen_config_home).get("mossen.profiles") or {}).get("custom") or {}
    ok = (
        rc == 0
        and data.get("status") == "NotMigrated"
        and data.get("reason") == "already-exists"
        and custom.get("apiKey") == EXISTING_CUSTOM_APIKEY
        and custom.get("baseURL") == EXISTING_CUSTOM_BASEURL
        and not _key_leaked(out + err)
    )
    return {
        "name": "existing_custom_no_force_is_noop",
        "ok": ok,
        "status": data.get("status"),
        "reason": data.get("reason"),
        "_ctx": ctx,
    }


def case_force_overwrites_existing_custom() -> dict[str, Any]:
    ctx = make_fixture("M9.10.force_overwrite")
    _seed_settings(
        ctx.mossen_config_home,
        {
            "custom": {
                "provider": "openai-compatible",
                "baseURL": EXISTING_CUSTOM_BASEURL,
                "model": EXISTING_CUSTOM_MODEL,
                "apiKey": EXISTING_CUSTOM_APIKEY,
            }
        },
        active=None,
    )
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--migrate-fallback-profile", "--force"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile", "--force"], out, err, rc)
    data = _parse_json(out) or {}
    custom = (_read_settings(ctx.mossen_config_home).get("mossen.profiles") or {}).get("custom") or {}
    ok = (
        rc == 0
        and data.get("status") == "Migrated"
        and custom.get("apiKey") == FAKE_ENV_APIKEY
        and custom.get("baseURL") == FAKE_ENV_BASEURL
        and not _key_leaked(out + err)
    )
    return {"name": "force_overwrites_existing_custom", "ok": ok, "status": data.get("status"), "_ctx": ctx}


def case_auto_keeps_existing_active_existing() -> dict[str, Any]:
    ctx = make_fixture("M9.10.auto_keeps_existing")
    _seed_settings(
        ctx.mossen_config_home,
        {
            "existing": {
                "provider": "openai-compatible",
                "baseURL": FAKE_EXISTING_BASEURL,
                "model": FAKE_EXISTING_MODEL,
                "apiKey": FAKE_EXISTING_APIKEY,
            }
        },
        active="existing",
    )
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--migrate-fallback-profile"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile"], out, err, rc)
    data = _parse_json(out) or {}
    settings = _read_settings(ctx.mossen_config_home)
    profiles = settings.get("mossen.profiles") or {}
    ok = (
        rc == 0
        and data.get("status") == "Migrated"
        and data.get("active_profile_set") is False
        and settings.get("mossen.activeProfile") == "existing"
        and "existing" in profiles
        and "custom" in profiles
    )
    return {
        "name": "auto_keeps_existing_active_existing",
        "ok": ok,
        "active_profile_set": data.get("active_profile_set"),
        "active": settings.get("mossen.activeProfile"),
        "_ctx": ctx,
    }


def case_activate_always_sets_custom() -> dict[str, Any]:
    ctx = make_fixture("M9.10.activate_always")
    _seed_settings(
        ctx.mossen_config_home,
        {
            "existing": {
                "provider": "openai-compatible",
                "baseURL": FAKE_EXISTING_BASEURL,
                "model": FAKE_EXISTING_MODEL,
                "apiKey": FAKE_EXISTING_APIKEY,
            }
        },
        active="existing",
    )
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--migrate-fallback-profile", "--activate", "always"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile", "--activate", "always"], out, err, rc)
    data = _parse_json(out) or {}
    settings = _read_settings(ctx.mossen_config_home)
    ok = (
        rc == 0
        and data.get("status") == "Migrated"
        and data.get("active_profile_set") is True
        and settings.get("mossen.activeProfile") == "custom"
    )
    return {
        "name": "activate_always_sets_custom",
        "ok": ok,
        "active_profile_set": data.get("active_profile_set"),
        "active": settings.get("mossen.activeProfile"),
        "_ctx": ctx,
    }


def case_no_fallback_is_noop() -> dict[str, Any]:
    ctx = make_fixture("M9.10.no_fallback")
    env = _make_env(ctx, with_fallback=False)
    rc, out, err = _run(env, ["--migrate-fallback-profile"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile"], out, err, rc)
    data = _parse_json(out) or {}
    ok = (
        rc == 0
        and data.get("status") == "NotMigrated"
        and data.get("reason") == "no-fallback"
        and not (ctx.mossen_config_home / "settings.json").exists()
    )
    return {"name": "no_fallback_is_noop", "ok": ok, "status": data.get("status"), "reason": data.get("reason"), "_ctx": ctx}


def case_post_migrate_list_uses_settings_profile() -> dict[str, Any]:
    ctx = make_fixture("M9.10.post_migrate_list")
    env = _make_env(ctx, with_fallback=True)
    rc1, out1, err1 = _run(env, ["--migrate-fallback-profile"])
    rc2, out2, err2 = _run(env, ["--list-model-profiles"])
    write_command_log(ctx, [RUN_MOSSEN, "--migrate-fallback-profile", "&&", "--list-model-profiles"], out1 + "\n---\n" + out2, err1 + err2, rc1 + rc2)
    data = _parse_json(out2) or {}
    all_profiles = data.get("allProfiles") or []
    current = data.get("currentProfile") or {}
    fallback = data.get("fallbackProfile") or {}
    ok = (
        rc1 == 0
        and rc2 == 0
        and len(all_profiles) == 1
        and all_profiles[0].get("name") == "custom"
        and all_profiles[0].get("source") == "settings"
        and current.get("source") == "settings"
        and fallback.get("source") == "fallback-env"
        and not _key_leaked(out1 + err1 + out2 + err2)
    )
    return {
        "name": "post_migrate_list_uses_settings_profile",
        "ok": ok,
        "current_source": current.get("source"),
        "fallback_source": fallback.get("source"),
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_default_migrate_empty_settings(),
        case_existing_custom_no_force_is_noop(),
        case_force_overwrites_existing_custom(),
        case_auto_keeps_existing_active_existing(),
        case_activate_always_sets_custom(),
        case_no_fallback_is_noop(),
        case_post_migrate_list_uses_settings_profile(),
    ]
    status = "passed" if all(case["ok"] for case in cases) else "failed"
    last_ctx = cases[-1].pop("_ctx")
    for case in cases[:-1]:
        case.pop("_ctx", None)
    write_assertions(
        last_ctx,
        status=status,
        assertions=[
            {
                "name": case["name"],
                "expected": True,
                "actual": case["ok"],
                "passed": case["ok"],
                "evidence": json.dumps(case, ensure_ascii=False)[:500],
            }
            for case in cases
        ],
    )
    print(json.dumps({"status": status, "results": cases}, indent=2, ensure_ascii=False))
    return 0 if status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
