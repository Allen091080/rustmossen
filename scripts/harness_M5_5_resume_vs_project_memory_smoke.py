#!/usr/bin/env python3
"""
M5.5 — resume 上下文 vs 项目记忆 — 不能把会话记录误当项目记忆。

按 harness全链路测试.md §C.1 + §C.5 契约:
  关键区分:
    - 项目记忆 (MOSSEN.md): 任意 mossen 进程 cd 到该目录都能读
    - resume 上下文 (--continue): 只在显式 --continue 时才带回会话历史
    - 新窗口 (无 --continue) 不该串到上一会话内容

  3 进程链路 (顺序很关键, 因 --continue 取最新 session):
    P1: cwd=fixture_cwd, 让 model 留下 CONVO_MARKER + 同时 fixture_cwd 已有
        MOSSEN.md 含 PROJECT_MARKER (建立 session_1)
    P2: --continue, 同 cwd. 提问 model CONVO_MARKER. 验:
          * reply 含 CONVO_MARKER (resume 真带回 session_1 内容)
          * reply 含 PROJECT_MARKER (项目记忆并行可见)
    P3: NEW window, 同 cwd, 不 --continue (新 session). 提问 model 关于
        PROJECT_MARKER 和 CONVO_MARKER. 验:
          * reply 含 PROJECT_MARKER (项目记忆在新会话也读到)
          * reply 不含 CONVO_MARKER (新会话不串过去会话历史)
          * P3 自己的 session log 不含 P1 的 CONVO_MARKER

  反测信号:
    - src/utils/mossenmd.ts 跳过 Project 分支 → P3 reply 缺 PROJECT_MARKER → fail
    - mossen 启动时 fall back to --continue → P3 串 P1/P2 history → reply 含
      CONVO_MARKER → fail (我们期望 P3 不含)
    - --continue 不 reload session → P2 reply 缺 CONVO_MARKER → fail
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

PROJECT_MARKER = "M5_5_PROJECT_MARKER_alpha_unique"
CONVO_MARKER = "M5_5_CONVO_MARKER_omega_secret_42"


def _find_session_logs(home_dir: Path) -> list[Path]:
    return list(home_dir.glob("**/projects/**/*.jsonl"))


def case_resume_vs_project_memory() -> dict:
    ctx = make_fixture("M5.5")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    project_md = fixture_cwd / "MOSSEN.md"
    project_md.write_text(
        f"# M5.5 project memory\n\n"
        f"Project marker: {PROJECT_MARKER}\n\n"
        f"Reply by quoting this marker when asked about project context.\n",
        encoding="utf-8",
    )

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # ---------------- P1: 留 CONVO_MARKER ----------------
    p1_prompt = (
        f"请记住这个 unique 字符串 (不写文件, 仅在你的 reply 中确认): "
        f"{CONVO_MARKER}. 完成后回复 OK 即可。"
    )
    p1 = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=p1_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )
    p1_logs = _find_session_logs(ctx.home_dir)
    p1_session_ids = {log.stem for log in p1_logs}

    # ---------------- P2: --continue (validates resume works) ----------------
    p2_prompt = (
        f"我之前让你记住的 unique 字符串是什么? 请直接打印它和 MOSSEN.md "
        f"里的 marker, 各占一行, 不要其它文本。"
    )
    p2 = subprocess.run(
        [RUN_MOSSEN, "-p", "--continue", "--tools", ""],
        input=p2_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )

    # ---------------- P3: NEW window (no --continue) ----------------
    p3_prompt = (
        "请在两行内回答: \n"
        "1) MOSSEN.md 里的 marker 是什么? \n"
        "2) 上一会话里我让你记住的 unique 字符串是什么? "
        "如果你不知道, 直接回 'NO_PRIOR_CONTEXT'。 \n"
        "请直接打印, 不要其它文本。"
    )
    p3 = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=p3_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )
    # P3 写入的新 jsonl (session_id 不在 p1_session_ids)
    p3_logs = _find_session_logs(ctx.home_dir)
    p3_only_logs = [log for log in p3_logs if log.stem not in p1_session_ids]
    p3_only_log_text = ""
    for log in p3_only_logs:
        try:
            p3_only_log_text += log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue

    write_command_log(
        ctx,
        ["mossen-3-process-resume-vs-project"],
        f"=== P1 ===\n{p1.stdout}\n=== P2 ===\n{p2.stdout}\n=== P3 ===\n{p3.stdout}\n",
        f"=== P1 ===\n{p1.stderr}\n=== P2 ===\n{p2.stderr}\n=== P3 ===\n{p3.stderr}\n",
        p3.returncode,
    )

    p2_has_convo = CONVO_MARKER in p2.stdout
    p2_has_project = PROJECT_MARKER in p2.stdout
    p3_has_project = PROJECT_MARKER in p3.stdout
    p3_has_convo = CONVO_MARKER in p3.stdout
    p3_log_has_convo = CONVO_MARKER in p3_only_log_text

    ok = (
        p1.returncode == 0
        and p2.returncode == 0
        and p3.returncode == 0
        and p2_has_convo  # resume works
        and p2_has_project  # project memory parallel
        and p3_has_project  # project memory always loaded
        and not p3_has_convo  # new window doesn't inherit
        and not p3_log_has_convo  # P3's own session log truly clean
    )

    return {
        "name": "M5_5_resume_vs_project_memory",
        "ok": ok,
        "p1_exit": p1.returncode,
        "p2_exit": p2.returncode,
        "p3_exit": p3.returncode,
        "p2_has_convo_marker": p2_has_convo,
        "p2_has_project_marker": p2_has_project,
        "p3_has_project_marker": p3_has_project,
        "p3_has_convo_marker_in_reply": p3_has_convo,
        "p3_has_convo_marker_in_log": p3_log_has_convo,
        "p3_log_count": len(p3_only_logs),
        "p1_stdout_excerpt": p1.stdout[:200],
        "p2_stdout_excerpt": p2.stdout[:300],
        "p3_stdout_excerpt": p3.stdout[:300],
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_resume_vs_project_memory()
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
                    f"P2_convo={r.get('p2_has_convo_marker')} "
                    f"P2_proj={r.get('p2_has_project_marker')} "
                    f"P3_proj={r.get('p3_has_project_marker')} "
                    f"P3_no_convo_reply={not r.get('p3_has_convo_marker_in_reply')} "
                    f"P3_no_convo_log={not r.get('p3_has_convo_marker_in_log')}"
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
            "M5.5: P1 留 conversation marker → P2 --continue 真 resume → "
            "P3 新窗口同 cwd 不串会话; 项目记忆 vs resume 上下文严格区分。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
