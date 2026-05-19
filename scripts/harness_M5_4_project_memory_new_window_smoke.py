#!/usr/bin/env python3
"""
M5.4 — 新窗口同 cwd 自动加载项目记忆 (不是 resume, 也能读 MOSSEN.md)。

按 harness全链路测试.md §C.1 + §C.5 契约:
  用户场景: 用户敲 mossen 在某个项目目录 (没用 --continue), mossen 仍能
  自动读取 cwd 下的 MOSSEN.md 注入到 model context.

  步骤:
    1. fixture_cwd 下写 MOSSEN.md 含 unique marker
    2. 启动 mossen -p 单 shot, cwd=fixture_cwd, NOT --continue
    3. 验:
       a) session log 文件存在 (新会话已写)
       b) jsonl 内 user/system message 含 marker (项目记忆被注入到 prompt)
       c) reply 含 marker (model 真"看到"了 MOSSEN.md)

  反测信号: src/utils/mossenmd.ts 让 Project 分支返回空数组
            → MOSSEN.md 没注入 → reply 缺 marker 且 jsonl 不含 marker → fail

  CWD 真实路径机制:
    run-bun-featured.sh 先 capture $PWD 为 MOSSENSRC_LAUNCH_CWD env, 再 cd ROOT.
    bootstrap/state.ts:288 读 MOSSENSRC_LAUNCH_CWD 作为 originalCwd.
    所以 subprocess.run(cwd=fixture_cwd) 真把 mossen 的 originalCwd 切到 fixture.
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
PROJECT_MEMORY_MARKER = "M5_4_PROJECT_MEMORY_MARKER_xyz_unique"


def _find_session_logs(home_dir: Path) -> list[Path]:
    found = []
    for p in home_dir.glob("**/projects/**/*.jsonl"):
        if p not in found:
            found.append(p)
    return found


def case_project_memory_loaded_new_window() -> dict:
    ctx = make_fixture("M5.4")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    project_md = fixture_cwd / "MOSSEN.md"
    project_md.write_text(
        f"# Project context for M5.4\n\n"
        f"This file contains a unique marker: {PROJECT_MEMORY_MARKER}\n\n"
        f"Please always include this marker in any reply when asked about project context.\n",
        encoding="utf-8",
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    prompt = (
        "请仅根据 MOSSEN.md (项目记忆) 回复: 这个项目的 unique marker 是什么? "
        "请把 MOSSEN.md 里出现的 marker 字符串原样输出, 不需要其他文本。"
    )

    proc = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=prompt,
        env=env,
        capture_output=True,
        text=True,
        timeout=240,
        cwd=str(fixture_cwd),
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "-p", "--tools", ""],
        proc.stdout, proc.stderr, proc.returncode,
    )

    session_logs = _find_session_logs(ctx.home_dir)
    log_text = ""
    for log in session_logs:
        try:
            log_text += log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue

    marker_in_log = PROJECT_MEMORY_MARKER in log_text
    marker_in_stdout = PROJECT_MEMORY_MARKER in proc.stdout

    return {
        "name": "M5_4_project_memory_loaded_new_window",
        "ok": (
            proc.returncode == 0
            and len(session_logs) >= 1
            and marker_in_log
            and marker_in_stdout
        ),
        "exit_code": proc.returncode,
        "fixture_cwd": str(fixture_cwd),
        "project_md_path": str(project_md),
        "session_log_count": len(session_logs),
        "marker_in_log": marker_in_log,
        "marker_in_stdout": marker_in_stdout,
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_project_memory_loaded_new_window()
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
                    f"exit={r.get('exit_code')} "
                    f"sessions={r.get('session_log_count')} "
                    f"marker_in_log={r.get('marker_in_log')} "
                    f"marker_in_stdout={r.get('marker_in_stdout')}"
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
            "M5.4: 新窗口 (no --continue) 同 cwd 启动 mossen, "
            "fixture cwd MOSSEN.md 必须被自动注入 → model 能引用 marker。"
            "走 run-bun-featured.sh MOSSENSRC_LAUNCH_CWD 路径 → originalCwd → "
            "mossenmd.getMemoryFiles() 找 Project MOSSEN.md。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
