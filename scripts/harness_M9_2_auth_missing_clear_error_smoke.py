#!/usr/bin/env python3
"""
M9.2 — Auth 缺失时报错明确指向 custom backend 配置, 不引导官方 hosted 登录。

按 harness全链路测试.md §C.1 / §C.4 契约:
  Mossen 是个人版 + custom backend, 用户不会去 claude.ai 登录。
  当 API key/auth token 缺失时:
    - 错误必须指向 MOSSEN_CODE_CUSTOM_API_KEY / MOSSEN_CODE_CUSTOM_BASE_URL 等
    - 不得出现 "claude.ai/login", "Please login at", hosted OAuth 引导

  前置:
    - fixture HOME 隔离
    - 启用 custom backend (MOSSEN_CODE_USE_CUSTOM_BACKEND=1) + base url
    - 故意删除 MOSSEN_CODE_CUSTOM_API_KEY / MOSSEN_CODE_CUSTOM_AUTH_TOKEN
    - 同时清空 ANTHROPIC_*, 防止 hosted 路径 fallback
  步骤:
    mossen -p simple prompt
  观察点 (强契约):
    1. exit_code != 0  (启动应失败 / API 调用应失败)
       OR stdout/stderr 含明确 custom backend 配置错误提示
    2. stderr 或 stdout 含 custom backend 配置 hint:
       至少含一项: "MOSSEN_CODE_CUSTOM_API_KEY" / "MOSSEN_CODE_CUSTOM_AUTH_TOKEN"
       / "MOSSEN_CODE_CUSTOM_BASE_URL" / "custom backend"
    3. stderr/stdout 不含 hosted OAuth 引导:
       禁止: "claude.ai/login", "Please login at claude", "anthropic.com/login"
  反测信号:
    把 src/utils/preflightChecks.tsx 提示文案改为 "Please login at claude.ai"
    → 测试 fail (hosted 字面命中黑名单)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = str(ROOT / "run-mossen.sh")

# 必须出现至少 1 项 (white-list, 证明错误真指向 custom backend)
EXPECTED_HINT_TOKENS = (
    "MOSSEN_CODE_CUSTOM_API_KEY",
    "MOSSEN_CODE_CUSTOM_AUTH_TOKEN",
    "MOSSEN_CODE_CUSTOM_BASE_URL",
    "custom backend",
    "Custom backend",
    "custom-backend",
)

# 禁止出现 (black-list, hosted OAuth 引导)
FORBIDDEN_HOSTED_TOKENS = (
    "claude.ai/login",
    "Please login at claude",
    "anthropic.com/login",
    "api.mossen.invalid",
    "/login at claude",
)


def case_auth_missing_clear_error() -> dict:
    ctx = make_fixture("M9.2")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    # 真 fix: 不继承 os.environ — 完全 minimal env (只留 HOME+PATH+TERM 等基础).
    # subprocess 默认继承父 env, 父 env 含 user 真实 backend key 让 mossen 不报错.
    # 用 minimal env 让 mossen 真看到"backend 没配置"场景.
    minimal_env = {
        "HOME": str(ctx.home_dir),
        "PATH": "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
        "TERM": "xterm",
        "LANG": "en_US.UTF-8",
        "MOSSEN_CONFIG_DIR": str(ctx.mossen_config_home),
        "MOSSENSRC_LAUNCH_CWD": str(fixture_cwd),
        # 只启用 custom backend, 不设 base_url 不设 key (完全无 backend 配置场景)
        # mossen 应该明确报 "No Mossen backend is configured. ... set MOSSEN_CODE_CUSTOM_BASE_URL ..."
        "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
    }

    prompt = "say hi"

    # 直接调 bun 跳过 wrapper, 用 minimal_env 完全控制
    p = subprocess.Popen(
        ["bun", "entrypoints/cli.tsx", "-p"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        env=minimal_env, text=True, cwd=str(ROOT),
    )
    try:
        proc_stdout, proc_stderr = p.communicate(input=prompt, timeout=30)
        proc_returncode = p.returncode
        timed_out = False
    except subprocess.TimeoutExpired:
        p.kill()
        proc_stdout, proc_stderr = p.communicate()
        proc_returncode = -9
        timed_out = True

    write_command_log(
        ctx,
        [RUN_MOSSEN, "-p"],
        proc_stdout, proc_stderr, proc_returncode,
    )

    combined = (proc_stdout or "") + "\n" + (proc_stderr or "")

    # 1) 失败信号: exit != 0 OR 错误信息出现
    matched_hints = [tok for tok in EXPECTED_HINT_TOKENS if tok in combined]
    matched_forbidden = [tok for tok in FORBIDDEN_HOSTED_TOKENS if tok in combined]

    failed_or_clear_error = (proc_returncode != 0) or len(matched_hints) > 0
    has_custom_backend_hint = len(matched_hints) > 0
    has_forbidden_hosted = len(matched_forbidden) > 0

    # 强契约 (修正后, 不接受 timeout 当 pass):
    #   (a) mossen 真返错误退出 (非 timeout kill)
    #   (b) stdout/stderr 真含 custom backend hint 字面 (e.g. MOSSEN_CODE_CUSTOM_API_KEY)
    #   (c) 不含 hosted OAuth 引导
    ok = (
        not timed_out
        and proc_returncode != 0
        and has_custom_backend_hint
        and not has_forbidden_hosted
    )

    return {
        "name": "auth_missing_clear_error",
        "ok": ok,
        "exit_code": proc_returncode,
        "timed_out": timed_out,
        "failed_or_clear_error": failed_or_clear_error,
        "has_custom_backend_hint": has_custom_backend_hint,
        "has_forbidden_hosted": has_forbidden_hosted,
        "matched_hints": matched_hints,
        "matched_forbidden": matched_forbidden,
        "stdout_excerpt": proc_stdout[:400],
        "stderr_excerpt": proc_stderr[:400],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = case_auth_missing_clear_error()
    ctx = res1.pop("_ctx")
    results = [res1]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {
                "name": r["name"],
                "expected": True,
                "actual": r.get("ok"),
                "passed": r.get("ok"),
                "evidence": (
                    f"exit={r.get('exit_code')} "
                    f"failed_or_err={r.get('failed_or_clear_error')} "
                    f"hints={r.get('matched_hints')} "
                    f"forbidden={r.get('matched_forbidden')}"
                ),
            }
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M9.2 删 MOSSEN_CODE_CUSTOM_API_KEY+AUTH_TOKEN 后真启 mossen -p: "
            "必须出现 custom backend 配置 hint (MOSSEN_CODE_CUSTOM_*/'custom backend') "
            "且不得出现 hosted OAuth 引导 (claude.ai/login 等)"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
