#!/usr/bin/env python3
"""
M10.2 — Timeout 归因 e2e。

按 harness全链路测试.md §C.6 M10.2 P0 契约:
  当工具超时, mossen 必须把它显示为"timeout / 超时"事件 (附带服务器/工具
  名), 不能静默 idle 或当成成功。

源码事实 (src/services/mcp/client.ts:212-228, 3066-3087):
  - DEFAULT_MCP_TOOL_TIMEOUT_MS = 100_000_000 (≈27.8h)
  - 可由 MCP_TOOL_TIMEOUT 环境变量覆盖 (单位 ms)
  - 超时时抛: `MCP server "${name}" tool "${tool}" timed out after ${secs}s`
  - timeoutMs 也传给 SDK 的 client.callTool({signal, timeout: timeoutMs})

策略:
  - mock server: forever_M10_2 真 sleep 60s
  - 我们设 MCP_TOOL_TIMEOUT=4000 (4s) → tool 4s 后被 mossen 内部 timeout
  - .mcp.json 指向 mock; allowedTools mcp__...forever_M10_2
  - mossen subprocess 主超时 = 90s (远大于 tool timeout, 让 mossen 自己处理 + 回流)
  - 验:
    1. mossen 不能跑满 60s 才结束 — 总耗时应远小于 60s (= 内部 timeout 真触发)
    2. session log 含 forever_M10_2 tool_use (model 真发了)
    3. session log 中, 该 tool_use 对应的 tool_result.is_error 是 True
       OR result content 含 "timed out" / "timeout" / "超时" 字面
       (= timeout 被归因, 不是 silent success)
    4. exit_code == 0 (mossen 自己应当 graceful 处理 tool timeout, 不崩)

强契约: tool timeout 在 session log 里有 timeout 字面, 不静默 idle。

反测信号: 如果有人改 src/services/mcp/client.ts 把 timeout 异常 swallow
(catch 后 return 空 success 而不是 throw / is_error), 那么 tool_result 既
不 is_error 也不含 timeout 字面 → 此 case fail。
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

MCP_SERVER_NAME = "harness_mock_slow_M10_2"
MCP_TOOL_FULL_NAME = f"mcp__{MCP_SERVER_NAME}__forever_M10_2"
FOREVER_SLEEP_SECS = 60          # mock server 真睡这么久
TOOL_TIMEOUT_MS = 4_000          # 但我们让 mossen 4s 就触发 timeout
TIMEOUT_KEYWORDS = ["timed out", "timeout", "超时", "time out", "timed-out"]


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_timeout_is_attributed() -> dict:
    ctx = make_fixture("M10.2")

    mock_server_path = ROOT / "scripts" / "harness_mock_slow_mcp_server.py"

    mcp_config = ctx.root_dir / ".mcp.json"
    mcp_config.write_text(json.dumps({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": "python3",
                "args": [str(mock_server_path)],
                "env": {
                    "HARNESS_FOREVER_SLEEP_SECS": str(FOREVER_SLEEP_SECS),
                },
            }
        }
    }, indent=2))

    env = dict(ctx.env)
    env["HARNESS_FOREVER_SLEEP_SECS"] = str(FOREVER_SLEEP_SECS)
    env["MCP_TOOL_TIMEOUT"] = str(TOOL_TIMEOUT_MS)  # 关键: 4s timeout

    prompt = (
        "请用 forever_M10_2 这个 MCP 工具, 参数 note=timeout_test。"
        "调用一次, 然后告诉我工具返回了什么 (包括任何错误/超时信息)。"
    )

    t_start = time.monotonic()
    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", MCP_TOOL_FULL_NAME],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,  # 远大于 tool timeout, mossen 应当自己 graceful 处理
        cwd=str(ctx.root_dir),
    )
    duration = time.monotonic() - t_start

    write_command_log(
        ctx,
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", MCP_TOOL_FULL_NAME],
        proc.stdout, proc.stderr, proc.returncode,
    )

    # mossen 不能跑满 60s — tool timeout 应当在 4s 触发, 留余量给 LLM 回流
    duration_under_full_sleep = duration < (FOREVER_SLEEP_SECS - 5)

    # session log: tool_use forever_M10_2 + tool_result is_error 或含 timeout 字面
    session_logs = _find_session_logs(ctx.home_dir)
    forever_tool_use_ids: set[str] = set()
    forever_tool_results: list[dict] = []
    timeout_attributed = False  # is_error=True 或 content 含 timeout 字面

    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg = obj.get("message", obj)
                content = msg.get("content")
                if not isinstance(content, list):
                    continue
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_use":
                        name = block.get("name", "")
                        if name.startswith("mcp__") and "forever_M10_2" in name:
                            tid = block.get("id")
                            if tid:
                                forever_tool_use_ids.add(tid)
                    elif block.get("type") == "tool_result":
                        tid = block.get("tool_use_id")
                        if tid in forever_tool_use_ids:
                            result_str = str(block.get("content", ""))
                            is_err = block.get("is_error") is True
                            has_timeout_kw = any(
                                kw.lower() in result_str.lower()
                                for kw in TIMEOUT_KEYWORDS
                            )
                            forever_tool_results.append({
                                "tool_use_id": tid,
                                "is_error": is_err,
                                "has_timeout_keyword": has_timeout_kw,
                                "content_excerpt": result_str[:300],
                            })
                            if is_err or has_timeout_kw:
                                timeout_attributed = True
        except (json.JSONDecodeError, OSError):
            continue

    forever_tool_use_found = len(forever_tool_use_ids) > 0

    # stdout / stderr 有 timeout 字面 (辅助证据)
    combined_out = (proc.stdout + "\n" + proc.stderr).lower()
    timeout_in_output = any(kw.lower() in combined_out for kw in TIMEOUT_KEYWORDS)

    ok = (
        proc.returncode == 0
        and duration_under_full_sleep
        and forever_tool_use_found
        and timeout_attributed  # 强契约
    )

    return {
        "name": "timeout_is_attributed",
        "ok": ok,
        "exit_code": proc.returncode,
        "duration_secs": round(duration, 2),
        "duration_under_full_sleep": duration_under_full_sleep,
        "forever_tool_use_found": forever_tool_use_found,
        "forever_tool_use_count": len(forever_tool_use_ids),
        "tool_results_count": len(forever_tool_results),
        "timeout_attributed_in_session": timeout_attributed,
        "timeout_keyword_in_stdout_or_stderr": timeout_in_output,
        "tool_result_excerpts": forever_tool_results[:3],
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_timeout_is_attributed()
        ctx = res1.pop("_ctx")
        if res1.get("ok"):
            res1["_attempt"] = attempt + 1
            break
        res1["_attempt"] = attempt + 1
    results = [res1]

    write_assertions(
        ctx,
        status="passed" if all(r.get("ok") for r in results) else "failed",
        assertions=[
            {"name": r["name"], "expected": True,
             "actual": r.get("ok"), "passed": r.get("ok"),
             "evidence": (
                 f"duration={r.get('duration_secs')}s "
                 f"under_full_sleep={r.get('duration_under_full_sleep')} "
                 f"tool_use={r.get('forever_tool_use_found')} "
                 f"timeout_attributed={r.get('timeout_attributed_in_session')}"
             )}
            for r in results
        ],
    )

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            f"M10.2 timeout 归因: mock server 睡 {FOREVER_SLEEP_SECS}s, "
            f"MCP_TOOL_TIMEOUT={TOOL_TIMEOUT_MS}ms 触发 mossen 内部 timeout, "
            "session log 应有 timeout 字面 / is_error=True, 不静默 idle。"
        ),
        "antitest_signal": (
            "如果有人改 client.ts 把 timeout 异常 swallow, tool_result 既不 "
            "is_error 也不含 timeout 字面 → 此 case fail。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
