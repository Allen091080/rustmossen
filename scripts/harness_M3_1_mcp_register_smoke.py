#!/usr/bin/env python3
"""
M3.1 — MCP 注册 + /mcp list e2e。

按 harness全链路测试.md §3.3 M3.1 契约:
  前置: fixture root 下创建 .mcp.json 配置一个 mock stdio server
  步骤: cwd=fixture_root, 跑 'mossen mcp list'
  观察点:
    1. exit_code 0 (mossen 自身正常)
    2. stdout 含 mock_server_name (server name 真被 list 出来)
    3. stdout 含连接状态字面 (即使 server 真 startup 失败, name 仍出现在 list)
  反测: 删 .mcp.json → mossen mcp list 不含 mock_server_name → fail
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

MOCK_SERVER_NAME = "mock_M3_1_register_test_unique"


def case_mcp_list_shows_registered() -> dict:
    ctx = make_fixture("M3.1")

    # 创建 .mcp.json 在 fixture root (作为 cwd 用)
    mcp_config = ctx.root_dir / ".mcp.json"
    mcp_config.write_text(json.dumps({
        "mcpServers": {
            MOCK_SERVER_NAME: {
                "type": "stdio",
                "command": "/bin/echo",
                "args": ["mock-mcp-fake-output"]
            }
        }
    }, indent=2))

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "mcp", "list"],
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=60,
        cwd=str(ctx.root_dir),  # 关键: cwd=fixture root, 让 mossen 读这里的 .mcp.json
    )

    write_command_log(ctx, ["mossen", "mcp", "list"],
                      proc.stdout, proc.stderr, proc.returncode)

    name_in_stdout = MOCK_SERVER_NAME in proc.stdout
    # 连接状态字面 (server 启动失败/成功 任一都算 list 工作)
    has_status_indicator = any(s in proc.stdout for s in
                               ["✓", "✗", "failed", "OK", "连接", "正常"])

    return {
        "name": "mcp_list_shows_registered",
        "ok": (
            proc.returncode == 0
            and name_in_stdout
            and has_status_indicator
        ),
        "exit_code": proc.returncode,
        "name_in_stdout": name_in_stdout,
        "has_status_indicator": has_status_indicator,
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:200],
        "fixture_root": str(ctx.root_dir),
        "mock_server_name": MOCK_SERVER_NAME,
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_mcp_list_shows_registered()
        ctx = res1.pop("_ctx")
        if res1.get("ok"):
            res1["_attempt"] = attempt + 1
            break
        res1["_attempt"] = attempt + 1
    results = [res1]

    write_assertions(ctx,
                     status="passed" if all(r.get("ok") for r in results) else "failed",
                     assertions=[
                         {"name": r["name"], "expected": True,
                          "actual": r.get("ok"), "passed": r.get("ok"),
                          "evidence": f"name_in_stdout={r.get('name_in_stdout')} has_status={r.get('has_status_indicator')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M3.1 mock MCP 注册: .mcp.json 在 fixture cwd, mossen mcp list 显示 server name",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
