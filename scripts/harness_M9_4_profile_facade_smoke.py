#!/usr/bin/env python3
"""
M9.4 — multi-profile schema 通过 services/config facade 真读 + apiKey 脱敏 (S1-09a P0).

按 Stage1-CLI基线加固.md §11.6 验收契约:
  当前 slice (S1-09a): 仅验"settings.json 顶层 mossen.profiles + mossen.activeProfile
  能通过 facade.resolveMossenConfig 真读出来; getProfiles/getActiveProfile/desensitize
  契约成立"。不验 customBackend.ts 行为变化 (留 S1-09b)。

  关键链路:
    fixture HOME 写 ~/.mossen/settings.json (含 mossen.profiles + mossen.activeProfile)
    → bun -e import services/config/index.ts {getProfiles, getActiveProfile, desensitizeProfiles}
    → 验输出 JSON 真含三 profile (qwen/minimax/glm) + activeProfile=qwen
    → 验 desensitizeProfiles 输出 apiKey 已脱敏 (前 6 + ... + 后 4)

  反测信号:
    a) 删 services/config/profiles.ts getProfiles 内 validateProfile 校验 → 非法 entry 不被过滤 → 测试 case_invalid_entry 失败
    b) 改 maskApiKey 直接返回 apiKey → 脱敏断言失败
    c) 改 getProfiles 不读 facade 而读 process.env → fixture HOME 隔离失效 → 测试拿不到 profile → 失败

注意: 当前测试用 fake apiKey 字面 (sk-test-XXX), 不依赖任何外网/真实 LLM API。
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

QWEN_KEY = "sk-test-qwen-1234567890abcdef"
MINIMAX_KEY = "sk-test-minimax-abcdefg1234567"
GLM_KEY = "sk-test-glm-zyxwvut0987654321"

VALID_PROFILES = {
    "qwen": {
        "provider": "openai-compatible",
        "baseURL": "https://coding.dashscope.aliyuncs.com/v1",
        "model": "qwen3.6-plus",
        "apiKey": QWEN_KEY,
        "name": "Qwen 3.6 Plus",
    },
    "minimax": {
        "provider": "openai-compatible",
        "baseURL": "https://api.minimaxi.com/v1",
        "model": "MiniMax-M2.7",
        "apiKey": MINIMAX_KEY,
    },
    "glm": {
        "provider": "openai-compatible",
        "baseURL": "https://open.bigmodel.cn/api/coding/paas/v4",
        "model": "glm-5.1",
        "apiKey": GLM_KEY,
    },
}


def _bun_probe(env: dict, settings_payload: dict) -> tuple[int, str, str]:
    """
    在 fixture HOME 写 settings.json, 然后 bun -e probe getProfiles/getActiveProfile/desensitize.
    """
    settings_dir = Path(env["MOSSEN_CONFIG_DIR"])
    settings_dir.mkdir(parents=True, exist_ok=True)
    settings_file = settings_dir / "settings.json"
    settings_file.write_text(json.dumps(settings_payload, indent=2), encoding="utf-8")

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const mod = await import('./services/config/index.ts');"
        "const profiles = mod.getProfiles();"
        "const activeName = mod.getActiveProfileName();"
        "const active = mod.getActiveProfile();"
        "const desensitized = mod.desensitizeProfiles(profiles);"
        "process.stdout.write(JSON.stringify({"
        "  profiles_count: Object.keys(profiles).length,"
        "  profile_names: Object.keys(profiles).sort(),"
        "  active_name: activeName,"
        "  active_baseURL: active ? active.baseURL : null,"
        "  active_model: active ? active.model : null,"
        "  active_apiKey_raw: active ? active.apiKey : null,"
        "  desensitized: desensitized,"
        "}) + '\\n');"
    )

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
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


def case_three_profiles_active_qwen() -> dict:
    ctx = make_fixture("M9.4.three_profiles")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings = {
        "mossen.profiles": VALID_PROFILES,
        "mossen.activeProfile": "qwen",
    }
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<probe getProfiles three>"], stdout, stderr, rc)

    parsed = _parse_last_json(stdout) or {}
    desensitized = parsed.get("desensitized") or {}
    qwen_d = desensitized.get("qwen") or {}
    minimax_d = desensitized.get("minimax") or {}
    glm_d = desensitized.get("glm") or {}

    expected_qwen_mask = f"{QWEN_KEY[:6]}...{QWEN_KEY[-4:]}"
    expected_minimax_mask = f"{MINIMAX_KEY[:6]}...{MINIMAX_KEY[-4:]}"
    expected_glm_mask = f"{GLM_KEY[:6]}...{GLM_KEY[-4:]}"

    ok = (
        rc == 0
        and parsed.get("profiles_count") == 3
        and parsed.get("profile_names") == ["glm", "minimax", "qwen"]
        and parsed.get("active_name") == "qwen"
        and parsed.get("active_baseURL") == VALID_PROFILES["qwen"]["baseURL"]
        and parsed.get("active_model") == VALID_PROFILES["qwen"]["model"]
        and parsed.get("active_apiKey_raw") == QWEN_KEY
        and qwen_d.get("apiKey") == expected_qwen_mask
        and minimax_d.get("apiKey") == expected_minimax_mask
        and glm_d.get("apiKey") == expected_glm_mask
        and qwen_d.get("baseURL") == VALID_PROFILES["qwen"]["baseURL"]
        and qwen_d.get("model") == VALID_PROFILES["qwen"]["model"]
    )

    return {
        "name": "M9_4_three_profiles_facade_read_and_desensitize",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:500],
        "_ctx": ctx,
    }


def case_active_points_to_missing_profile() -> dict:
    """activeProfile 指向不存在的 profile → getActiveProfile 应返回 null (不抛错)"""
    ctx = make_fixture("M9.4.active_missing")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings = {
        "mossen.profiles": {"qwen": VALID_PROFILES["qwen"]},
        "mossen.activeProfile": "ghost",
    }
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<probe active missing>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("profiles_count") == 1
        and parsed.get("active_name") is None
        and parsed.get("active_baseURL") is None
    )
    return {
        "name": "M9_4_active_points_to_missing_profile_returns_null",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:500],
        "_ctx": ctx,
    }


def case_invalid_entry_filtered_out() -> dict:
    """非法 entry (缺 baseURL / 缺 apiKey / 错误 provider) 应被 getProfiles 过滤掉"""
    ctx = make_fixture("M9.4.invalid_entry")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings = {
        "mossen.profiles": {
            "good": VALID_PROFILES["qwen"],
            "missing_apikey": {
                "provider": "openai-compatible",
                "baseURL": "https://example.com/v1",
                "model": "x",
            },
            "wrong_provider": {
                "provider": "provider",
                "baseURL": "https://example.com",
                "model": "x",
                "apiKey": "k",
            },
            "not_an_object": "string-value",
        },
        "mossen.activeProfile": "good",
    }
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<probe invalid filter>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("profiles_count") == 1
        and parsed.get("profile_names") == ["good"]
        and parsed.get("active_name") == "good"
    )
    return {
        "name": "M9_4_invalid_entries_filtered_by_validate",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:500],
        "_ctx": ctx,
    }


def case_no_profiles_field() -> dict:
    """settings.json 完全没有 mossen.profiles 字段 → getProfiles 返回 {}, getActiveProfile null"""
    ctx = make_fixture("M9.4.no_profiles")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings = {"unrelated": "value"}
    rc, stdout, stderr = _bun_probe(env, settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<probe no profiles>"], stdout, stderr, rc)
    parsed = _parse_last_json(stdout) or {}

    ok = (
        rc == 0
        and parsed.get("profiles_count") == 0
        and parsed.get("active_name") is None
    )
    return {
        "name": "M9_4_no_profiles_field_returns_empty",
        "ok": ok,
        "exit_code": rc,
        "parsed": parsed,
        "stderr_excerpt": stderr[:500],
        "_ctx": ctx,
    }


def case_crud_full_lifecycle() -> dict:
    """
    完整 CRUD + setActive 链路 (Workbench / UI 预留 API).
      1. 起始: 空 settings.json
      2. setProfile('alpha', schema)         → list = ['alpha']
      3. setProfile('beta', schema2)         → list = ['alpha', 'beta']
      4. setActiveProfile('beta')            → activeName = 'beta'
      5. setProfile('alpha', updated_schema) → alpha.model 真换
      6. deleteProfile('beta')               → list = ['alpha'], activeName = null (clear cascade)
      7. clearActiveProfile()                → activeName 仍 null (no-op)
      8. validateProfileName('123-bad')      → ok=false (字母开头)
    用同一 bun -e 跑完所有步骤, 同一 settings.json 真持久化跨步骤.
    """
    ctx = make_fixture("M9.4.crud_lifecycle")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    settings_dir = ctx.mossen_config_home
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "settings.json").write_text(json.dumps({}), encoding="utf-8")

    alpha_v1 = {
        "provider": "openai-compatible",
        "baseURL": "https://example.com/v1",
        "model": "alpha-1",
        "apiKey": "sk-test-alpha-AAAAAAAAAAAAAAAA",
    }
    alpha_v2 = {**alpha_v1, "model": "alpha-2"}
    beta = {
        "provider": "openai-compatible",
        "baseURL": "https://example.com/v1",
        "model": "beta-1",
        "apiKey": "sk-test-beta-BBBBBBBBBBBBBBBB",
    }

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const m = await import('./services/config/index.ts');"
        f"const ALPHA_V1 = {json.dumps(alpha_v1)};"
        f"const ALPHA_V2 = {json.dumps(alpha_v2)};"
        f"const BETA = {json.dumps(beta)};"
        "const trace = [];"
        # Step 1: empty
        "trace.push({step: 'init', list: Object.keys(m.getProfiles()).sort(), active: m.getActiveProfileName()});"
        # Step 2: create alpha
        "m.setProfile('alpha', ALPHA_V1);"
        "trace.push({step: 'create_alpha', list: Object.keys(m.getProfiles()).sort(), alpha_model: m.getProfileByName('alpha')?.model});"
        # Step 3: create beta
        "m.setProfile('beta', BETA);"
        "trace.push({step: 'create_beta', list: Object.keys(m.getProfiles()).sort()});"
        # Step 4: setActive beta
        "m.setActiveProfile('beta');"
        "trace.push({step: 'activate_beta', active: m.getActiveProfileName(), active_model: m.getActiveProfile()?.model});"
        # Step 5: update alpha
        "m.setProfile('alpha', ALPHA_V2);"
        "trace.push({step: 'update_alpha', alpha_model: m.getProfileByName('alpha')?.model});"
        # Step 6: delete beta (cascade clear active)
        "const delResult = m.deleteProfile('beta');"
        "trace.push({step: 'delete_beta', deleted: delResult.deleted, active_cleared: delResult.activeProfileCleared, list: Object.keys(m.getProfiles()).sort(), active: m.getActiveProfileName()});"
        # Step 7: clearActiveProfile no-op
        "m.clearActiveProfile();"
        "trace.push({step: 'clear_active_noop', active: m.getActiveProfileName()});"
        # Step 8: bad name
        "const badName = m.validateProfileName('123-bad');"
        "trace.push({step: 'bad_name', ok: badName.ok});"
        # Step 9: bad schema (missing apiKey) — setProfile must throw
        "let throwMsg = null;"
        "try { m.setProfile('charlie', {provider: 'openai-compatible', baseURL: 'x', model: 'y'}); }"
        "catch (e) { throwMsg = String(e.message || e); }"
        "trace.push({step: 'invalid_schema_throws', throw_msg: throwMsg});"
        # Step 10: setActive on missing profile must throw
        "let actThrow = null;"
        "try { m.setActiveProfile('ghost'); }"
        "catch (e) { actThrow = String(e.message || e); }"
        "trace.push({step: 'activate_missing_throws', throw_msg: actThrow});"
        "process.stdout.write(JSON.stringify({ trace }) + '\\n');"
    )

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    write_command_log(ctx, [RUN_BUN, "-e", "<probe CRUD>"], proc.stdout, proc.stderr, proc.returncode)
    parsed = _parse_last_json(proc.stdout) or {}
    trace = parsed.get("trace") or []
    by_step = {t["step"]: t for t in trace if isinstance(t, dict) and "step" in t}

    # 跨步骤验真持久化 + 真行为
    ok = (
        proc.returncode == 0
        and by_step.get("init", {}).get("list") == []
        and by_step.get("init", {}).get("active") is None
        and by_step.get("create_alpha", {}).get("list") == ["alpha"]
        and by_step.get("create_alpha", {}).get("alpha_model") == "alpha-1"
        and by_step.get("create_beta", {}).get("list") == ["alpha", "beta"]
        and by_step.get("activate_beta", {}).get("active") == "beta"
        and by_step.get("activate_beta", {}).get("active_model") == "beta-1"
        and by_step.get("update_alpha", {}).get("alpha_model") == "alpha-2"
        and by_step.get("delete_beta", {}).get("deleted") is True
        and by_step.get("delete_beta", {}).get("active_cleared") is True
        and by_step.get("delete_beta", {}).get("list") == ["alpha"]
        and by_step.get("delete_beta", {}).get("active") is None
        and by_step.get("clear_active_noop", {}).get("active") is None
        and by_step.get("bad_name", {}).get("ok") is False
        and isinstance(by_step.get("invalid_schema_throws", {}).get("throw_msg"), str)
        and "apiKey" in (by_step.get("invalid_schema_throws", {}).get("throw_msg") or "")
        and isinstance(by_step.get("activate_missing_throws", {}).get("throw_msg"), str)
        and "ghost" in (by_step.get("activate_missing_throws", {}).get("throw_msg") or "")
    )

    # 验 settings.json 真在磁盘上持久化 (跨进程 — 这里同进程, 但读盘验内容)
    persisted = json.loads((settings_dir / "settings.json").read_text(encoding="utf-8"))
    persisted_profiles = persisted.get("mossen.profiles") or {}
    apikey_alpha_persisted = (persisted_profiles.get("alpha") or {}).get("apiKey")

    ok_persisted = (
        list(persisted_profiles.keys()) == ["alpha"]
        and persisted_profiles["alpha"]["model"] == "alpha-2"
        and apikey_alpha_persisted == alpha_v1["apiKey"]
        and persisted.get("mossen.activeProfile") in (None, "")
    )

    return {
        "name": "M9_4_crud_full_lifecycle_with_persistence",
        "ok": bool(ok and ok_persisted),
        "exit_code": proc.returncode,
        "trace_steps": list(by_step.keys()),
        "persisted_profiles_keys": list(persisted_profiles.keys()),
        "persisted_alpha_model": persisted_profiles.get("alpha", {}).get("model"),
        "stderr_excerpt": proc.stderr[:800],
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_three_profiles_active_qwen(),
        case_active_points_to_missing_profile(),
        case_invalid_entry_filtered_out(),
        case_no_profiles_field(),
        case_crud_full_lifecycle(),
    ]

    summary_status = "passed" if all(c.get("ok") for c in cases) else "failed"

    # 用第一个 ctx 写汇总 (per harness 惯例 — 各 case 的 fixture 各自独立, 但 assertions.json 写最后一个 case 的目录)
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
                "evidence": json.dumps(c.get("parsed"), ensure_ascii=False)[:500],
            }
            for c in cases
        ],
    )

    out = {
        "status": summary_status,
        "results": cases,
    }
    print(json.dumps(out, indent=2, ensure_ascii=False))
    return 0 if summary_status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
