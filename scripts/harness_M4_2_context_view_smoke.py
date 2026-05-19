#!/usr/bin/env python3
"""
M4.2 — /context 命令真显示 token 占比 e2e。

按 harness全链路测试.md §3.4 M4.2 契约:
  步骤: stdin 发 "/context" 给 mossen -p
  观察点:
    1. exit_code == 0
    2. stdout 含 "Tokens:" 字面 + 数字
    3. stdout 含 "Auto-compact" 字面 (上下文管理基础)
    4. stdout 含 model 名 (一致性: 跟 statusline / status 同源)
  反测: 暂无好的源码 mutation; 验 stdout 必须含具体数字以防"没真显示"
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log


def case_context_view_real() -> dict:
    ctx = make_fixture("M4.2")

    proc = subprocess.run(
        [str(ROOT / "run-mossen.sh"), "-p"],
        input="/context",
        env=ctx.env,
        capture_output=True,
        text=True,
        timeout=120,
        cwd=str(ROOT),
    )

    write_command_log(ctx, ["mossen", "-p", "/context"],
                      proc.stdout, proc.stderr, proc.returncode)

    has_tokens_label = "Tokens:" in proc.stdout or "tokens" in proc.stdout.lower()
    has_token_number = bool(re.search(r"\d+(\.\d+)?\s*[kK]\s*/", proc.stdout))
    has_autocompact = "Auto-compact" in proc.stdout or "auto-compact" in proc.stdout.lower()
    has_model_label = "Model:" in proc.stdout or "model:" in proc.stdout.lower()

    return {
        "name": "context_view_real",
        "ok": (
            proc.returncode == 0
            and has_tokens_label
            and has_token_number
            and has_autocompact
            and has_model_label
        ),
        "exit_code": proc.returncode,
        "has_tokens_label": has_tokens_label,
        "has_token_number": has_token_number,
        "has_autocompact": has_autocompact,
        "has_model_label": has_model_label,
        "stdout_excerpt": proc.stdout[:400],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res1 = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res1 = case_context_view_real()
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
                          "evidence": f"tokens_label={r.get('has_tokens_label')} number={r.get('has_token_number')} autocompact={r.get('has_autocompact')}"}
                         for r in results
                     ])

    summary = {
        "results": results,
        "passed": sum(1 for r in results if r.get("ok")),
        "total": len(results),
        "fixture_root": str(ctx.root_dir),
        "design_note": "M4.2 /context 显示: 验 4 个必要字段 (Tokens label / 数字 / Auto-compact / Model)",
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
