#!/usr/bin/env python3
"""
M1.3 — Agent loop e2e: 用户 prompt 让模型用 Edit 工具改 fixture 文件。

按 harness全链路测试.md §3.1 M1.3 契约:
  前置: fixture target.txt 含 OLD_LINE_M1_3_unique
  步骤: stdin 发 "用 Edit 工具把 <path> 中的 OLD_LINE_M1_3_unique 替换为 NEW_LINE_M1_3_unique"
  观察点:
    1. exit_code == 0
    2. 文件内容真被改 (NEW_LINE_M1_3_unique present, OLD_LINE_M1_3_unique gone)
    3. session log 含 tool_use Edit + input 含 old/new strings
    4. (强契约) session log 的 tool_result 表示 success
  反测: 改 FileEditTool.call() 让它 no-op → 文件未改 → fail
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

OLD_MARKER = "OLD_LINE_M1_3_unique_xyz"
NEW_MARKER = "NEW_LINE_M1_3_unique_xyz"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_edit_e2e_full() -> dict:
    ctx = make_fixture("M1.3")

    target = ctx.root_dir / "fixture" / "M1_3_target.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(f"line1\n{OLD_MARKER}\nline3\n")

    prompt = (
        f"请用 Edit 工具把 {target} 中的 {OLD_MARKER} "
        f"替换为 {NEW_MARKER}, 不要做其他修改"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--allowedTools", "Edit",
         "--add-dir", str(ctx.root_dir)],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Edit"],
                      proc.stdout, proc.stderr, proc.returncode)

    final_content = target.read_text() if target.exists() else ""
    file_has_new = NEW_MARKER in final_content
    file_has_old = OLD_MARKER in final_content

    session_logs = _find_session_logs(ctx.home_dir)
    tool_use_edit_found = False
    edit_input_has_markers = False
    tool_result_no_error = False
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
                        if block.get("type") == "tool_use" and block.get("name") == "Edit":
                            tool_use_edit_found = True
                            input_data = block.get("input", {})
                            old_s = str(input_data.get("old_string", ""))
                            new_s = str(input_data.get("new_string", ""))
                            if OLD_MARKER in old_s and NEW_MARKER in new_s:
                                edit_input_has_markers = True
                        elif block.get("type") == "tool_result":
                            is_error = block.get("is_error", False)
                            if not is_error:
                                tool_result_no_error = True
        except (json.JSONDecodeError, OSError):
            continue

    return {
        "name": "edit_e2e_full",
        "ok": (
            proc.returncode == 0
            and file_has_new
            and not file_has_old  # 强断言: OLD 必须不在
            and tool_use_edit_found
            and edit_input_has_markers
        ),
        "exit_code": proc.returncode,
        "file_has_new": file_has_new,
        "file_has_old_should_be_false": file_has_old,
        "tool_use_edit_found": tool_use_edit_found,
        "edit_input_has_markers": edit_input_has_markers,
        "tool_result_no_error": tool_result_no_error,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:300],
        "final_file_content": final_content[:200],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_edit_e2e_full()
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
                          "evidence": f"file_has_new={r.get('file_has_new')} file_old_gone={not r.get('file_has_old_should_be_false')} tool_use_edit={r.get('tool_use_edit_found')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M1.3 Edit 工具 e2e: 文件真改 + session log 含 Edit tool_use",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
