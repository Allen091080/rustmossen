#!/usr/bin/env python3
"""
M4.3 — 手动 /compact 跨 --continue e2e (M4.1 auto-compact blocked 的替代方案)。

按 harness全链路测试.md §3.4 / 附录 E L7 契约:
  M4.1 (auto-compact) 因需打满上下文窗口不可行 → blocked。
  M4.3 直接验更直接的代理: 用户手动调 /compact, 然后 --continue 续会话,
  验 compaction 真发生 + 摘要写入 session log + 续会话不损坏。

  步骤:
    P1: mossen -p stdin "<任意 prompt>" → 建立 session, 累积一些 message
    P2: mossen -p --continue stdin "/compact" → 触发 compaction
    P3: mossen -p --continue stdin "<简单 prompt>" → 验 compact 后会话仍可用

  观察点 (强契约):
    1. P1 EXIT 0, session log 文件存在
    2. P2 EXIT 0, session log 增长 (compact 不该 crash)
    3. P3 EXIT 0, model 仍能响应
    4. P2 后 session log 含字面 '<command-name>/compact</command-name>'
       (slash 命令 dispatch 证据, src/utils/processUserInput/processSlashCommand.tsx)
    5. P2 后 session log 含字面 '"isCompactSummary":true'
       (compaction 真写出 summary 标记, src/utils/messages.ts:521)
    6. P2 后 session log NOT 含 '<local-command-stderr>Error: No messages to compact'
       (确认 P1 真留下了可压缩的 messages)

  反测信号 (mutation 抓力):
    - 改 src/services/compact/compact.ts 让 compactConversation 早 return / noop
      → session log 不再写 isCompactSummary → 观察点 5 fail
    - 改 src/commands/compact/compact.ts 让 type:'compact' 不返回
      → summary message 不进 conversation → isCompactSummary 缺失 → fail
    - 改 src/utils/messages.ts:521 拿掉 isCompactSummary 字段
      → 观察点 5 fail (字面消失)

  注意:
    - 三进程必须共享同一 fixture HOME + 同一 cwd, 否则 --continue 找不到 session
    - env 必须显式设 MOSSEN_CONFIG_DIR (子进程读这个, 不是 MOSSEN_CONFIG_HOME)
    - 工具用 --tools "" 禁用, 让 model 不绕路调工具
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


def _find_session_logs(home_dir: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in home_dir.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def _aggregate_log_text(logs: list[Path]) -> str:
    parts = []
    for log in logs:
        try:
            parts.append(log.read_text(encoding="utf-8", errors="replace"))
        except OSError:
            continue
    return "\n".join(parts)


def _aggregate_log_size(logs: list[Path]) -> int:
    total = 0
    for log in logs:
        try:
            total += log.stat().st_size
        except OSError:
            continue
    return total


def case_manual_compact_continue() -> dict:
    ctx = make_fixture("M4.3")

    # 子进程读 MOSSEN_CONFIG_DIR (不是 MOSSEN_CONFIG_HOME), 必须显式补
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # 三进程共享同一 cwd, 否则 --continue 按 cwd 找不到 session
    shared_cwd = ctx.root_dir
    shared_cwd.mkdir(parents=True, exist_ok=True)

    mossen = str(ROOT / "run-mossen.sh")

    # ---------- P1: 建会话, 留下足够的 messages 让 /compact 有东西可压 ----------
    prompt1 = (
        "请简短地用中文写两句话, 描述什么是单元测试 (不超过 60 字), "
        "不要使用任何工具, 直接回答即可。"
    )
    proc1 = subprocess.run(
        [mossen, "-p", "--tools", ""],
        input=prompt1,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(shared_cwd),
    )

    logs_after_p1 = _find_session_logs(ctx.home_dir)
    size_after_p1 = _aggregate_log_size(logs_after_p1)

    # ---------- P2: --continue + /compact 触发 compaction ----------
    proc2 = subprocess.run(
        [mossen, "-p", "--continue", "--tools", ""],
        input="/compact",
        env=env,
        capture_output=True,
        text=True,
        timeout=240,
        cwd=str(shared_cwd),
    )

    logs_after_p2 = _find_session_logs(ctx.home_dir)
    size_after_p2 = _aggregate_log_size(logs_after_p2)
    log_text_after_p2 = _aggregate_log_text(logs_after_p2)

    # ---------- P3: --continue 验会话仍可用 ----------
    prompt3 = "请回复一个汉字: 好"
    proc3 = subprocess.run(
        [mossen, "-p", "--continue", "--tools", ""],
        input=prompt3,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(shared_cwd),
    )

    logs_after_p3 = _find_session_logs(ctx.home_dir)
    size_after_p3 = _aggregate_log_size(logs_after_p3)

    write_command_log(
        ctx,
        ["mossen-3-process-compact-continue"],
        f"=== P1 stdout ===\n{proc1.stdout}\n"
        f"=== P2 stdout ===\n{proc2.stdout}\n"
        f"=== P3 stdout ===\n{proc3.stdout}\n",
        f"=== P1 stderr ===\n{proc1.stderr}\n"
        f"=== P2 stderr ===\n{proc2.stderr}\n"
        f"=== P3 stderr ===\n{proc3.stderr}\n",
        proc3.returncode,
    )

    # 强契约 marker
    has_slash_compact_dispatch = "<command-name>/compact</command-name>" in log_text_after_p2
    has_compact_summary = '"isCompactSummary":true' in log_text_after_p2
    has_no_messages_error = (
        "<local-command-stderr>Error: No messages to compact" in log_text_after_p2
    )

    # P3 是否真有响应 (非空 stdout)
    p3_has_response = bool(proc3.stdout.strip())

    ok = (
        proc1.returncode == 0
        and proc2.returncode == 0
        and proc3.returncode == 0
        and len(logs_after_p1) >= 1
        and size_after_p2 > size_after_p1  # P2 让 log 增长
        and has_slash_compact_dispatch
        and has_compact_summary
        and not has_no_messages_error
        and p3_has_response
    )

    return {
        "name": "manual_compact_continue",
        "ok": ok,
        "p1_exit": proc1.returncode,
        "p2_exit": proc2.returncode,
        "p3_exit": proc3.returncode,
        "p1_stdout_excerpt": proc1.stdout[:200],
        "p2_stdout_excerpt": proc2.stdout[:200],
        "p3_stdout_excerpt": proc3.stdout[:200],
        "log_count_p1": len(logs_after_p1),
        "log_count_p2": len(logs_after_p2),
        "log_count_p3": len(logs_after_p3),
        "log_size_p1": size_after_p1,
        "log_size_p2": size_after_p2,
        "log_size_p3": size_after_p3,
        "log_grew_after_p2": size_after_p2 > size_after_p1,
        "has_slash_compact_dispatch": has_slash_compact_dispatch,
        "has_compact_summary_marker": has_compact_summary,
        "no_messages_to_compact_error": has_no_messages_error,
        "p3_has_response": p3_has_response,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_manual_compact_continue()
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
                    f"exits=({r.get('p1_exit')},{r.get('p2_exit')},{r.get('p3_exit')}) "
                    f"slash_dispatch={r.get('has_slash_compact_dispatch')} "
                    f"compact_summary={r.get('has_compact_summary_marker')} "
                    f"no_messages_err={r.get('no_messages_to_compact_error')} "
                    f"log_grew={r.get('log_grew_after_p2')} "
                    f"p3_resp={r.get('p3_has_response')}"
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
            "M4.3 (M4.1 替代): 3 进程链路验手动 /compact 跨 --continue。"
            "强契约: session log 含 isCompactSummary marker (compactConversation "
            "真写 summary message), 且不含 'No messages to compact' error。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
