#!/usr/bin/env python3
"""
M10.3 — 嵌套子任务 / Agent 工具完成后主任务状态正确 e2e。

按 harness全链路测试.md §C.6 M10.3 P1 契约:
  通过 Agent 工具 spawn 子任务, 子任务完成后, 父对话状态正常 (能拿到子结果
  并继续输出最终回复)。

源码事实:
  - src/tools/AgentTool/constants.ts: AGENT_TOOL_NAME = 'Agent'
    (legacy 别名 'Task' 仍存在)
  - src/tools/AgentTool/AgentTool.tsx: Agent 工具会 runAgent({...}) 起子代理,
    finalizeAgentTool 把子结果回流为 tool_result block

策略:
  - prompt 让 model 用 Agent 工具跑一个简单子任务 (例如让子代理回复一个
    UUID-like marker), 然后父任务必须在收到子结果后回复 PARENT_OK_M10_3
  - --allowedTools 包含 Agent (并加 Read 让子代理有点能力, 不卡死)
  - 验:
    1. exit_code == 0
    2. session log 含 Agent tool_use (model 真发了 Agent 调用)
    3. session log 含对应 tool_result, is_error 不为 True (子任务没 error 上浮)
    4. 父任务最终 stdout 含 'PARENT_OK_M10_3' (= 父在收到子结果后真继续)

强契约: 子任务结束后, 父任务状态正确, 能拿到子结果继续 (不卡死、不崩)。

反测信号: 如果有人改 src/tools/AgentTool/AgentTool.tsx 的 finalizeAgentTool
让它在子结果回流时 throw, 父任务收不到 result → PARENT_OK_M10_3 不出现 → fail。
或父任务直接被 sub-error 终止 → exit != 0 / PARENT_OK 不出现 → fail。

注意: 大模型对长 prompt 不 100% 听话, 我们把契约写得宽容 (Agent tool_use
真发了 + 父最终 reply 含 marker), 不强求子代理本身 reply 的特定格式。
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

AGENT_TOOL_NAME = "Agent"
PARENT_MARKER = "PARENT_OK_M10_3"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_nested_subtask_completes() -> dict:
    ctx = make_fixture("M10.3")

    # 给子代理一点上下文物料, 否则它没事可做容易拒绝
    work_dir = ctx.root_dir / "work"
    work_dir.mkdir(parents=True, exist_ok=True)
    (work_dir / "M10_3_input.txt").write_text(
        "child task input\nline2 token=child_marker_M10_3\n"
    )

    prompt = (
        "请用 Agent 工具 spawn 一个子任务, 让子代理完成下面这件小事: "
        f"读取 {work_dir / 'M10_3_input.txt'} 文件并把第二行返回给你。"
        " 你必须在子任务真正完成、并把结果交回给你之后, 再用一句话回复我, "
        f"且最终回复必须包含字面 '{PARENT_MARKER}' (用于自动化验证)。"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", f"{AGENT_TOOL_NAME},Read"],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=360,
        cwd=str(ROOT),
    )

    write_command_log(
        ctx,
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", f"{AGENT_TOOL_NAME},Read"],
        proc.stdout, proc.stderr, proc.returncode,
    )

    parent_marker_in_stdout = PARENT_MARKER in proc.stdout

    # 扫 session log 找 Agent tool_use + 对应 tool_result
    session_logs = _find_session_logs(ctx.home_dir)
    agent_tool_use_ids: set[str] = set()
    agent_tool_result_count = 0
    agent_tool_result_error_count = 0

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
                        # legacy alias 'Task' 也算
                        if block.get("name") in (AGENT_TOOL_NAME, "Task"):
                            tid = block.get("id")
                            if tid:
                                agent_tool_use_ids.add(tid)
                    elif block.get("type") == "tool_result":
                        tid = block.get("tool_use_id")
                        if tid in agent_tool_use_ids:
                            agent_tool_result_count += 1
                            if block.get("is_error") is True:
                                agent_tool_result_error_count += 1
        except (json.JSONDecodeError, OSError):
            continue

    agent_tool_use_found = len(agent_tool_use_ids) > 0
    every_agent_call_got_non_error_result = (
        agent_tool_use_found
        and agent_tool_result_count >= len(agent_tool_use_ids)
        and agent_tool_result_error_count == 0
    )

    ok = (
        proc.returncode == 0
        and agent_tool_use_found  # 父真用了 Agent
        and every_agent_call_got_non_error_result  # 子结果回流且非错
        and parent_marker_in_stdout  # 父真继续到最终回复
    )

    return {
        "name": "nested_subtask_completes",
        "ok": ok,
        "exit_code": proc.returncode,
        "agent_tool_use_found": agent_tool_use_found,
        "agent_tool_use_count": len(agent_tool_use_ids),
        "agent_tool_result_count": agent_tool_result_count,
        "agent_tool_result_error_count": agent_tool_result_error_count,
        "every_agent_call_got_non_error_result": every_agent_call_got_non_error_result,
        "parent_marker_in_stdout": parent_marker_in_stdout,
        "stdout_excerpt": proc.stdout[:500],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_nested_subtask_completes()
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
                 f"agent_use={r.get('agent_tool_use_found')} "
                 f"agent_result={r.get('agent_tool_result_count')} "
                 f"agent_err={r.get('agent_tool_result_error_count')} "
                 f"parent_marker={r.get('parent_marker_in_stdout')}"
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
            "M10.3 子任务/Agent 完成后父状态正确: 父调 Agent 工具 → 子代理跑完 "
            "→ tool_result 非错 → 父继续输出含 PARENT_OK_M10_3。"
        ),
        "antitest_signal": (
            "如果有人改 finalizeAgentTool 让子结果不回流或主任务被 sub-error "
            "终止, parent_marker_in_stdout=False → fail。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
