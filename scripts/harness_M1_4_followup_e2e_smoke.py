#!/usr/bin/env python3
"""
M1.4 — 多轮 follow-up e2e: 用户在两个 turn 跨上下文引用前一次结果。

按 harness全链路测试.md §3.1 M1.4 契约:
  前置: fixture target.txt 含 SECRET_NUMBER_M1_4=42
  步骤 1: 进程 1 启动 mossen -p --allowedTools Read, stdin "读 <path> 记住数字, 别说出来"
  步骤 2: 进程 2 启动 mossen -p -c (continue), stdin "刚才记住的数字 +100 等于多少"
  观察点:
    1. 进程 2 stdout 含 "142"
    2. 进程 2 session log 含至少 2 条 user 消息 (跨 turn)
    3. 进程 2 session log 含至少 1 条 Read tool_use (来自进程 1)
  反测: 改 query.ts/messages.ts 让 -c 不带回历史 messages → 模型看不到 42 → 输出不含 142

注意: 两个进程必须共享 fixture HOME (env.HOME 一致) + cwd 一致, 否则 -c 找不到 session。
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

SECRET_MARKER = "SECRET_NUMBER_M1_4"
SECRET_VALUE = 42
EXPECTED_RESULT = SECRET_VALUE + 100  # 142


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_followup_continue() -> dict:
    ctx = make_fixture("M1.4")

    target = ctx.root_dir / "fixture" / "M1_4_target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(f"{SECRET_MARKER}={SECRET_VALUE}\nother content\n")

    # Turn 1
    prompt1 = (
        f"请用 Read 工具读一下 {target} 文件, 记住里面的 "
        f"{SECRET_MARKER} 的数值, 但不要告诉我具体数字"
    )
    proc1 = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", "Read",
         "--add-dir", str(ctx.root_dir)],
        input=prompt1,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    # Turn 2 — 用 --continue 续上次会话
    # 加固抓力: prompt 不引用 SECRET_MARKER 字面 + 禁用所有工具
    # → 强制 model 只能从历史 messages 取数字, 不能重新读文件
    prompt2 = (
        "刚才记住的那个数字, 加上 100 等于多少? "
        "直接告诉我数字结果, 不要使用任何工具。"
    )
    proc2 = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p", "--continue", "--tools", ""],
        input=prompt2,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, ["mossen-multi-turn"],
                      f"=== TURN 1 ===\n{proc1.stdout}\n=== TURN 2 ===\n{proc2.stdout}",
                      f"{proc1.stderr}\n{proc2.stderr}",
                      proc2.returncode)

    result_in_stdout = str(EXPECTED_RESULT) in proc2.stdout

    session_logs = _find_session_logs(ctx.home_dir)

    user_message_count = 0
    read_tool_use_count = 0
    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                msg_type = obj.get("type", obj.get("message", {}).get("role"))
                if msg_type == "user":
                    user_message_count += 1
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name") == "Read":
                            read_tool_use_count += 1
        except (json.JSONDecodeError, OSError):
            continue

    return {
        "name": "followup_continue",
        "ok": (
            proc1.returncode == 0
            and proc2.returncode == 0
            and result_in_stdout
            and user_message_count >= 2  # 两轮 user message
            and read_tool_use_count >= 1  # 进程 1 的 Read 调用
        ),
        "turn1_exit": proc1.returncode,
        "turn2_exit": proc2.returncode,
        "turn1_stdout_excerpt": proc1.stdout[:200],
        "turn2_stdout_excerpt": proc2.stdout[:200],
        "result_142_in_stdout": result_in_stdout,
        "user_message_count": user_message_count,
        "read_tool_use_count": read_tool_use_count,
        "session_log_count": len(session_logs),
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_followup_continue()
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
                          "evidence": f"result_142={r.get('result_142_in_stdout')} user_msgs={r.get('user_message_count')} read_tool_use={r.get('read_tool_use_count')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M1.4 多轮 follow-up: 进程1 read+记忆, 进程2 -c 续会话 + 计算, 验跨 turn 上下文真传递",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
