#!/usr/bin/env python3
"""
M3.2 — MCP tool 调用真执行 e2e。

按 harness全链路测试.md §3.3 M3.2 契约:
  前置: fixture .mcp.json 指向 harness_mock_mcp_server.py
  步骤: cd fixture, 启动 mossen 让 model 调用 echo_M3_2 工具发送 marker
  观察点:
    1. exit_code == 0
    2. mossen final stdout 含 ECHO_TAG_FROM_MOCK_MCP (mock server 真返回的)
    3. stdout 含我们 payload marker (model 真传了正确参数)
    4. session log 含 mcp__ tool_use (确认 model 调用了 mcp 工具)
  反测: 把 mock server command 改成 /bin/false → tool call 失败 → fail
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

MCP_SERVER_NAME = "harness_mock_M3_2"
MCP_TOOL_FULL_NAME = f"mcp__{MCP_SERVER_NAME}__echo_M3_2"
ECHO_TAG = "ECHO_TAG_FROM_MOCK_MCP"
PAYLOAD = "M3_2_PAYLOAD_unique_xyz"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_mcp_tool_call_real() -> dict:
    ctx = make_fixture("M3.2")

    mock_server_path = ROOT / "scripts" / "harness_mock_mcp_server.py"

    mcp_config = ctx.root_dir / ".mcp.json"
    mcp_config.write_text(json.dumps({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": "python3",
                "args": [str(mock_server_path)]
            }
        }
    }, indent=2))

    prompt = (
        f"请用 echo_M3_2 这个 MCP 工具, 发送 text={PAYLOAD}, "
        f"然后把工具返回的完整内容原样打印出来"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", MCP_TOOL_FULL_NAME],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ctx.root_dir),
    )

    write_command_log(ctx, ["mossen", "-p", "--allowedTools", MCP_TOOL_FULL_NAME],
                      proc.stdout, proc.stderr, proc.returncode)

    echo_tag_in_stdout = ECHO_TAG in proc.stdout
    payload_in_stdout = PAYLOAD in proc.stdout

    # session log 验: mcp tool_use 真被调 + tool_result 含 ECHO_TAG
    session_logs = _find_session_logs(ctx.home_dir)
    mcp_tool_use_found = False
    mcp_tool_result_has_tag = False
    mcp_tool_use_ids = set()

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "tool_use":
                            name = block.get("name", "")
                            if name.startswith("mcp__") and "echo_M3_2" in name:
                                mcp_tool_use_found = True
                                mcp_tool_use_ids.add(block.get("id"))
        except (json.JSONDecodeError, OSError):
            continue

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "tool_result":
                            tool_use_id = block.get("tool_use_id")
                            if tool_use_id in mcp_tool_use_ids:
                                result_str = str(block.get("content", ""))
                                if ECHO_TAG in result_str:
                                    mcp_tool_result_has_tag = True
        except (json.JSONDecodeError, OSError):
            continue

    return {
        "name": "mcp_tool_call_real",
        "ok": (
            proc.returncode == 0
            and echo_tag_in_stdout
            and payload_in_stdout
            and mcp_tool_use_found
            and mcp_tool_result_has_tag  # 强契约: 真 mock server 返回的
        ),
        "exit_code": proc.returncode,
        "echo_tag_in_stdout": echo_tag_in_stdout,
        "payload_in_stdout": payload_in_stdout,
        "mcp_tool_use_found": mcp_tool_use_found,
        "mcp_tool_result_has_tag": mcp_tool_result_has_tag,
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_mcp_tool_call_real()
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
                          "evidence": f"echo_tag={r.get('echo_tag_in_stdout')} payload={r.get('payload_in_stdout')} mcp_tool_use={r.get('mcp_tool_use_found')} result_has_tag={r.get('mcp_tool_result_has_tag')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M3.2 MCP tool 真调用: mock stdio server + mossen --allowedTools mcp__... → 真 echo back",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
