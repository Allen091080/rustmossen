#!/usr/bin/env python3
"""
M9.8 — /model 交互态接 multi-profile schema (S1-09f P0).

按 Allen 补充要求 (S1-09 必须 done 的最后一块):
  1. /model 列出 profiles + 标 session active vs global default + 脱敏 apiKey
  2. /model <name> 切换 session-only profile (不修改全局默认)
  3. 切换后全局默认 ~/.mossen/settings.json 不变
  4. 未知 profile 给清晰错误 (列出可选 profiles)

  实现走 type='local' 文本输出, 直接调 commands/model/model.ts call(args, ctx).

  关键 case (5):
    1. case_list_no_profiles: profiles 为空 → 提示用 CLI 创建
    2. case_list_with_profiles: 写 3 profile + active=qwen → list 真展示 + apiKey 脱敏
    3. case_switch_session_only: /model minimax → session active 真切, ~/.mossen/settings.json 仍 active=qwen
    4. case_switch_unknown_profile: /model ghost → 错误清晰 + 列可选
    5. case_apikey_never_in_output: 任何输出都不能含真 apiKey 字面

  反测信号:
    a) commands/model/model.ts 改用 active profile (而非 session override) → settings.json 真被改 → case 3 fail
    b) formatList 不调 desensitizeProfile → apiKey 字面进 stdout → case 5 fail
    c) setSessionActiveProfile 写 'user' scope (而非 'override') → settings.json 被改 → case 3 fail
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

QWEN_KEY = "sk-test-qwen-AAAAAAAAAAAAAAAAAAAA"
MINIMAX_KEY = "sk-test-minimax-BBBBBBBBBBBBBBBBBB"
GLM_KEY = "sk-test-glm-CCCCCCCCCCCCCCCCCCCC"

THREE_PROFILES = {
    "qwen": {
        "provider": "openai-compatible",
        "baseURL": "https://example.com/qwen/v1",
        "model": "qwen-test",
        "apiKey": QWEN_KEY,
        "name": "Qwen Test",
    },
    "minimax": {
        "provider": "openai-compatible",
        "baseURL": "https://example.com/minimax/v1",
        "model": "minimax-test",
        "apiKey": MINIMAX_KEY,
    },
    "glm": {
        "provider": "openai-compatible",
        "baseURL": "https://example.com/glm/v1",
        "model": "glm-test",
        "apiKey": GLM_KEY,
    },
}


def _bun_invoke_model_command(
    env: dict, args: str, settings: dict | None, *, use_wrapper: bool = True,
) -> tuple[int, str, str]:
    """
    用 bun -e 直接 import /model command 的 call(), 模拟 slash command 调用.
    不真启 mossen REPL (避免 PTY 复杂度); call 拿 args + 最小 ctx, 验输出 text.

    use_wrapper=False: 走 raw bun, 不 source .mossensrc/custom-backend.env
    (case 1 "no fallback" 必须用此, 避免 wrapper 注入 MOSSEN_CODE_CUSTOM_*).
    """
    settings_dir = Path(env["MOSSEN_CONFIG_DIR"])
    settings_dir.mkdir(parents=True, exist_ok=True)
    sf = settings_dir / "settings.json"
    if settings is not None:
        sf.write_text(json.dumps(settings, indent=2), encoding="utf-8")
    elif sf.exists():
        sf.unlink()

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const mod = await import('./commands/model/model.tsx');"
        f"const result = await mod.call({json.dumps(args)}, {{}} as any);"
        "process.stdout.write(JSON.stringify(result) + '\\n');"
    )
    cmd = [RUN_BUN, "-e", snippet] if use_wrapper else ["bun", "-e", snippet]
    proc = subprocess.run(
        cmd,
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _parse_call_result(stdout: str) -> dict | None:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def _read_settings(env: dict) -> dict:
    sf = Path(env["MOSSEN_CONFIG_DIR"]) / "settings.json"
    if not sf.exists():
        return {}
    return json.loads(sf.read_text(encoding="utf-8"))


def _build_env(ctx) -> dict:
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    for k in list(env.keys()):
        if k.startswith("MOSSEN_CODE_CUSTOM") or k == "MOSSEN_CODE_USE_CUSTOM_BACKEND":
            env.pop(k, None)
    return env


def case_list_no_profiles() -> dict:
    ctx = make_fixture("M9.8.list_empty")
    env = _build_env(ctx)
    # 关键: use_wrapper=False — wrapper 会 source .mossensrc/custom-backend.env 注入
    # MOSSEN_CODE_CUSTOM_*, 与 case 设计 "0 settings + 0 env fallback" 冲突.
    rc, stdout, stderr = _bun_invoke_model_command(env, "", settings={}, use_wrapper=False)
    write_command_log(ctx, ["bun", "-e", "<call /model empty raw>"], stdout, stderr, rc)
    parsed = _parse_call_result(stdout) or {}
    text = parsed.get("value", "")

    ok = (
        rc == 0
        and parsed.get("type") == "text"
        and "No model profiles configured" in text
        and "mossen --add-model-profile" in text
        and "mossen --set-model-profile" in text
    )
    return {
        "name": "M9_8_list_no_profiles_shows_creation_hint",
        "ok": ok,
        "exit_code": rc,
        "text_excerpt": text[:300],
        "_ctx": ctx,
    }


def case_list_with_profiles() -> dict:
    ctx = make_fixture("M9.8.list_three")
    env = _build_env(ctx)
    settings = {
        "mossen.profiles": THREE_PROFILES,
        "mossen.activeProfile": "qwen",
    }
    rc, stdout, stderr = _bun_invoke_model_command(env, "", settings=settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<call /model list>"], stdout, stderr, rc)
    parsed = _parse_call_result(stdout) or {}
    text = parsed.get("value", "")

    expected_qwen_mask = f"{QWEN_KEY[:6]}...{QWEN_KEY[-4:]}"
    expected_minimax_mask = f"{MINIMAX_KEY[:6]}...{MINIMAX_KEY[-4:]}"
    expected_glm_mask = f"{GLM_KEY[:6]}...{GLM_KEY[-4:]}"

    ok = (
        rc == 0
        and parsed.get("type") == "text"
        and "Model profiles (3)" in text
        and "qwen [session, default]" in text
        and "minimax" in text
        and "glm" in text
        and expected_qwen_mask in text
        and expected_minimax_mask in text
        and expected_glm_mask in text
        # 强契约: 真 apiKey 不出现在 text 里
        and QWEN_KEY not in text
        and MINIMAX_KEY not in text
        and GLM_KEY not in text
        and "Current session profile: qwen" in text
        and "Global default profile:  qwen" in text
    )
    return {
        "name": "M9_8_list_three_profiles_with_tags_and_desensitize",
        "ok": ok,
        "exit_code": rc,
        "text_excerpt": text[:600],
        "raw_apikey_leak_check": {
            "qwen": QWEN_KEY in text,
            "minimax": MINIMAX_KEY in text,
            "glm": GLM_KEY in text,
        },
        "_ctx": ctx,
    }


def case_switch_session_only() -> dict:
    """关键: /model minimax 切换 session, settings.json activeProfile 字面仍是 qwen."""
    ctx = make_fixture("M9.8.switch_session")
    env = _build_env(ctx)
    settings = {
        "mossen.profiles": THREE_PROFILES,
        "mossen.activeProfile": "qwen",  # global default = qwen
    }
    # 注意: 同一 bun -e 进程内 setSessionActiveProfile + read settings.json,
    # 所以一次 call 里测 session 切换 + settings 文件未改.
    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const mod = await import('./commands/model/model.tsx');"
        "const before = await mod.call('', {} as any);"
        "const switchResult = await mod.call('minimax', {} as any);"
        "const after = await mod.call('', {} as any);"
        "const fs = await import('node:fs');"
        "const path = await import('node:path');"
        "const settingsPath = path.join(process.env.MOSSEN_CONFIG_DIR, 'settings.json');"
        "const settingsAfter = JSON.parse(fs.readFileSync(settingsPath, 'utf-8'));"
        "process.stdout.write(JSON.stringify({"
        "  before_text: before.value,"
        "  switch_text: switchResult.value,"
        "  after_text: after.value,"
        "  settings_after: settingsAfter,"
        "}) + '\\n');"
    )

    settings_dir = Path(env["MOSSEN_CONFIG_DIR"])
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "settings.json").write_text(json.dumps(settings, indent=2), encoding="utf-8")

    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    write_command_log(ctx, [RUN_BUN, "-e", "<switch session 3-step>"], proc.stdout, proc.stderr, proc.returncode)
    parsed = _parse_call_result(proc.stdout) or {}

    before_text = parsed.get("before_text", "")
    switch_text = parsed.get("switch_text", "")
    after_text = parsed.get("after_text", "")
    settings_after = parsed.get("settings_after") or {}

    ok = (
        proc.returncode == 0
        # before: session=qwen, default=qwen
        and "Current session profile: qwen" in before_text
        and "Global default profile:  qwen" in before_text
        # switch result
        and 'Switched session profile to "minimax"' in switch_text
        and "Global default profile remains \"qwen\"" in switch_text
        and "this only affects the current session" in switch_text
        # after: session=minimax, default=qwen (override 改, 文件不改)
        and "Current session profile: minimax" in after_text
        and "Global default profile:  qwen" in after_text
        and "Session has been overridden — restart mossen to revert to \"qwen\"" in after_text
        # 文件没被改: activeProfile 字面仍是 qwen
        and settings_after.get("mossen.activeProfile") == "qwen"
        # 真 apiKey 都不在任何 text 里
        and QWEN_KEY not in before_text
        and QWEN_KEY not in switch_text
        and QWEN_KEY not in after_text
        and MINIMAX_KEY not in before_text
        and MINIMAX_KEY not in switch_text
        and MINIMAX_KEY not in after_text
    )
    return {
        "name": "M9_8_switch_session_only_settings_file_unchanged",
        "ok": ok,
        "exit_code": proc.returncode,
        "settings_active_after_switch": settings_after.get("mossen.activeProfile"),
        "before_excerpt": before_text[:200],
        "switch_excerpt": switch_text[:300],
        "after_excerpt": after_text[:300],
        "stderr_excerpt": proc.stderr[:200],
        "_ctx": ctx,
    }


def case_switch_unknown_profile() -> dict:
    ctx = make_fixture("M9.8.switch_unknown")
    env = _build_env(ctx)
    settings = {
        "mossen.profiles": {"qwen": THREE_PROFILES["qwen"]},
        "mossen.activeProfile": "qwen",
    }
    rc, stdout, stderr = _bun_invoke_model_command(env, "ghost", settings=settings)
    write_command_log(ctx, [RUN_BUN, "-e", "<call /model ghost>"], stdout, stderr, rc)
    parsed = _parse_call_result(stdout) or {}
    text = parsed.get("value", "")

    ok = (
        rc == 0  # call 不抛错, 只返 text 错误信息
        and parsed.get("type") == "text"
        and 'Cannot switch to profile "ghost"' in text
        and "Available profiles: qwen" in text
        and "/model" in text
        and QWEN_KEY not in text
    )
    return {
        "name": "M9_8_switch_unknown_profile_clear_error_with_options",
        "ok": ok,
        "exit_code": rc,
        "text_excerpt": text[:400],
        "_ctx": ctx,
    }


def case_apikey_never_in_output() -> dict:
    """覆盖性检查: 多场景 + 多 profile, 真 apiKey 字面在任意输出都不应出现"""
    ctx = make_fixture("M9.8.apikey_safety")
    env = _build_env(ctx)
    settings = {
        "mossen.profiles": THREE_PROFILES,
        "mossen.activeProfile": "qwen",
    }
    settings_dir = Path(env["MOSSEN_CONFIG_DIR"])
    settings_dir.mkdir(parents=True, exist_ok=True)
    (settings_dir / "settings.json").write_text(json.dumps(settings, indent=2), encoding="utf-8")

    snippet = (
        "import { enableConfigs } from './utils/config.ts';"
        "enableConfigs();"
        "const mod = await import('./commands/model/model.tsx');"
        "const a = await mod.call('', {} as any);"
        "const b = await mod.call('minimax', {} as any);"
        "const c = await mod.call('glm', {} as any);"
        "const d = await mod.call('', {} as any);"
        "const e = await mod.call('ghost', {} as any);"
        "const all = a.value + '\\n' + b.value + '\\n' + c.value + '\\n' + d.value + '\\n' + e.value;"
        "process.stdout.write(JSON.stringify({ all_text: all }) + '\\n');"
    )
    proc = subprocess.run(
        [RUN_BUN, "-e", snippet],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    write_command_log(ctx, [RUN_BUN, "-e", "<apikey safety multi>"], proc.stdout, proc.stderr, proc.returncode)
    parsed = _parse_call_result(proc.stdout) or {}
    all_text = parsed.get("all_text", "")

    ok = (
        proc.returncode == 0
        and len(all_text) > 100  # 5 个 call 结果都收到
        # 真 apiKey 三个都不出现
        and QWEN_KEY not in all_text
        and MINIMAX_KEY not in all_text
        and GLM_KEY not in all_text
        # 但脱敏版应出现 (说明 call 真有调用)
        and f"{QWEN_KEY[:6]}...{QWEN_KEY[-4:]}" in all_text
        and f"{MINIMAX_KEY[:6]}...{MINIMAX_KEY[-4:]}" in all_text
    )
    return {
        "name": "M9_8_apikey_never_in_any_output_across_5_calls",
        "ok": ok,
        "exit_code": proc.returncode,
        "all_text_length": len(all_text),
        "qwen_key_leak": QWEN_KEY in all_text,
        "minimax_key_leak": MINIMAX_KEY in all_text,
        "glm_key_leak": GLM_KEY in all_text,
        "_ctx": ctx,
    }


def main() -> int:
    cases = [
        case_list_no_profiles(),
        case_list_with_profiles(),
        case_switch_session_only(),
        case_switch_unknown_profile(),
        case_apikey_never_in_output(),
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
                "evidence": json.dumps(c, ensure_ascii=False)[:500],
            }
            for c in cases
        ],
    )
    print(json.dumps({"status": summary_status, "results": cases}, indent=2, ensure_ascii=False))
    return 0 if summary_status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
