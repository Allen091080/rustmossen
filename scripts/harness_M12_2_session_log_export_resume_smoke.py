#!/usr/bin/env python3
"""
M12.2 — session log 真存在 + --continue 真恢复 + 普通新窗口不冒充 resume。

按 harness全链路测试.md §C.1 (M12.2 P0) 契约:
  3 进程链路 (共享 fixture HOME + 同 cwd):
    P1 (case_log_written):
      mossen -p 简单 prompt + 留下 unique marker LOG_MARKER_M12_2
      → 验 .jsonl session log 真存在 + size > 0 + 含 marker
    P2 (case_resume_works):
      mossen -p --continue, 续问 marker
      → 验 stdout 复述 marker (resume 真带回历史)
    P3 (case_new_window_isolated):
      新 mossen -p (无 --continue), 同 cwd, 问 marker
      → 验 P3 自己的 NEW session_id (不是 P1 的) + P3 jsonl 不含 marker

  与 M5.5 区分:
    M5.5 关注 resume 上下文 vs 项目记忆 (MOSSEN.md) 的并行;
    M12.2 关注 session log 持久化 + 跨进程恢复 + 新窗口隔离 三件事的存在性。

  反测信号:
    - 改 src/utils/sessionStorage.ts/sessionStoragePortable.ts 让 writeSession noop
      → P1 jsonl size = 0 / 文件缺失 → P1 fail
    - 改 --continue 不 reload session → P2 stdout 不复述 marker → P2 fail
    - 不写新 session_id, 让 P3 续 P1 → P3 jsonl 含 P1 marker → P3 fail
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

LOG_MARKER = "LOG_MARKER_M12_2_session_persist_unique_777"


def _find_session_logs(home_dir: Path) -> list[Path]:
    return list(home_dir.glob("**/projects/**/*.jsonl"))


def _read_log_text(p: Path) -> str:
    try:
        return p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def case_session_log_export_resume() -> dict:
    ctx = make_fixture("M12.2")
    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    shared_cwd = ctx.root_dir / "project_root"
    shared_cwd.mkdir(parents=True, exist_ok=True)

    mossen = str(ROOT / "run-mossen.sh")

    # ---------- P1: 留下 marker, session log 真写 ----------
    p1_prompt = (
        f"请记住这个 unique 字符串 (在 reply 里复述确认): {LOG_MARKER}. "
        f"完成后回复 OK。"
    )
    p1 = subprocess.run(
        [mossen, "-p", "--tools", ""],
        input=p1_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(shared_cwd),
    )

    p1_logs = _find_session_logs(ctx.home_dir)
    p1_session_ids = {log.stem for log in p1_logs}
    p1_log_total_size = sum(log.stat().st_size for log in p1_logs)
    p1_log_text = "\n".join(_read_log_text(log) for log in p1_logs)
    p1_jsonl_has_marker = LOG_MARKER in p1_log_text

    # ---------- P2: --continue 续, 验 resume 真把历史带回 ----------
    p2_prompt = (
        "我之前让你记住的 unique 字符串是什么? "
        "请直接打印它, 不要其它文本。"
    )
    p2 = subprocess.run(
        [mossen, "-p", "--continue", "--tools", ""],
        input=p2_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(shared_cwd),
    )
    p2_stdout_has_marker = LOG_MARKER in p2.stdout

    # ---------- P3: 新窗口 (无 --continue), 同 cwd, 不该串前会话 ----------
    p3_prompt = (
        "上一会话里我让你记住的 unique 字符串是什么? "
        "如果你不知道, 请直接回复 NO_PRIOR_CONTEXT。"
    )
    p3 = subprocess.run(
        [mossen, "-p", "--tools", ""],
        input=p3_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(shared_cwd),
    )
    p3_logs = _find_session_logs(ctx.home_dir)
    p3_only_logs = [log for log in p3_logs if log.stem not in p1_session_ids]
    p3_only_log_text = "\n".join(_read_log_text(log) for log in p3_only_logs)
    p3_stdout_has_marker = LOG_MARKER in p3.stdout
    p3_jsonl_has_marker = LOG_MARKER in p3_only_log_text
    p3_has_new_session = len(p3_only_logs) >= 1

    write_command_log(
        ctx,
        ["mossen-3-process-session-log-resume"],
        f"=== P1 stdout ===\n{p1.stdout}\n"
        f"=== P2 stdout ===\n{p2.stdout}\n"
        f"=== P3 stdout ===\n{p3.stdout}\n",
        f"=== P1 stderr ===\n{p1.stderr}\n"
        f"=== P2 stderr ===\n{p2.stderr}\n"
        f"=== P3 stderr ===\n{p3.stderr}\n",
        p3.returncode,
    )

    case_log_written_ok = (
        p1.returncode == 0
        and len(p1_logs) >= 1
        and p1_log_total_size > 0
        and p1_jsonl_has_marker
    )
    case_resume_works_ok = (
        p2.returncode == 0
        and p2_stdout_has_marker
    )
    case_new_window_isolated_ok = (
        p3.returncode == 0
        and p3_has_new_session
        and not p3_stdout_has_marker
        and not p3_jsonl_has_marker
    )

    ok = case_log_written_ok and case_resume_works_ok and case_new_window_isolated_ok

    return {
        "name": "M12_2_session_log_export_resume",
        "ok": ok,
        "p1_exit": p1.returncode,
        "p2_exit": p2.returncode,
        "p3_exit": p3.returncode,
        "case_log_written": case_log_written_ok,
        "case_resume_works": case_resume_works_ok,
        "case_new_window_isolated": case_new_window_isolated_ok,
        "p1_log_count": len(p1_logs),
        "p1_log_total_size": p1_log_total_size,
        "p1_jsonl_has_marker": p1_jsonl_has_marker,
        "p2_stdout_has_marker": p2_stdout_has_marker,
        "p3_only_log_count": len(p3_only_logs),
        "p3_stdout_has_marker": p3_stdout_has_marker,
        "p3_jsonl_has_marker": p3_jsonl_has_marker,
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
        res = case_session_log_export_resume()
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
                    f"log_written={r.get('case_log_written')} "
                    f"resume_works={r.get('case_resume_works')} "
                    f"new_window_isolated={r.get('case_new_window_isolated')} "
                    f"exits=({r.get('p1_exit')},{r.get('p2_exit')},{r.get('p3_exit')})"
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
            "M12.2 session log 持久化: P1 写真 jsonl 含 marker, "
            "P2 --continue 真把 marker 带回 stdout, "
            "P3 新窗口写 NEW session 且不串 marker"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
