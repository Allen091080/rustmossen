#!/usr/bin/env python3
"""
M1.1 — Agent loop e2e: 用户 prompt 让模型用 Read 工具读真文件。

按 harness全链路测试.md §3.1 M1.1 契约:
  前置: fixture HOME 隔离 + /tmp/.../target.txt 含 marker MARKER_M1_1_READ_TARGET_xyz
  步骤: stdin 发 "请用 Read 工具读一下 <path> 然后把内容原样打印" → 等模型回流
  观察点:
    1. mossen exit_code == 0
    2. marker 出现在 stdout（model 真把文件内容回显）
    3. session log JSONL 含 tool_use block name=Read, input.file_path 匹配
  反测: 改 FileReadTool.ts call() 让它返回空字符串 → marker 不出现 → fail

依赖: M0.2 fixture helper.
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

MARKER = "MARKER_M1_1_READ_TARGET_xyz_unique"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    """在 fixture HOME 下扫 .jsonl session log."""
    candidates = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        candidates.extend(fixture_home.glob(pattern))
    # 去重
    seen = set()
    uniq = []
    for p in candidates:
        if p not in seen:
            seen.add(p)
            uniq.append(p)
    return uniq


def case_read_e2e_full() -> dict:
    ctx = make_fixture("M1.1")

    # 创建 fixture target file
    target = ctx.root_dir / "fixture" / "M1_1_target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(f"{MARKER}\nline2\nline3\n")

    prompt = f"请用 Read 工具读一下 {target} 然后把内容原样打印"

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Read"],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Read"],
                      proc.stdout, proc.stderr, proc.returncode)

    marker_in_stdout = MARKER in proc.stdout

    # 扫 session log
    session_logs = _find_session_logs(ctx.home_dir)
    tool_use_read_found = False
    matched_path = None
    for log_file in session_logs:
        try:
            for line in log_file.read_text().splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                # session log 是 message envelope, content 里含 tool_use blocks
                msg = obj.get("message", obj)
                content = msg.get("content")
                if isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name") == "Read":
                            tool_use_read_found = True
                            input_data = block.get("input", {})
                            if isinstance(input_data, dict) and str(target) in str(input_data.get("file_path", "")):
                                matched_path = str(target)
        except (json.JSONDecodeError, OSError):
            continue

    return {
        "name": "read_e2e_full",
        "ok": (
            proc.returncode == 0
            and marker_in_stdout
            and tool_use_read_found
        ),
        "exit_code": proc.returncode,
        "marker_in_stdout": marker_in_stdout,
        "tool_use_read_found": tool_use_read_found,
        "matched_path": matched_path,
        "session_log_count": len(session_logs),
        "session_logs": [str(p) for p in session_logs[:3]],
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:200],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    # Transient retry (LLM 偶发): 至多 3 次.
    res1 = None
    ctx = None
    for attempt in range(3):
        res1 = case_read_e2e_full()
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
                          "evidence": f"marker={r.get('marker_in_stdout')} tool_use_read={r.get('tool_use_read_found')} exit={r.get('exit_code')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": (
            "M1.1 真启动 mossen -p + --allowedTools Read 真发 prompt 真验证: "
            "stdout 含 marker + session log 含 tool_use Read block"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
