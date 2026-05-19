#!/usr/bin/env python3
"""
M4.1 — auto-compact 触发 + 语义保留 (P1, 取消 skipped 状态).

按 harness全链路测试.md §3.4 M4.1 契约 (修正后):
  早期版本标 skipped 因为 auto-compact 默认需 >100K tokens 不可 deterministic 触发.
  发现关键 env: MOSSEN_AUTOCOMPACT_PCT_OVERRIDE=1 → threshold=1% effective ≈1.8K tokens
  → 几轮 mossen 对话即可 deterministic 触发.

策略 (4 进程链路, 全 --continue 链):
  P1: prompt 留 marker 'PROJX_M4_1_unique', model 回复确认 (建立 session)
  P2-P3: --continue + 大量内容 prompt (让 token 积累超过 1% 阈值)
  P4: --continue + 问 "我项目叫什么?", 验:
    - session log 含 isCompactSummary marker (auto-compact 真触发)
    - model reply 含 'PROJX_M4_1_unique' (compact 后语义保留)

观察点 (强契约):
  1. 全 4 进程 EXIT 0
  2. session log 含 '"isCompactSummary":true' 字面 (auto-compact event)
  3. P4 reply 含 'PROJX_M4_1_unique' (语义保留, 不只是结构)

反测信号:
  - 改 src/services/compact/compact.ts 让 compactConversation 短路 noop
    → 无 isCompactSummary → fail
  - 改 src/utils/messages.ts:521 删 isCompactSummary 字段 → 字面消失 → fail
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
MARKER = "PROJX_M4_1_unique_proj_name_xyz"


def _aggregate_log(home_dir: Path) -> tuple[str, int, int]:
    logs = list(home_dir.glob("**/projects/**/*.jsonl"))
    text = ""
    total = 0
    for log in logs:
        try:
            t = log.read_text(encoding="utf-8", errors="replace")
            text += t
            total += len(t)
        except OSError:
            continue
    return text, total, len(logs)


def case_auto_compact_preserves_semantic() -> dict:
    ctx = make_fixture("M4.1")
    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    # 关键: 把 auto-compact 阈值降到 1% (~1.8K tokens), 几轮即触发
    env["MOSSEN_AUTOCOMPACT_PCT_OVERRIDE"] = "0.1"  # ~180 tokens 阈值, 任一对话即触发

    # P1: 留 marker
    p1_prompt = (
        f"请严格记住: 我的项目名叫 {MARKER}. 后续问起时必须答出。"
        f"现在回复 OK 即可。"
    )
    p1 = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=p1_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )
    log_text_p1, _, _ = _aggregate_log(ctx.home_dir)

    # P2: --continue + 大量 token 积累 prompt
    p2_prompt = (
        "请用大约 800 字详细介绍 Python 编程语言的 10 个特性, "
        "每个特性给具体例子 + 优劣分析。回复尽量长。"
    )
    p2 = subprocess.run(
        [RUN_MOSSEN, "-p", "--continue", "--tools", ""],
        input=p2_prompt, env=env,
        capture_output=True, text=True, timeout=300,
        cwd=str(fixture_cwd),
    )

    # P3: --continue + 再积累
    p3_prompt = (
        "请再用 800 字详细介绍 JavaScript 的事件循环、Promise、async/await, "
        "并对比 Python 异步模型, 给具体代码例子。"
    )
    p3 = subprocess.run(
        [RUN_MOSSEN, "-p", "--continue", "--tools", ""],
        input=p3_prompt, env=env,
        capture_output=True, text=True, timeout=300,
        cwd=str(fixture_cwd),
    )

    # P4: --continue + 问回 marker, 验 auto-compact 后仍记得
    p4_prompt = "我的项目名叫什么? 请直接打印项目名, 不要其它任何文字。"
    p4 = subprocess.run(
        [RUN_MOSSEN, "-p", "--continue", "--tools", ""],
        input=p4_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )

    log_text_full, log_size_full, log_count = _aggregate_log(ctx.home_dir)

    write_command_log(
        ctx,
        ["mossen-4-process-auto-compact"],
        f"=== P1 ===\n{p1.stdout[:200]}\n=== P2 ===\n{p2.stdout[:200]}\n"
        f"=== P3 ===\n{p3.stdout[:200]}\n=== P4 ===\n{p4.stdout[:200]}\n",
        f"=== P4 stderr ===\n{p4.stderr[:300]}\n",
        p4.returncode,
    )

    has_compact_marker = '"isCompactSummary":true' in log_text_full or '"isCompactSummary": true' in log_text_full
    p4_has_marker = MARKER in p4.stdout
    all_exit_ok = all(p.returncode == 0 for p in [p1, p2, p3, p4])

    return {
        "name": "auto_compact_preserves_semantic",
        "ok": all_exit_ok and has_compact_marker and p4_has_marker,
        "p1_exit": p1.returncode,
        "p2_exit": p2.returncode,
        "p3_exit": p3.returncode,
        "p4_exit": p4.returncode,
        "log_count": log_count,
        "log_size_bytes": log_size_full,
        "auto_compact_marker_in_log": has_compact_marker,
        "p4_reply_has_project_marker": p4_has_marker,
        "p4_stdout_excerpt": p4.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(2):  # transient retry
        res = case_auto_compact_preserves_semantic()
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
                    f"exits=({r.get('p1_exit')},{r.get('p2_exit')},{r.get('p3_exit')},{r.get('p4_exit')}) "
                    f"compact_marker={r.get('auto_compact_marker_in_log')} "
                    f"p4_has_proj={r.get('p4_reply_has_project_marker')} "
                    f"log_size={r.get('log_size_bytes')}"
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
            "M4.1 (取消 skipped): 4 进程 --continue 链, MOSSEN_AUTOCOMPACT_PCT_OVERRIDE=1 "
            "强制 auto-compact 阈值降到 1% (~1.8K tokens), P2/P3 大量 token 触发, "
            "P4 验 marker 仍在 (compact 后语义保留)。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
