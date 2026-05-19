#!/usr/bin/env python3
"""
M9.5 — customBackend.ts 8 getter profile-aware (S1-09b P0).

按 Stage1-CLI基线加固.md §11.6 + Allen D-S09-3=P 契约:
  customBackend.ts 8 getter (isEnabled / baseUrl / apiKey / authToken / model / name /
  protocol / config) 必须按"active profile > 旧 MOSSEN_CODE_CUSTOM_* env > null" 优先级.

  关键 case (5):
    1. case_profile_only_no_env: 写 mossen.profiles + activeProfile=qwen, 不设旧 env →
       getCustomBackendBaseUrl/Model/ApiKey/Name/Protocol 全走 profile + isCustomBackendEnabled=true
    2. case_env_only_no_profile (Allen 当前现状): 不写 profiles, 仅旧 env →
       全部走旧 env, 旧 qwen 默认零破坏
    3. case_both_profile_priority: 同时写 profile + 旧 env (不同值) →
       profile 字面赢 (active profile 优先)
    4. case_no_active_profile_falls_back: 写 mossen.profiles 但 activeProfile 缺失 →
       fallback 到旧 env, isCustomBackendEnabled 看旧 env
    5. case_customBackendCapabilityAppliesToModel: 切换 active profile 后,
       customBackendCapabilityAppliesToModel(profile.model) → true,
       customBackendCapabilityAppliesToModel('other-model') → false (除非 profile.model 为空)

  反测信号:
    a) 删 customBackend.ts activeProfileOrNull() → case_profile_only stays env-only fallback
       → case 1 fail (baseURL 不命中)
    b) 把 isCustomBackendEnabled 改成不查 profile → case 1 fail
    c) 把 getCustomBackendBaseUrl 改成 env 优先 → case 3 fail (env 字面赢)
    d) profile.name 字段不向 getCustomBackendName 透传 → case 1 fail (name='Custom backend' 而非 profile.name)

  apiKey 脱敏: 本测试输出 active_apikey 是真值 (验链路真透传), 不在 production CLI 输出里出现.
  M9_4 case_three_profiles 已守 desensitize 路径.
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

PROFILE_QWEN = {
    "provider": "openai-compatible",
    "baseURL": "https://profile-side.example.com/v1",
    "model": "profile-qwen-model",
    "apiKey": "sk-test-PROFILE-key-AAAAAAAAAAAAAAA",
    "name": "Profile-Side Qwen",
}

PROFILE_MINIMAX = {
    "provider": "openai-compatible",
    "baseURL": "https://minimax.example.com/v1",
    "model": "profile-minimax-model",
    "apiKey": "sk-test-PROFILE-key-BBBBBBBBBBBBBBB",
    "name": "Profile MiniMax",
}

ENV_BASE_URL = "https://env-side.example.com/v1"
ENV_MODEL = "env-side-model"
ENV_API_KEY = "sk-test-ENV-key-XXXXXXXXXXXXXXX"
ENV_NAME = "Env-Side Backend"

SNIPPET = (
    "import { enableConfigs } from './utils/config.ts';"
    "enableConfigs();"
    "const m = await import('./utils/customBackend.ts');"
    "const out = {"
    "  enabled: m.isCustomBackendEnabled(),"
    "  baseUrl: m.getCustomBackendBaseUrl(),"
    "  apiKey: m.getCustomBackendApiKey(),"
    "  model: m.getCustomBackendModel(),"
    "  name: m.getCustomBackendName(),"
    "  protocol: m.getCustomBackendProtocol(),"
    "  capabilityForProfileModel: m.customBackendCapabilityAppliesToModel('profile-qwen-model'),"
    "  capabilityForEnvModel: m.customBackendCapabilityAppliesToModel('env-side-model'),"
    "  capabilityForOther: m.customBackendCapabilityAppliesToModel('totally-other-model'),"
    "};"
    "process.stdout.write(JSON.stringify(out) + '\\n');"
)


def _bun_probe(env: dict, settings_payload: dict | None) -> tuple[int, str, str]:
    settings_dir = Path(env["MOSSEN_CONFIG_DIR"])
    settings_dir.mkdir(parents=True, exist_ok=True)
    settings_file = settings_dir / "settings.json"
    if settings_payload is not None:
        settings_file.write_text(json.dumps(settings_payload, indent=2), encoding="utf-8")
    elif settings_file.exists():
        settings_file.unlink()

    proc = subprocess.run(
        [RUN_BUN, "-e", SNIPPET],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_last_json(stdout: str) -> dict | None:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def _build_env_clean(ctx) -> dict:
    """fixture env, 清掉所有 MOSSEN_CODE_CUSTOM_* / 旧 env 残留, 让测试 case 自己决定塞什么"""
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    for k in list(env.keys()):
        if k.startswith("MOSSEN_CODE_CUSTOM") or k == "MOSSEN_CODE_USE_CUSTOM_BACKEND":
            env.pop(k, None)
    return env


def case_profile_only_no_env() -> dict:
    """active profile 真生效, 全 getter 走 profile 字面"""
    ctx = make_fixture("M9.5.profile_only")
    env = _build_env_clean(ctx)
    settings = {
        "mossen.profiles": {"qwen": PROFILE_QWEN},
        "mossen.activeProfile": "qwen",
    }
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<profile only>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("enabled") is True
        and parsed.get("baseUrl") == PROFILE_QWEN["baseURL"]
        and parsed.get("apiKey") == PROFILE_QWEN["apiKey"]
        and parsed.get("model") == PROFILE_QWEN["model"]
        and parsed.get("name") == PROFILE_QWEN["name"]
        and parsed.get("protocol") == "openai-compatible"
        and parsed.get("capabilityForProfileModel") is True
        and parsed.get("capabilityForOther") is False
    )
    return {
        "name": "M9_5_profile_only_no_env",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:400],
        "_ctx": ctx,
    }


def case_env_only_no_profile() -> dict:
    """没有 profiles, 全靠旧 env (Allen 当前现状, qwen 默认零破坏)"""
    ctx = make_fixture("M9.5.env_only")
    env = _build_env_clean(ctx)
    env.update({
        "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
        "MOSSEN_CODE_CUSTOM_BASE_URL": ENV_BASE_URL,
        "MOSSEN_CODE_CUSTOM_API_KEY": ENV_API_KEY,
        "MOSSEN_CODE_CUSTOM_MODEL": ENV_MODEL,
        "MOSSEN_CODE_CUSTOM_NAME": ENV_NAME,
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
    })
    rc, stdout, stderr = _bun_probe(env, settings_payload=None)
    write_command_log(ctx, [RUN_BUN, "-e", "<env only>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("enabled") is True
        and parsed.get("baseUrl") == ENV_BASE_URL
        and parsed.get("apiKey") == ENV_API_KEY
        and parsed.get("model") == ENV_MODEL
        and parsed.get("name") == ENV_NAME
        and parsed.get("protocol") == "openai-compatible"
        and parsed.get("capabilityForEnvModel") is True
        and parsed.get("capabilityForOther") is False
    )
    return {
        "name": "M9_5_env_only_no_profile_qwen_default_zero_break",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:400],
        "_ctx": ctx,
    }


def case_both_profile_priority() -> dict:
    """profile + env 同时存在 → profile 字面赢 (D-S09-3=P 优先级契约)"""
    ctx = make_fixture("M9.5.both")
    env = _build_env_clean(ctx)
    env.update({
        "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
        "MOSSEN_CODE_CUSTOM_BASE_URL": ENV_BASE_URL,
        "MOSSEN_CODE_CUSTOM_API_KEY": ENV_API_KEY,
        "MOSSEN_CODE_CUSTOM_MODEL": ENV_MODEL,
        "MOSSEN_CODE_CUSTOM_NAME": ENV_NAME,
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
    })
    settings = {
        "mossen.profiles": {"qwen": PROFILE_QWEN, "minimax": PROFILE_MINIMAX},
        "mossen.activeProfile": "minimax",
    }
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<both, profile=minimax>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("enabled") is True
        and parsed.get("baseUrl") == PROFILE_MINIMAX["baseURL"]
        and parsed.get("apiKey") == PROFILE_MINIMAX["apiKey"]
        and parsed.get("model") == PROFILE_MINIMAX["model"]
        and parsed.get("name") == PROFILE_MINIMAX["name"]
        # 反测: env 字面不能出现在结果
        and parsed.get("baseUrl") != ENV_BASE_URL
        and parsed.get("apiKey") != ENV_API_KEY
        and parsed.get("model") != ENV_MODEL
    )
    return {
        "name": "M9_5_active_profile_overrides_env",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:400],
        "_ctx": ctx,
    }


def case_no_active_profile_falls_back() -> dict:
    """profiles 存在但 activeProfile 缺失 → fallback 到旧 env"""
    ctx = make_fixture("M9.5.no_active")
    env = _build_env_clean(ctx)
    env.update({
        "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
        "MOSSEN_CODE_CUSTOM_BASE_URL": ENV_BASE_URL,
        "MOSSEN_CODE_CUSTOM_API_KEY": ENV_API_KEY,
        "MOSSEN_CODE_CUSTOM_MODEL": ENV_MODEL,
        "MOSSEN_CODE_CUSTOM_NAME": ENV_NAME,
    })
    settings = {
        "mossen.profiles": {"qwen": PROFILE_QWEN, "minimax": PROFILE_MINIMAX},
        # activeProfile 故意缺失
    }
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<no active>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("enabled") is True
        and parsed.get("baseUrl") == ENV_BASE_URL
        and parsed.get("model") == ENV_MODEL
        and parsed.get("name") == ENV_NAME
    )
    return {
        "name": "M9_5_no_active_profile_falls_back_to_env",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:400],
        "_ctx": ctx,
    }


def case_neither_profile_nor_env() -> dict:
    """
    profile 缺 + env 缺 → isCustomBackendEnabled=false, 全部 null.

    注意: 不能走 run-bun-featured.sh (会自动 source dev 机 .mossensrc/custom-backend.env,
    把 Allen qwen 字面塞进 env 不可清). 直接用 raw bun + bun-only env.
    """
    import os as _os
    ctx = make_fixture("M9.5.neither")
    env = _build_env_clean(ctx)

    settings_dir = ctx.mossen_config_home
    settings_dir.mkdir(parents=True, exist_ok=True)
    sf = settings_dir / "settings.json"
    if sf.exists():
        sf.unlink()

    bun_path = _os.environ.get("BUN_PATH") or "/opt/homebrew/bin/bun"
    proc = subprocess.run(
        [bun_path, "-e", SNIPPET],
        cwd=str(ROOT / "src"),  # bun -e 需要在源码目录跑, 让相对 import 解析
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    write_command_log(ctx, [bun_path, "-e", "<neither, raw bun>"], proc.stdout, proc.stderr, proc.returncode)
    parsed = _parse_last_json(proc.stdout) or {}

    ok = (
        proc.returncode == 0
        and parsed.get("enabled") is False
        and parsed.get("baseUrl") is None
        and parsed.get("apiKey") is None
        and parsed.get("model") is None
        # name 总是有 default 'Custom backend'
        and parsed.get("name") == "Custom backend"
    )
    return {
        "name": "M9_5_neither_profile_nor_env_disabled",
        "ok": ok,
        "exit_code": proc.returncode,
        "parsed": parsed,
        "stderr_excerpt": proc.stderr[:400],
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_profile_only_no_env(),
        case_env_only_no_profile(),
        case_both_profile_priority(),
        case_no_active_profile_falls_back(),
        case_neither_profile_nor_env(),
    ]
    summary_status = "passed" if all(c.get("ok") for c in cases) else "failed"

    last_ctx = cases[-1].pop("_ctx")
    for c in cases[:-1]:
        c.pop("_ctx", None)

    write_assertions(
        last_ctx,
        status=summary_status,
        assertions=[
            {
                "name": c["name"],
                "expected": True,
                "actual": c.get("ok"),
                "passed": c.get("ok"),
                "evidence": json.dumps(c.get("parsed"), ensure_ascii=False)[:400],
            }
            for c in cases
        ],
    )
    print(json.dumps({"status": summary_status, "results": cases}, indent=2, ensure_ascii=False))
    return 0 if summary_status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
