#!/usr/bin/env python3
"""
M9.12 — getCustomBackendAuthHeaders 必须按 protocol 选 auth header 风格.

Allen 2026-04-28 报: /model glm 之后请求成功; /model minimax → 401.
根因 (curl 验证):
  - MiniMax 严格只接受 `Authorization: Bearer <key>`, x-api-key → 401
  - GLM/qwen 宽松, 两者都接 (历史 x-api-key 跑得通)
  - mossen 之前对所有 profile 一律送 `x-api-key` (mossen-compatible style) → MiniMax 必挂
修复: utils/customBackend.ts getCustomBackendAuthHeaders 按 protocol 分支:
  openai-compatible → Authorization: Bearer <apiKey>
  其他 (mossen-compatible / private) → x-api-key (mossen-compatible style)

守护契约:
  case 1 — openai-compatible profile + apiKey
           → headers.Authorization === 'Bearer <key>'
           → headers['x-api-key'] === undefined
  case 2 — mossen-compatible profile + apiKey
           → headers['x-api-key'] === <key>
           → headers.Authorization === undefined
  case 3 — openai-compatible + 用户自定义 Authorization header (env)
           → 用户的 Authorization 保留, 不被覆盖
  case 4 — openai-compatible + 用户自定义 x-api-key header
           → 用户的 x-api-key 保留, 不写入 Authorization
  case 5 — authToken (env) > apiKey: authToken 优先填 Authorization,
           apiKey 不再触发任何 header (无双写)
  case 6 — apiKey 真值绝不出现在 stdout/stderr (本测试以 dump 方式输出 headers,
           固定真值会出现在 dumped headers value, 故只对其它非 dump 输出做 leak 检查)

反测信号:
  - 修复回退 → case 1 fail (Authorization 缺失, x-api-key 存在)
  - 误把 mossen-compatible 也切到 Bearer → case 2 fail
  - 用户 header 被覆盖 → case 3/4 fail
  - authToken/apiKey 双写 → case 5 fail (Authorization 应是 token-version, 不是 apiKey)
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


OAI_KEY = "sk-fake-oai-test-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
PROVIDER_KEY = "sk-fake-internal-test-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"
USER_BEARER = "user-supplied-bearer-CCCCCCCCCCCCCCCCCCCCCCCCC"
USER_XAPIKEY = "user-supplied-xkey-DDDDDDDDDDDDDDDDDDDDDDDDD"
AUTH_TOKEN = "sk-fake-token-test-EEEEEEEEEEEEEEEEEEEEEEEEEE"


def _make_env(ctx, extra: dict | None = None) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    for k in (
        "MOSSEN_CODE_USE_CUSTOM_BACKEND",
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_AUTH_TOKEN",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_CODE_CUSTOM_NAME",
        "MOSSEN_CODE_CUSTOM_HEADERS",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
        "MOSSEN_CONFIG_OVERRIDES",
        "MOSSEN_INTERNAL_FC_OVERRIDES",
    ):
        env.pop(k, None)
    if extra:
        env.update(extra)
    return env


def _seed_profile(home: Path, name: str, provider: str, apiKey: str) -> None:
    home.mkdir(parents=True, exist_ok=True)
    payload = {
        "mossen.profiles": {
            name: {
                "provider": provider,
                "baseURL": f"https://fake-{name}.example/v1",
                "model": f"{name}-model",
                "apiKey": apiKey,
            },
        },
        "mossen.activeProfile": name,
    }
    (home / "settings.json").write_text(json.dumps(payload, indent=2) + "\n")


def _dump_headers(env: dict) -> tuple[int, str, str]:
    snippet = (
        "const cb = await import('./utils/customBackend.ts');"
        "const headers = cb.getCustomBackendAuthHeaders();"
        "process.stdout.write(JSON.stringify({"
        "  headers,"
        "  protocol: cb.getCustomBackendProtocol(),"
        "  apiKeyLen: (cb.getCustomBackendApiKey() || '').length,"
        "}));"
    )
    proc = subprocess.run(
        ["bun", "-e", snippet],
        env=env, capture_output=True, text=True, timeout=60, cwd=str(ROOT),
    )
    return proc.returncode, proc.stdout, proc.stderr


def case_1_openai_compatible_uses_bearer(ctx) -> dict:
    home = ctx.mossen_config_home
    _seed_profile(home, "minimax_like", "openai-compatible", OAI_KEY)
    env = _make_env(ctx)
    rc, out, err = _dump_headers(env)
    write_command_log(ctx, ["bun", "dump headers", "(case 1 oai)"], out, err, rc)
    try:
        data = json.loads(out)
    except Exception:
        data = None
    if not data:
        return {"name": "case1_openai_compatible_uses_bearer", "ok": False,
                "exit_code": rc, "stdout_excerpt": out[:400], "stderr_excerpt": err[:400]}
    headers = data.get("headers", {})
    return {
        "name": "case1_openai_compatible_uses_bearer",
        "ok": (
            rc == 0
            and headers.get("Authorization") == f"Bearer {OAI_KEY}"
            and "x-api-key" not in headers
            and "X-Api-Key" not in headers
            and data.get("protocol") == "openai-compatible"
        ),
        "headers_keys": sorted(headers.keys()),
        "auth_value_prefix": (headers.get("Authorization") or "")[:20],
        "expected_auth_prefix": f"Bearer {OAI_KEY[:6]}",
    }


def case_2_mossen_compatible_uses_x_api_key(ctx) -> dict:
    home = ctx.mossen_config_home
    # Settings 不能直接配 mossen-compatible (ProfileSchema.provider 当前只允许 openai-compatible);
    # 用 env-based custom backend + 显式协议 env 触发 mossen-compatible 路径.
    if (home / "settings.json").exists():
        (home / "settings.json").unlink()
    env = _make_env(ctx, extra={
        "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
        "MOSSEN_CODE_CUSTOM_BASE_URL": "https://fake-provider.example/v1",
        "MOSSEN_CODE_CUSTOM_API_KEY": PROVIDER_KEY,
        "MOSSEN_CODE_CUSTOM_MODEL": "mossen-fake",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "mossen-compatible",
    })
    rc, out, err = _dump_headers(env)
    write_command_log(ctx, ["bun", "dump headers", "(case 2 mossen)"], out, err, rc)
    try:
        data = json.loads(out)
    except Exception:
        data = None
    if not data:
        return {"name": "case2_mossen_compatible_uses_x_api_key", "ok": False,
                "exit_code": rc, "stdout_excerpt": out[:400], "stderr_excerpt": err[:400]}
    headers = data.get("headers", {})
    return {
        "name": "case2_mossen_compatible_uses_x_api_key",
        "ok": (
            rc == 0
            and headers.get("x-api-key") == PROVIDER_KEY
            and "Authorization" not in headers
            and data.get("protocol") == "mossen-compatible"
        ),
        "headers_keys": sorted(headers.keys()),
        "xkey_value_prefix": (headers.get("x-api-key") or "")[:8],
    }


def case_3_user_authorization_header_takes_precedence(ctx) -> dict:
    home = ctx.mossen_config_home
    _seed_profile(home, "oai_with_user_auth", "openai-compatible", OAI_KEY)
    env = _make_env(ctx, extra={
        "MOSSEN_CODE_CUSTOM_HEADERS": json.dumps({
            "Authorization": f"Bearer {USER_BEARER}",
        }),
    })
    rc, out, err = _dump_headers(env)
    write_command_log(ctx, ["bun", "dump headers", "(case 3 user auth)"], out, err, rc)
    try:
        data = json.loads(out)
    except Exception:
        data = None
    if not data:
        return {"name": "case3_user_authorization_header_takes_precedence", "ok": False,
                "exit_code": rc, "stdout_excerpt": out[:400], "stderr_excerpt": err[:400]}
    headers = data.get("headers", {})
    return {
        "name": "case3_user_authorization_header_takes_precedence",
        "ok": (
            rc == 0
            and headers.get("Authorization") == f"Bearer {USER_BEARER}"
            # 不能既有用户 Bearer 又混入 profile apiKey 的 Bearer
            and OAI_KEY not in (headers.get("Authorization") or "")
            and "x-api-key" not in headers
        ),
        "headers_keys": sorted(headers.keys()),
        "auth_value": headers.get("Authorization", "")[:30],
    }


def case_4_user_xapikey_header_takes_precedence(ctx) -> dict:
    home = ctx.mossen_config_home
    _seed_profile(home, "oai_with_user_xkey", "openai-compatible", OAI_KEY)
    env = _make_env(ctx, extra={
        "MOSSEN_CODE_CUSTOM_HEADERS": json.dumps({
            "x-api-key": USER_XAPIKEY,
        }),
    })
    rc, out, err = _dump_headers(env)
    write_command_log(ctx, ["bun", "dump headers", "(case 4 user x-api-key)"], out, err, rc)
    try:
        data = json.loads(out)
    except Exception:
        data = None
    if not data:
        return {"name": "case4_user_xapikey_header_takes_precedence", "ok": False,
                "exit_code": rc, "stdout_excerpt": out[:400], "stderr_excerpt": err[:400]}
    headers = data.get("headers", {})
    return {
        "name": "case4_user_xapikey_header_takes_precedence",
        "ok": (
            rc == 0
            and headers.get("x-api-key") == USER_XAPIKEY
            # profile apiKey 既不该出现在 x-api-key 也不该出现在 Authorization
            and OAI_KEY not in (headers.get("x-api-key") or "")
            and OAI_KEY not in (headers.get("Authorization") or "")
        ),
        "headers_keys": sorted(headers.keys()),
        "xkey_value": headers.get("x-api-key", "")[:30],
    }


def case_5_authToken_overrides_apiKey_no_double_write(ctx) -> dict:
    """authToken (env-only) 存在时, Authorization=Bearer authToken; apiKey 不再被写入."""
    home = ctx.mossen_config_home
    _seed_profile(home, "oai_with_token", "openai-compatible", OAI_KEY)
    env = _make_env(ctx, extra={
        "MOSSEN_CODE_CUSTOM_AUTH_TOKEN": AUTH_TOKEN,
    })
    rc, out, err = _dump_headers(env)
    write_command_log(ctx, ["bun", "dump headers", "(case 5 token + apikey)"], out, err, rc)
    try:
        data = json.loads(out)
    except Exception:
        data = None
    if not data:
        return {"name": "case5_authToken_overrides_apiKey_no_double_write", "ok": False,
                "exit_code": rc, "stdout_excerpt": out[:400], "stderr_excerpt": err[:400]}
    headers = data.get("headers", {})
    return {
        "name": "case5_authToken_overrides_apiKey_no_double_write",
        "ok": (
            rc == 0
            and headers.get("Authorization") == f"Bearer {AUTH_TOKEN}"
            # apiKey 不写到 Authorization (会污染 token), 也不写到 x-api-key (双写违 openai)
            and OAI_KEY not in (headers.get("Authorization") or "")
            and "x-api-key" not in headers
            and "X-Api-Key" not in headers
        ),
        "headers_keys": sorted(headers.keys()),
        "auth_value_prefix": (headers.get("Authorization") or "")[:25],
    }


def case_6_no_apikey_in_unrelated_output(ctx) -> dict:
    """
    ApiKey 在 dump 模式下必然出现在 headers value (这是测试本身的目的);
    但其它意外位置 (stderr 警告, 调试日志) 不应泄漏.
    本 case 用 mossen --list-model-profiles JSON 路径验 (该路径自带脱敏).
    """
    home = ctx.mossen_config_home
    _seed_profile(home, "leak_check_oai", "openai-compatible", OAI_KEY)
    env = _make_env(ctx)
    proc = subprocess.run(
        ["mossen", "--list-model-profiles"],
        env=env, capture_output=True, text=True, timeout=30, cwd=str(ROOT),
    )
    write_command_log(ctx, ["mossen", "--list-model-profiles"], proc.stdout, proc.stderr, proc.returncode)
    leaked = OAI_KEY in proc.stdout or OAI_KEY in proc.stderr
    return {
        "name": "case6_apikey_never_in_list_model_profiles_output",
        "ok": proc.returncode == 0 and not leaked,
        "leaked": leaked,
        "stdout_len": len(proc.stdout),
        "stderr_len": len(proc.stderr),
    }


def main() -> int:
    ctx = make_fixture("M9.12_auth_header_per_protocol")
    results = [
        case_1_openai_compatible_uses_bearer(ctx),
        case_2_mossen_compatible_uses_x_api_key(ctx),
        case_3_user_authorization_header_takes_precedence(ctx),
        case_4_user_xapikey_header_takes_precedence(ctx),
        case_5_authToken_overrides_apiKey_no_double_write(ctx),
        case_6_no_apikey_in_unrelated_output(ctx),
    ]
    all_ok = all(r["ok"] for r in results)
    write_assertions(ctx, status="passed" if all_ok else "failed", assertions=results)
    print(json.dumps({
        "results": results,
        "passed": sum(1 for r in results if r["ok"]),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M9.12 (S1-09 三次回归): customBackend auth header 必须按 protocol 选风格, openai-compatible→Bearer, mossen-compatible→x-api-key.",
    }, indent=2, ensure_ascii=False))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
