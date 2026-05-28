#!/usr/bin/env python3
"""
R7 — 远程 GrowthBook 0 流量的安全网测试 (G2-2c, weak mode).

按 GrowthBook迁移计划.md §1.3 + G0-5 测试矩阵 §R7.

守护契约 (随 slice 渐进收紧):
  - G2-2c 当前: weak mode — exit 0 + session 落盘 = pass; GB 请求只记录不 fail
  - G6-2 后切 strict: GB 请求计数必须 == 0 (init 路径已删)
  - G6-3 后强 strict: + 不再写入 ~/.mossen.json 的 cachedGrowthBookFeatures

设计要点 (D-G05-B = b):
  - 只重定向 MOSSEN_CODE_GB_BASE_URL → mock
  - 不重定向 MOSSEN_CODE_PLATFORM_BASE_URL (growthbook.ts 已优先读 GB_BASE_URL)
  - 强制开 trust dialog (绕守卫让代码真试图访问 GB)
  - 模型 backend env 不动 (custom backend / external provider 走真 API)

2 user_type case:
  - default (USER_TYPE unset, 非 internal) — 标准路径, 必须 exit 0 + session 落盘
  - USER_TYPE=internal — Mossen 个人版已 strip internal 代码 (REPLTool 等仅 internal 工具不存在),
    该路径在 require 时即抛 module-not-found. 这其实是更强的 0-GB 保证 (proc
    在 init 前就死了, 根本没机会发请求). 测试视为符合契约的 "blocked-but-safe".

GB endpoint 形状 (mock 收到任意一个就算 GB 流量):
  - /api/features/...  (GrowthBook SDK feature endpoint)
  - /api/eval/...      (GrowthBook eval endpoint)
  - /sub/...           (GrowthBook subscriptions)

反测信号 (G6 strict 后):
  - G6-2 漏删 client.init() → 启动时 ≥1 GB POST → strict fail
  - G2-1 wrapper 改向后误 fallback 到 GrowthBook → mock 收到 → fail
  - 兼容代码 USER_TYPE === 'internal' 守卫但 internal 代理走真 GB → internal case fail
"""

from __future__ import annotations

import json
import sys
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log
from lib.mock_http_capture import MockCaptureServer, alloc_port
from lib.mock_openai_provider import apply_mock_provider_env, mock_openai_provider


GB_PATH_PREFIXES = ("/api/features/", "/api/eval/", "/sub/")
WEAK_MODE = False  # G6-2 已删除远程 GrowthBook 客户端; STRICT mode 启用

# Mossen 个人版已 strip internal codepath; require './tools/REPLTool/REPLTool.js' 抛此错
ANT_STRIPPED_MARKER = "./tools/REPLTool/REPLTool.js"


def _is_gb_request(req: dict) -> bool:
    return any(req["path"].startswith(p) for p in GB_PATH_PREFIXES)


def _make_env(ctx, mock_port: int, *, user_type_internal: bool) -> dict:
    env = dict(ctx.env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"

    # 强制开 trust dialog → 让 GB 初始化路径真跑
    env["MOSSEN_CODE_TRUST_DIALOG_ACCEPTED"] = "1"
    env["MOSSEN_NON_INTERACTIVE_SESSION"] = "1"

    # D-G05-B: 只重定向 GB_BASE_URL, 不动 PLATFORM_BASE_URL
    mock_url = f"http://127.0.0.1:{mock_port}"
    env["MOSSEN_CODE_GB_BASE_URL"] = mock_url

    if user_type_internal:
        env["USER_TYPE"] = "internal"
    else:
        env.pop("USER_TYPE", None)

    return env


def _find_session_logs(home: Path) -> list[Path]:
    found = []
    for pattern in (
        "**/.mossen/transcripts/*.json",
        "**/transcripts/*.json",
        "**/projects/**/*.jsonl",
        "**/sessions/**/*.jsonl",
        "**/.mossen/**/*.jsonl",
    ):
        for p in home.glob(pattern):
            if p.is_file() and p not in found:
                found.append(p)
    return found


def case_no_gb_traffic(*, user_type_internal: bool) -> dict:
    label = "internal" if user_type_internal else "default"
    ctx = make_fixture(f"R7_no_gb_{label}")

    server = MockCaptureServer.start(port=alloc_port())
    try:
        env = _make_env(ctx, server.port, user_type_internal=user_type_internal)
        fake_proj = ctx.root_dir / "fake_project"
        fake_proj.mkdir(parents=True, exist_ok=True)

        prompt = "请把以下字符串原样回复给我: R7_GB_TEST_OK"

        model = f"r7-gb-{label}-model"
        with mock_openai_provider(model=model) as (base_url, provider):
            apply_mock_provider_env(
                env,
                base_url,
                model=model,
                name=f"R7 GB {label} Mock",
            )
            ctx.env.update(env)
            proc = subprocess.run(
                [str(ROOT / "scripts" / "start-mossen.sh"), "--stdin"],
                input=prompt,
                env=env,
                capture_output=True,
                text=True,
                timeout=180,
                cwd=str(fake_proj),
            )
            provider_snapshot = provider.snapshot()
        write_command_log(
            ctx,
            ["mossen", "--stdin", f"(mock_gb_port={server.port}, user_type={label})"],
            proc.stdout, proc.stderr, proc.returncode,
        )

        # 等 5s 给 GB periodic refresh queue flush 机会
        time.sleep(5)

        all_requests = server.received
    finally:
        server.stop()

    gb_requests = [r for r in all_requests if _is_gb_request(r)]
    other_requests = [r for r in all_requests if not _is_gb_request(r)]

    session_logs = _find_session_logs(ctx.home_dir)
    session_landed = len(session_logs) > 0

    # internal codepath stripped in Mossen 个人版 → require 抛 module-not-found, 而 GB
    # 请求一定 0 (proc 死在 init 前). 视为契约 trivially 满足.
    ant_stripped_ok = (
        user_type_internal
        and proc.returncode != 0
        and ANT_STRIPPED_MARKER in (proc.stderr or "")
        and len(gb_requests) == 0
    )

    # WEAK MODE (G2-2c): exit 0 + session 落盘 = pass; gb_requests 只记录
    # STRICT MODE (G6-2 后): 加 len(gb_requests) == 0
    base_ok = proc.returncode == 0 and session_landed
    if WEAK_MODE:
        ok = base_ok or ant_stripped_ok
    else:
        ok = (base_ok or ant_stripped_ok) and len(gb_requests) == 0

    return {
        "name": f"no_remote_gb_traffic_{label}",
        "ok": ok,
        "user_type": label,
        "exit_code": proc.returncode,
        "mock_port": server.port,
        "weak_mode": WEAK_MODE,
        "ant_stripped_path": ant_stripped_ok,
        "gb_request_count": len(gb_requests),
        "gb_request_excerpt": gb_requests[:5],
        "other_request_count": len(other_requests),
        "other_request_excerpt": other_requests[:3],
        "session_landed": session_landed,
        "session_log_count": len(session_logs),
        "provider_request_count": provider_snapshot["request_count"],
        "provider_paths": provider_snapshot["paths"],
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def _retry(case_fn, n=3, **kwargs):
    res = None
    for i in range(n):
        res = case_fn(**kwargs)
        if res.get("ok"):
            res["_attempt"] = i + 1
            return res
        res["_attempt"] = i + 1
    return res


def main() -> int:
    results = []

    res_default = _retry(case_no_gb_traffic, user_type_internal=False)
    ctx_d = res_default.pop("_ctx")
    write_assertions(
        ctx_d,
        status="passed" if res_default.get("ok") else "failed",
        assertions=[{
            "name": res_default["name"],
            "expected": True,
            "actual": res_default.get("ok"),
            "passed": res_default.get("ok"),
            "evidence": (
                f"exit={res_default.get('exit_code')} "
                f"gb_req={res_default.get('gb_request_count')} "
                f"session_landed={res_default.get('session_landed')} "
                f"weak_mode={res_default.get('weak_mode')} "
                f"provider_req={res_default.get('provider_request_count')}"
            ),
        }],
    )
    results.append(res_default)

    res_ant = _retry(case_no_gb_traffic, user_type_internal=True)
    ctx_a = res_ant.pop("_ctx")
    write_assertions(
        ctx_a,
        status="passed" if res_ant.get("ok") else "failed",
        assertions=[{
            "name": res_ant["name"],
            "expected": True,
            "actual": res_ant.get("ok"),
            "passed": res_ant.get("ok"),
            "evidence": (
                f"exit={res_ant.get('exit_code')} "
                f"gb_req={res_ant.get('gb_request_count')} "
                f"session_landed={res_ant.get('session_landed')} "
                f"weak_mode={res_ant.get('weak_mode')} "
                f"provider_req={res_ant.get('provider_request_count')}"
            ),
        }],
    )
    results.append(res_ant)

    overall_ok = all(r.get("ok") for r in results)
    print(json.dumps({
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "design_note": (
            "R7 (G2-2c weak mode): mock GB endpoint, 强制开 trust, 验 exit 0 + session 落盘. "
            "G6-2 后切 STRICT: gb_request_count 必须 == 0."
        ),
    }, indent=2, ensure_ascii=False, default=str))
    return 0 if overall_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
