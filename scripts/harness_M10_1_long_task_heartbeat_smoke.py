#!/usr/bin/env python3
"""
M10.1 — 长任务 heartbeat / 真完成 e2e。

按 harness全链路测试.md §C.6 M10.1 P0 契约:
  长 (理论 30min) 工具调用必须真完成, 不被无声 abort, mossen 进程不退、
  进度可见。这里用 mock MCP server 真 sleep 10s 替代 30min, 验"长任务能
  真完成"这条最小但强契约。

源码事实 (src/services/mcp/client.ts:212-228):
  DEFAULT_MCP_TOOL_TIMEOUT_MS = 100_000_000  (≈27.8 小时)
  → 默认 mossen 不会 5s 就 timeout, 10s sleep 安全。
  内部还有"Log every 30 seconds"的 progress logger (3060 行附近)。

策略:
  - mock server: harness_mock_slow_mcp_server.py, slow_M10_1 真 sleep 10s
  - .mcp.json 指向 mock
  - prompt 让 model 调 slow_M10_1
  - 验:
    1. exit_code == 0
    2. 总耗时 ≥ 9.5s (mock server 真 sleep 10s, model 真等到 + 回流)
    3. session log 含 mcp__...slow_M10_1 tool_use + tool_result
    4. tool_result 含 SLOW_TAG_FROM_MOCK_M10_1 (= mock server 真返了, 没被 abort)

强契约: long-running tool 不被 mossen 主进程 abort, 真完成。

反测信号: 如果有人改 src/services/mcp/client.ts 把 timeoutMs 强写成 5000ms,
slow_M10_1 在 5s 后被 abort → tool_result 是 timeout error → SLOW_TAG 不出现
→ 此 case fail。
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

MCP_SERVER_NAME = "harness_mock_slow_M10_1"
MCP_TOOL_FULL_NAME = f"mcp__{MCP_SERVER_NAME}__slow_M10_1"
SLOW_TAG = "SLOW_TAG_FROM_MOCK_M10_1"
SLOW_SLEEP_SECS = 10
MIN_EXPECTED_DURATION = 9.5  # 给一点点 jitter


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_long_task_completes() -> dict:
    ctx = make_fixture("M10.1")

    mock_server_path = ROOT / "scripts" / "harness_mock_slow_mcp_server.py"

    mcp_config = ctx.root_dir / ".mcp.json"
    mcp_config.write_text(json.dumps({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": "python3",
                "args": [str(mock_server_path)],
                "env": {
                    # 让 mock server 知道睡多久
                    "HARNESS_SLOW_SLEEP_SECS": str(SLOW_SLEEP_SECS),
                },
            }
        }
    }, indent=2))

    # 把 env 也设上 (subprocess 启动时 mock server 进程会继承部分)
    env = dict(ctx.env)
    env["HARNESS_SLOW_SLEEP_SECS"] = str(SLOW_SLEEP_SECS)

    prompt = (
        f"请用 slow_M10_1 这个 MCP 工具, 参数 note=heartbeat_test, "
        f"等它返回后把工具返回内容原样打印出来。"
        f"该工具会真睡 {SLOW_SLEEP_SECS} 秒后才返, 请耐心等待, 不要中途取消。"
    )

    t_start = time.monotonic()
    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", MCP_TOOL_FULL_NAME],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=360,
        cwd=str(ctx.root_dir),
    )
    duration = time.monotonic() - t_start

    write_command_log(
        ctx,
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", MCP_TOOL_FULL_NAME],
        proc.stdout, proc.stderr, proc.returncode,
    )

    duration_ok = duration >= MIN_EXPECTED_DURATION
    slow_tag_in_stdout = SLOW_TAG in proc.stdout

    # session log 验: tool_use + tool_result + tool_result 含 SLOW_TAG
    session_logs = _find_session_logs(ctx.home_dir)
    slow_tool_use_ids: set[str] = set()
    slow_tool_result_has_tag = False
    slow_tool_result_was_error = False

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
                        if name.startswith("mcp__") and "slow_M10_1" in name:
                            tid = block.get("id")
                            if tid:
                                slow_tool_use_ids.add(tid)
                    elif block.get("type") == "tool_result":
                        tid = block.get("tool_use_id")
                        if tid in slow_tool_use_ids:
                            result_str = str(block.get("content", ""))
                            if SLOW_TAG in result_str:
                                slow_tool_result_has_tag = True
                            if block.get("is_error") is True:
                                slow_tool_result_was_error = True
        except (json.JSONDecodeError, OSError):
            continue

    slow_tool_use_found = len(slow_tool_use_ids) > 0

    ok = (
        proc.returncode == 0
        and duration_ok
        and slow_tool_use_found
        and slow_tool_result_has_tag
        and not slow_tool_result_was_error
    )

    return {
        "name": "long_task_completes",
        "ok": ok,
        "exit_code": proc.returncode,
        "duration_secs": round(duration, 2),
        "duration_ok": duration_ok,
        "slow_tag_in_stdout": slow_tag_in_stdout,
        "slow_tool_use_found": slow_tool_use_found,
        "slow_tool_use_count": len(slow_tool_use_ids),
        "slow_tool_result_has_tag": slow_tool_result_has_tag,
        "slow_tool_result_was_error": slow_tool_result_was_error,
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_long_task_completes()
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
                 f"duration_ok={r.get('duration_ok')} "
                 f"slow_use={r.get('slow_tool_use_found')} "
                 f"result_tag={r.get('slow_tool_result_has_tag')} "
                 f"result_err={r.get('slow_tool_result_was_error')}"
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
            f"M10.1 长任务真完成: mock MCP server 真 sleep {SLOW_SLEEP_SECS}s, "
            "mossen 必须等到 + 收 tool_result + 含 SLOW_TAG, 不被 abort。"
        ),
        "antitest_signal": (
            "如果有人改 src/services/mcp/client.ts 让 timeoutMs 强写成 5000, "
            "slow_M10_1 会在 5s 被 abort → result_tag=False / result_err=True → fail。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
