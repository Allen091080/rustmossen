#!/usr/bin/env python3
"""
M1.5 — 工具失败有 error event, 模型能总结失败, 不提前 idle。

按 harness全链路测试.md §3.1 / §C.1 (M1.5 P0) 契约:
  前置: fixture HOME 隔离 + 一个保证不存在的目标路径
  步骤: prompt 让 model 用 Read 工具读 /tmp/nonexistent_M1_5_xyz_unique.txt
        然后总结发生了什么 (用一句话)
  观察点 (强契约):
    1. mossen exit_code == 0 (主流程正常退出, 不是 crash)
    2. session log 含 tool_use Read with input.file_path 含目标
    3. session log 含 tool_result is_error=true 或 content 含 error 字面
       (ENOENT / no such / does not exist / not found / 不存在 / 找不到)
    4. session log 在 tool_result 之后还有 assistant message
       (model 真"总结"了 error, 不是停在 tool_use 阶段)
    5. stdout (final reply) 含至少一个 error 字面
       (model 真把失败告诉用户了)
  反测信号:
    - 改 src/tools/FileReadTool/FileReadTool.ts call() 让 not-found 也返
      success + 空字符串 → tool_result 不 is_error → 观察点 3 fail
    - 改 agent loop 在 tool_result error 后直接 break → 观察点 4 fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

NONEXISTENT_TARGET = "/tmp/nonexistent_M1_5_xyz_unique_target_8472.txt"


def _find_session_logs(home_dir: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in home_dir.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def _scan_log(log_file: Path, target_path: str) -> dict:
    """返回 {read_tool_use, tool_result_error, post_error_assistant}"""
    out = {
        "read_tool_use": False,
        "tool_result_error": False,
        "post_error_assistant": False,
    }
    error_keywords = (
        "enoent", "no such", "does not exist", "not found",
        "不存在", "找不到", "no_such",
    )
    saw_error_result = False
    try:
        for line in log_file.read_text(encoding="utf-8", errors="replace").splitlines():
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = obj.get("message", obj)
            role = msg.get("role") or obj.get("type")
            content = msg.get("content")

            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_use" and block.get("name") == "Read":
                        input_data = block.get("input", {})
                        if isinstance(input_data, dict) and target_path in str(input_data.get("file_path", "")):
                            out["read_tool_use"] = True
                    elif block.get("type") == "tool_result":
                        if block.get("is_error") is True:
                            out["tool_result_error"] = True
                            saw_error_result = True
                        result_content = json.dumps(block.get("content", ""), ensure_ascii=False).lower()
                        if any(kw in result_content for kw in error_keywords):
                            out["tool_result_error"] = True
                            saw_error_result = True

            # 在 saw_error_result 之后, 看到 assistant message (含 text content)
            if saw_error_result and role == "assistant" and isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "text":
                        text_val = block.get("text", "")
                        if isinstance(text_val, str) and text_val.strip():
                            out["post_error_assistant"] = True
                            break
    except OSError:
        pass
    return out


def case_tool_failure_recovery() -> dict:
    ctx = make_fixture("M1.5")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # 双保险: 先确认 target 真不存在 (不创建)
    target = Path(NONEXISTENT_TARGET)
    if target.exists():
        target.unlink()

    prompt = (
        f"请用 Read 工具读取这个文件: {NONEXISTENT_TARGET} . "
        f"如果读取失败, 请用一句话告诉我具体发生了什么 (包括错误原因)。"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Read"],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(
        ctx,
        [str(ROOT / "run-mossen.sh"), "-p", "--allowedTools", "Read"],
        proc.stdout,
        proc.stderr,
        proc.returncode,
    )

    session_logs = _find_session_logs(ctx.home_dir)
    agg = {"read_tool_use": False, "tool_result_error": False, "post_error_assistant": False}
    for log in session_logs:
        scan = _scan_log(log, NONEXISTENT_TARGET)
        for k in agg:
            agg[k] = agg[k] or scan[k]

    error_keywords_stdout = (
        "enoent", "no such", "does not exist", "not found",
        "不存在", "找不到", "失败", "错误", "error",
    )
    stdout_lower = proc.stdout.lower()
    stdout_has_error_word = any(kw in stdout_lower for kw in error_keywords_stdout)

    ok = (
        proc.returncode == 0
        and agg["read_tool_use"]
        and agg["tool_result_error"]
        and agg["post_error_assistant"]
        and stdout_has_error_word
    )

    return {
        "name": "M1_5_tool_failure_recovery",
        "ok": ok,
        "exit_code": proc.returncode,
        "read_tool_use_in_log": agg["read_tool_use"],
        "tool_result_error_in_log": agg["tool_result_error"],
        "post_error_assistant_in_log": agg["post_error_assistant"],
        "stdout_has_error_word": stdout_has_error_word,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:400],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_tool_failure_recovery()
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
                    f"read_tool_use={r.get('read_tool_use_in_log')} "
                    f"tool_result_error={r.get('tool_result_error_in_log')} "
                    f"post_error_assistant={r.get('post_error_assistant_in_log')} "
                    f"stdout_has_error={r.get('stdout_has_error_word')} "
                    f"exit={r.get('exit_code')}"
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
            "M1.5 工具失败可恢复: Read 不存在文件 → tool_result is_error → "
            "model 续生成 assistant message 总结 → stdout 含 error 字面"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
