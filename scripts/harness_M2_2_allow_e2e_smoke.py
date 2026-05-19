#!/usr/bin/env python3
"""
M2.2 — 权限安全 e2e: allow 后工具真执行 (M2.1 的反向场景)。

按 harness全链路测试.md §3.2 M2.2 契约:
  前置: fixture 内目录, target 文件不存在
  步骤: stdin 让 model 用 Write 工具创建 target.txt 内容 ALLOWED_M2_2_xyz
        --allowedTools Write + --add-dir <fixture> 显式允许
  观察点:
    1. exit_code == 0
    2. target.txt 必须被创建 (核心断言)
    3. 文件内容含 marker
    4. session log 含 tool_use Write
  反测: 收回 --allowedTools → Write 不执行 / 被拦截 → 文件不存在
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

MARKER = "ALLOWED_M2_2_xyz_unique_marker"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_allow_then_execute() -> dict:
    ctx = make_fixture("M2.2")
    target = ctx.root_dir / "fixture" / "M2_2_target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        target.unlink()

    prompt = (
        f"请用 Write 工具创建文件 {target}, 内容只有一行: {MARKER}"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", "Write",
         "--add-dir", str(ctx.root_dir)],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, ["mossen", "-p", "--allowedTools", "Write"],
                      proc.stdout, proc.stderr, proc.returncode)

    file_exists = target.exists()
    file_has_marker = (
        MARKER in target.read_text() if file_exists else False
    )

    session_logs = _find_session_logs(ctx.home_dir)
    write_tool_use_found = False
    write_input_has_marker = False
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
                        if not isinstance(block, dict):
                            continue
                        if block.get("type") == "tool_use" and block.get("name") == "Write":
                            write_tool_use_found = True
                            input_data = block.get("input", {})
                            if MARKER in str(input_data.get("content", "")):
                                write_input_has_marker = True
        except (json.JSONDecodeError, OSError):
            continue

    return {
        "name": "allow_then_execute",
        "ok": (
            proc.returncode == 0
            and file_exists  # 关键
            and file_has_marker  # 关键
            and write_tool_use_found
            and write_input_has_marker
        ),
        "exit_code": proc.returncode,
        "file_exists": file_exists,
        "file_has_marker": file_has_marker,
        "write_tool_use_found": write_tool_use_found,
        "write_input_has_marker": write_input_has_marker,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_allow_then_execute()
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
                          "evidence": f"file_exists={r.get('file_exists')} write_tool_use={r.get('write_tool_use_found')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M2.2 allow 真执行: --allowedTools Write 后 model 真创建文件",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
