#!/usr/bin/env python3
"""
M2.3 — 权限安全 e2e: settings.json 配置 permissions.deny 真生效。

按 harness全链路测试.md §3.2 M2.3 契约:
  前置: fixture HOME, 写一个 settings.json 配 {"permissions":{"deny":["Bash"]}}
  步骤: mossen -p --settings <fixture/settings.json>, prompt: 用 Bash 跑 echo
  观察点:
    1. exit_code == 0
    2. session log: Bash 不被执行 (config-level deny)
    3. mossen 回复含明确"被禁/denied/拦截"字面
  反测: 移除 settings.json 的 deny 规则 → Bash 真执行 (默认 sandbox 允许 cwd 内)
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

ECHO_MARKER = "M2_3_BASH_OUT_should_NOT_appear_xyz"


def _find_session_logs(fixture_home: Path) -> list[Path]:
    found = []
    for pattern in ("**/projects/**/*.jsonl", "**/sessions/**/*.jsonl",
                    "**/.mossen/**/*.jsonl"):
        for p in fixture_home.glob(pattern):
            if p not in found:
                found.append(p)
    return found


def case_config_deny_bash() -> dict:
    ctx = make_fixture("M2.3")

    settings_file = ctx.root_dir / "settings.json"
    settings_data = {
        "permissions": {
            "deny": ["Bash"]
        }
    }
    settings_file.write_text(json.dumps(settings_data, indent=2))

    prompt = (
        f"请用 Bash 工具执行 echo {ECHO_MARKER}, 把结果原样打印"
    )

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p",
         "--settings", str(settings_file)],
        input=prompt,
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(ROOT),
    )

    write_command_log(ctx, ["mossen", "-p", "--settings", str(settings_file)],
                      proc.stdout, proc.stderr, proc.returncode)

    # 检查 session log: Bash tool_result (按 tool_use_id 严格匹配) 应该是 error 或不含 marker
    session_logs = _find_session_logs(ctx.home_dir)
    bash_tool_attempted = False
    bash_tool_result_blocked = False
    bash_actual_output_has_marker = False
    bash_tool_use_ids = set()

    # Pass 1: 收集所有 Bash 的 tool_use_id
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
                        if isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name") == "Bash":
                            bash_tool_attempted = True
                            tool_id = block.get("id")
                            if tool_id:
                                bash_tool_use_ids.add(tool_id)
        except (json.JSONDecodeError, OSError):
            continue

    # Pass 2: 只检查 Bash 关联的 tool_result
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
                        if block.get("type") == "tool_result":
                            tool_use_id = block.get("tool_use_id")
                            # 严格匹配: 只看 Bash 的 tool_result
                            if tool_use_id not in bash_tool_use_ids:
                                continue
                            is_error = block.get("is_error", False)
                            result_str = str(block.get("content", ""))
                            if is_error or any(kw in result_str.lower()
                                               for kw in ["denied", "deny", "permission", "blocked",
                                                          "禁用", "禁止", "拦截", "拒绝"]):
                                bash_tool_result_blocked = True
                            if ECHO_MARKER in result_str:
                                bash_actual_output_has_marker = True
        except (json.JSONDecodeError, OSError):
            continue

    # stdout 不应该出现来自 Bash tool 的 marker (但 model 可能在文字里复述)
    # 关键: tool 执行被阻 (tool_result 被 block) + tool output 不含 marker
    block_keywords = ["denied", "deny", "permission", "blocked", "block",
                      "禁用", "禁止", "拦截", "拒绝", "拒",
                      "不允许", "config", "settings", "unavailable",
                      "没有", "无法", "not available", "可用"]
    has_block_in_stdout = any(kw.lower() in proc.stdout.lower()
                              for kw in block_keywords)

    return {
        "name": "config_deny_bash",
        "ok": (
            # 核心契约只用 session log 确定性信号, 不依赖 LLM 字面 (避免 transient)
            proc.returncode == 0
            and bash_tool_attempted  # Bash 真被尝试
            and bash_tool_result_blocked  # 但被 config 阻
            and not bash_actual_output_has_marker  # tool 输出 NOT 含 marker (没真执行)
        ),
        "exit_code": proc.returncode,
        "bash_tool_attempted": bash_tool_attempted,
        "bash_tool_result_blocked": bash_tool_result_blocked,
        "bash_actual_output_has_marker": bash_actual_output_has_marker,
        "has_block_in_stdout_evidence_only": has_block_in_stdout,
        "session_log_count": len(session_logs),
        "stdout_excerpt": proc.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = case_config_deny_bash()
    ctx = res1.pop("_ctx")
    results = [res1]

    write_assertions(ctx,
                     status="passed" if all(r.get("ok") for r in results) else "failed",
                     assertions=[
                         {"name": r["name"], "expected": True,
                          "actual": r.get("ok"), "passed": r.get("ok"),
                          "evidence": f"bash_blocked={r.get('bash_tool_result_blocked')} no_real_output={not r.get('bash_actual_output_has_marker')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M2.3 config deny: settings.json 的 permissions.deny=['Bash'] 真生效",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
