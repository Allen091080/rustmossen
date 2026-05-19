#!/usr/bin/env python3
"""
M3.5 — MCP tool schema validation: 缺参/错参必须返回明确错误, 不假成功.

按 harness全链路测试.md §3.3 M3.5 契约:
  前置: fixture .mcp.json 指向 harness_mock_strict_mcp_server.py
        strict_tool_M3_5 工具 inputSchema 含 required:["text"], 且 server 主动验.
  步骤: cd fixture, 启动 mossen 让 model 调 strict_tool_M3_5, 故意不传 'text',
        只传无关参数 'foo':'bar'.
  观察点:
    1. exit_code == 0 (mossen 不 crash)
    2. session log 含 mcp__... strict_tool_M3_5 的 tool_use
    3. 对应的 tool_result 含 is_error 字面 (等价 isError: true) 或 content 含
       'MISSING_REQUIRED_text_M3_5' (mock server 主动返回的拒绝 marker)
    4. tool_result 不含 'STRICT_OK_M3_5' (没有静默成功)
  反测信号: harness_mock_strict_mcp_server.py:handle_tools_call 把 missing-text
            分支删, 让缺参也返 SUCCESS_MARKER → tool_result 是 success →
            no error marker → fail

  注: mossen 不在 client 端做 input schema validation (grep client.ts: 仅传
      inputSchema 给 model 作 inputJSONSchema, 不在 callTool 前 safeParse args).
      所以 strict 验由 mock server 端落实, 见 client.ts:3122 isError 分支.
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

MCP_SERVER_NAME = "harness_mock_strict_M3_5"
TOOL_NAME = "strict_tool_M3_5"
MCP_TOOL_FULL_NAME = f"mcp__{MCP_SERVER_NAME}__{TOOL_NAME}"
MISSING_MARKER = "MISSING_REQUIRED_text_M3_5"
SUCCESS_MARKER = "STRICT_OK_M3_5"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_strict_tool_missing_required_returns_error() -> dict:
    ctx = make_fixture("M3.5")

    mock_server_path = ROOT / "scripts" / "harness_mock_strict_mcp_server.py"

    mcp_config = ctx.root_dir / ".mcp.json"
    mcp_config.write_text(
        json.dumps(
            {
                "mcpServers": {
                    MCP_SERVER_NAME: {
                        "type": "stdio",
                        "command": "python3",
                        "args": [str(mock_server_path)],
                    },
                },
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    # Prompt 故意诱导 model 不传 'text' 参数
    prompt = (
        f"请用 {TOOL_NAME} 这个 MCP 工具调用一次, 但请故意 NOT 传 'text' 参数, "
        f"改为只传 {{\"foo\": \"bar\"}}. 调完后把工具返回的完整内容原样打印出来. "
        f"不要重试, 不要修正, 即便工具报错也直接打印错误."
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", MCP_TOOL_FULL_NAME],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ctx.root_dir),
    )

    write_command_log(
        ctx,
        ["mossen", "-p", "--allowedTools", MCP_TOOL_FULL_NAME],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    session_logs = _find_session_logs(ctx.home_dir)

    mcp_tool_use_found = False
    mcp_tool_use_ids = set()
    tool_result_has_error_flag = False
    tool_result_has_missing_marker = False
    tool_result_has_success_marker = False

    for log_file in session_logs:
        try:
            text = log_file.read_text()
        except OSError:
            continue
        for line in text.splitlines():
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = obj.get("message", obj)
            content = msg.get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict):
                    continue
                btype = block.get("type")
                if btype == "tool_use":
                    name = block.get("name", "") or ""
                    if name.startswith("mcp__") and TOOL_NAME in name:
                        mcp_tool_use_found = True
                        if block.get("id"):
                            mcp_tool_use_ids.add(block.get("id"))

    for log_file in session_logs:
        try:
            text = log_file.read_text()
        except OSError:
            continue
        for line in text.splitlines():
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = obj.get("message", obj)
            content = msg.get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict):
                    continue
                if block.get("type") != "tool_result":
                    continue
                tool_use_id = block.get("tool_use_id")
                if tool_use_id not in mcp_tool_use_ids:
                    continue
                # is_error: API 字段; content 字面 marker
                if block.get("is_error") is True:
                    tool_result_has_error_flag = True
                result_str = json.dumps(block.get("content") or "")
                if MISSING_MARKER in result_str:
                    tool_result_has_missing_marker = True
                if SUCCESS_MARKER in result_str:
                    tool_result_has_success_marker = True

    # 强契约: 缺 required 参数必须由 mock server 真返回 missing marker (确认
    # MCP server 真启动 + 真收到 call + 真验 schema), 不接受任何错误都算 (例如
    # "no such tool" 表示 server 没启动, 不是 schema validation).
    error_visible = tool_result_has_missing_marker
    no_silent_success = not tool_result_has_success_marker

    return {
        "name": "strict_tool_missing_required_returns_error",
        "ok": (
            proc.returncode == 0
            and mcp_tool_use_found
            and error_visible
            and no_silent_success
        ),
        "exit_code": proc.returncode,
        "mcp_tool_use_found": mcp_tool_use_found,
        "tool_result_has_error_flag": tool_result_has_error_flag,
        "tool_result_has_missing_marker": tool_result_has_missing_marker,
        "tool_result_has_success_marker": tool_result_has_success_marker,
        "error_visible": error_visible,
        "no_silent_success": no_silent_success,
        "stdout_excerpt": (proc.stdout or "")[:400],
        "stderr_excerpt": (proc.stderr or "")[:200],
        "session_log_count": len(session_logs),
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_strict_tool_missing_required_returns_error()
        ctx = res.pop("_ctx")
        if res.get("ok"):
            res["_attempt"] = attempt + 1
            break
        res["_attempt"] = attempt + 1
    results = [res]

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
                    f"tool_use={r.get('mcp_tool_use_found')} "
                    f"is_error={r.get('tool_result_has_error_flag')} "
                    f"missing_marker={r.get('tool_result_has_missing_marker')} "
                    f"no_silent_success={r.get('no_silent_success')}"
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
            "M3.5 schema validation: missing 'text' (required) on strict_tool_M3_5 "
            "must surface as is_error/MISSING marker in tool_result; "
            "STRICT_OK marker must NOT appear (no silent success)."
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
