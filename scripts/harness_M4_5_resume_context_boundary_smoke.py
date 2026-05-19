#!/usr/bin/env python3
"""
M4.5 — --resume <session-id> 显式恢复 vs 新窗口边界 (P1)。

按 harness全链路测试.md §3.4 / §C.1 M4.5 契约:
  M5.5 已用 --continue 验过"resume 上下文 vs 项目记忆"边界, 本测聚焦:
    - 用 --resume <session-id> 显式定位 (不靠"最近 cwd"启发式)
    - 验同一 session_id 真带回会话历史
    - 验新窗口同 cwd 不串到该 session 的会话 marker

  M4.5 区分点 vs M5.5:
    M5.5: --continue (隐式找 cwd 最近 session)
    M4.5: --resume <session-id> (显式按 ID 定位 session)
    两条路径在 main.tsx:786 和 commander 1029 的 -r/--resume <value> 共存,
    但绑定的 session locator 不同 — 本测覆盖 ID-based locator。

  3 进程链路:
    P1: cwd=fixture_cwd, 留 CONVO_MARKER 到 session_1 (无 --continue, 新会话)。
        从 session log 文件名提取 session_id (jsonl 文件 stem)。
    P2: --resume <session_id_from_p1>, 同 cwd. 提问 model CONVO_MARKER。
        验: reply 含 CONVO_MARKER (--resume 真按 ID 把 session_1 历史载入)。
    P3: NEW window, 同 cwd, 不 --resume / 不 --continue. 提问 model CONVO_MARKER。
        验: reply NOT 含 CONVO_MARKER (新会话不串)。
        验: P3 自己的 session log 不含 CONVO_MARKER。

  反测信号:
    - src/main.tsx:786 让 --resume value 被忽略 (强制走 --continue 隐式路径)
      → P2 仍能找 session_1 (隐式同 cwd 命中) — 不易抓
    - src/main.tsx 让 --resume <id> 当 ID 错误时 fail-soft 启 fresh session
      → P2 reply 缺 CONVO_MARKER → fail
    - 若 --resume 链路被偷换成"恢复随便一个 session"
      → P2 可能拿不到 P1 的 marker → fail

  注意: --resume 在 -p 模式下走 main.tsx:3354 的 sessionId 解析路径 — 必须给
        合法 UUID, 否则会 exitWithError(`Failed to resume session ${sessionId}`)。
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions, write_command_log

RUN_MOSSEN = str(ROOT / "run-mossen.sh")

CONVO_MARKER = "M4_5_CONVO_MARKER_resume_id_path_unique"
UUID_RE = re.compile(
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-"
    r"[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)


def _find_session_logs(home_dir: Path) -> list[Path]:
    return list(home_dir.glob("**/projects/**/*.jsonl"))


def _extract_session_ids(logs: list[Path]) -> list[str]:
    """jsonl 文件名 stem 即 session_id (UUID 形式)。"""
    ids = []
    for log in logs:
        stem = log.stem
        if UUID_RE.match(stem):
            ids.append(stem)
    return ids


def case_resume_by_id_vs_new_window() -> dict:
    ctx = make_fixture("M4.5")

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)

    # ---------------- P1: 留 CONVO_MARKER, 抓 session_id ----------------
    p1_prompt = (
        f"请记住这个 unique 字符串 (仅在你的 reply 中复述一次确认): "
        f"{CONVO_MARKER}. 完成后回复 OK 即可。"
    )
    p1 = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=p1_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )

    p1_logs = _find_session_logs(ctx.home_dir)
    p1_ids = _extract_session_ids(p1_logs)
    target_session_id = p1_ids[0] if p1_ids else None
    p1_session_id_set = set(p1_ids)

    if target_session_id is None:
        write_command_log(
            ctx,
            ["mossen-resume-by-id-no-session-id"],
            f"=== P1 ===\n{p1.stdout}\n",
            f"=== P1 ===\n{p1.stderr}\n",
            p1.returncode,
        )
        return {
            "name": "M4_5_resume_by_id_vs_new_window",
            "ok": False,
            "stage": "no_session_id_after_p1",
            "p1_exit": p1.returncode,
            "p1_log_count": len(p1_logs),
            "p1_session_ids": p1_ids,
            "stdout_excerpt": p1.stdout[:300],
            "stderr_excerpt": p1.stderr[:300],
            "fixture_root": str(ctx.root_dir),
            "_ctx": ctx,
        }

    # ---------------- P2: --resume <session_id> 显式 ----------------
    p2_prompt = (
        f"我刚才让你记住的 unique 字符串是什么? 请只输出该字符串本身, "
        f"不要其它文本。"
    )
    p2 = subprocess.run(
        [RUN_MOSSEN, "-p", "--resume", target_session_id, "--tools", ""],
        input=p2_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )

    # ---------------- P3: NEW window (no --resume, no --continue) ----------------
    p3_prompt = (
        f"上一会话里我让你记住的 unique 字符串是什么? "
        f"如果你不知道, 直接回 'NO_PRIOR_CONTEXT'。 "
        f"请只输出一行结果。"
    )
    p3 = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=p3_prompt, env=env,
        capture_output=True, text=True, timeout=240,
        cwd=str(fixture_cwd),
    )

    p3_logs = _find_session_logs(ctx.home_dir)
    # P3 自己的 session log = stem 不在 P1 已有 ids 集合里
    p3_only_logs = [log for log in p3_logs if log.stem not in p1_session_id_set]
    p3_only_log_text = ""
    for log in p3_only_logs:
        try:
            p3_only_log_text += log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue

    write_command_log(
        ctx,
        ["mossen-3-process-resume-by-id"],
        f"=== P1 ===\n{p1.stdout}\n=== P2 (--resume {target_session_id}) ===\n{p2.stdout}\n=== P3 (new window) ===\n{p3.stdout}\n",
        f"=== P1 ===\n{p1.stderr}\n=== P2 ===\n{p2.stderr}\n=== P3 ===\n{p3.stderr}\n",
        p3.returncode,
    )

    p2_has_convo = CONVO_MARKER in p2.stdout
    p3_has_convo_in_reply = CONVO_MARKER in p3.stdout
    p3_has_convo_in_log = CONVO_MARKER in p3_only_log_text

    ok = (
        p1.returncode == 0
        and p2.returncode == 0
        and p3.returncode == 0
        and p2_has_convo                      # --resume <id> 真带回 session 历史
        and not p3_has_convo_in_reply         # 新窗口不串 reply
        and not p3_has_convo_in_log           # 新窗口 session log 真干净
        and len(p3_only_logs) >= 1            # P3 真起了新 session
    )

    return {
        "name": "M4_5_resume_by_id_vs_new_window",
        "ok": ok,
        "p1_exit": p1.returncode,
        "p2_exit": p2.returncode,
        "p3_exit": p3.returncode,
        "target_session_id": target_session_id,
        "p1_session_ids": p1_ids,
        "p2_has_convo_marker": p2_has_convo,
        "p3_has_convo_marker_in_reply": p3_has_convo_in_reply,
        "p3_has_convo_marker_in_log": p3_has_convo_in_log,
        "p3_new_log_count": len(p3_only_logs),
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
        res = case_resume_by_id_vs_new_window()
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
                    f"target_id={r.get('target_session_id')} "
                    f"P2_resume_convo={r.get('p2_has_convo_marker')} "
                    f"P3_no_convo_reply={not r.get('p3_has_convo_marker_in_reply')} "
                    f"P3_no_convo_log={not r.get('p3_has_convo_marker_in_log')} "
                    f"P3_new_logs={r.get('p3_new_log_count')}"
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
            "M4.5: P1 留 marker 并抓 session_id (jsonl stem) → P2 用 --resume "
            "<session_id> 显式恢复 → P3 新窗口同 cwd 无 --resume 不串。"
            "区分 M5.5: M5.5 用 --continue (隐式 cwd locator), M4.5 用 "
            "--resume <id> (显式 ID locator), 覆盖不同 main.tsx flag 路径。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
