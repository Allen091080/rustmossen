#!/usr/bin/env python3
"""
M9.9 — fallback env profile visibility on the current Rust CLI.

Fallback profiles are derived from `MOSSEN_CODE_CUSTOM_*`. In the personal
edition they are not a hosted/team feature; they are a local compatibility path
for existing custom-backend env configuration. This smoke verifies the current
Rust CLI behavior:
  - fallback appears in `--list-model-profiles` when no settings profiles exist;
  - settings profiles hide fallback from `allProfiles`, while `fallbackProfile`
    still reports that the env fallback exists;
  - `--set-model-profile <fallback>` can switch back to fallback without writing
    a fake settings profile;
  - raw API keys do not leak in CLI output.
"""

from __future__ import annotations

import json
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


def _seed_existing_profile(home: Path) -> None:
    home.mkdir(parents=True, exist_ok=True)
    payload = {
        "mossen.profiles": {
            "existing": {
                "provider": "openai-compatible",
                "baseURL": FAKE_EXISTING_BASEURL,
                "model": FAKE_EXISTING_MODEL,
                "apiKey": FAKE_EXISTING_APIKEY,
            }
        },
        "mossen.activeProfile": "existing",
    }
    (home / "settings.json").write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def _key_leaked(text: str) -> bool:
    return FAKE_ENV_APIKEY in text or FAKE_EXISTING_APIKEY in text


def case_fallback_visible_without_settings() -> dict[str, Any]:
    ctx = make_fixture("M9.9.fallback_only")
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--list-model-profiles"])
    write_command_log(ctx, [RUN_MOSSEN, "--list-model-profiles"], out, err, rc)
    data = _parse_json(out) or {}
    fallback = data.get("fallbackProfile") or {}
    current = data.get("currentProfile") or {}
    all_profiles = data.get("allProfiles") or []
    fallback_in_all = any(item.get("source") == "fallback-env" for item in all_profiles)
    masked = "..." in json.dumps(fallback)

    ok = (
        rc == 0
        and data.get("count") == 0
        and data.get("countAll") == 1
        and fallback.get("name") == "custom"
        and fallback.get("source") == "fallback-env"
        and current.get("name") == "custom"
        and current.get("source") == "fallback-env"
        and fallback_in_all
        and masked
        and not _key_leaked(out + err)
    )
    return {
        "name": "fallback_visible_without_settings_profiles",
        "ok": ok,
        "count": data.get("count"),
        "countAll": data.get("countAll"),
        "fallback": {"name": fallback.get("name"), "source": fallback.get("source")},
        "current": {"name": current.get("name"), "source": current.get("source")},
        "fallback_in_all": fallback_in_all,
        "_ctx": ctx,
    }


def case_settings_hide_fallback_from_all_profiles() -> dict[str, Any]:
    ctx = make_fixture("M9.9.settings_plus_fallback")
    _seed_existing_profile(ctx.mossen_config_home)
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--list-model-profiles"])
    write_command_log(ctx, [RUN_MOSSEN, "--list-model-profiles"], out, err, rc)
    data = _parse_json(out) or {}
    all_profiles = data.get("allProfiles") or []
    fallback_in_all = any(item.get("source") == "fallback-env" for item in all_profiles)
    fallback = data.get("fallbackProfile") or {}

    ok = (
        rc == 0
        and data.get("count") == 1
        and data.get("countAll") == 1
        and len(all_profiles) == 1
        and all_profiles[0].get("name") == "existing"
        and all_profiles[0].get("source") == "settings"
        and fallback.get("name") == "custom"
        and fallback.get("source") == "fallback-env"
        and not fallback_in_all
        and not _key_leaked(out + err)
    )
    return {
        "name": "settings_profiles_hide_fallback_from_allProfiles",
        "ok": ok,
        "count": data.get("count"),
        "countAll": data.get("countAll"),
        "fallback_in_all": fallback_in_all,
        "_ctx": ctx,
    }


def case_set_model_profile_to_fallback_clears_settings_active() -> dict[str, Any]:
    ctx = make_fixture("M9.9.set_fallback")
    _seed_existing_profile(ctx.mossen_config_home)
    env = _make_env(ctx, with_fallback=True)
    rc, out, err = _run(env, ["--set-model-profile", "custom"])
    write_command_log(ctx, [RUN_MOSSEN, "--set-model-profile", "custom"], out, err, rc)
    data = _parse_json(out) or {}
    settings = json.loads((ctx.mossen_config_home / "settings.json").read_text(encoding="utf-8"))

    ok = (
        rc == 0
        and data.get("ok") is True
        and data.get("activeProfile") == "custom"
        and data.get("source") == "fallback-env"
        and settings.get("mossen.activeProfile") is None
        and "existing" in (settings.get("mossen.profiles") or {})
        and "custom" not in (settings.get("mossen.profiles") or {})
        and not _key_leaked(out + err)
    )
    return {
        "name": "set_model_profile_to_fallback_clears_settings_active",
        "ok": ok,
        "source": data.get("source"),
        "settings_active": settings.get("mossen.activeProfile"),
        "_ctx": ctx,
    }


def case_no_fallback_when_env_unset() -> dict[str, Any]:
    ctx = make_fixture("M9.9.no_fallback")
    env = _make_env(ctx, with_fallback=False)
    rc, out, err = _run(env, ["--list-model-profiles"])
    write_command_log(ctx, [RUN_MOSSEN, "--list-model-profiles"], out, err, rc)
    data = _parse_json(out) or {}
    ok = (
        rc == 0
        and data.get("count") == 0
        and data.get("countAll") == 0
        and data.get("fallbackProfile") is None
        and data.get("currentProfile") is None
    )
    return {
        "name": "no_fallback_when_env_unset",
        "ok": ok,
        "count": data.get("count"),
        "countAll": data.get("countAll"),
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_fallback_visible_without_settings(),
        case_settings_hide_fallback_from_all_profiles(),
        case_set_model_profile_to_fallback_clears_settings_active(),
        case_no_fallback_when_env_unset(),
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
