#!/usr/bin/env python3
"""
M11.2 — 英文模式遇中文输入, 不应崩溃 / 空 reply (P1 positive smoke)。

按 harness全链路测试.md §C.1 / §11 契约:
  设计前提: 用户在 settings.json 里固定 language="en", 但 prompt 用了中文。
  期望行为:
    - mossen 仍能正常处理 (不 crash, exit 0)
    - 仍能产出非空回复 (不论是 zh 还是 en, 取决于 model)
    - session log 写入 user prompt + assistant reply (链路完整)

  注意: 本测是 positive smoke — model 回复语言是"自由发挥", 不强制 en/zh。
  关键是验"runtime 不会因为语言不一致就 reject 输入"这一负向边界。

  反测信号 (理论):
    - 改 src/constants/prompts.ts 让 en 模式 system prompt 强制 reject 非英文
      → exit 非 0 或空 reply → fail
    - 改 input pipeline 在 en 模式下检测中文字符直接 abort
      → fail
    实际现网这是 positive smoke (mossen 没硬 reject 异语言), 反测点偏理论。
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

CHINESE_CHAR_RE = re.compile(r"[一-鿿]")

CHINESE_PROMPT = "你好,请问 1+1 等于几? 请直接给出数字答案, 不需要解释。"


def case_chinese_input_in_english_mode() -> dict:
    ctx = make_fixture("M11.2")

    # 1) settings.json language=en
    settings_file = ctx.mossen_config_home / "settings.json"
    settings_file.write_text(json.dumps({"language": "en"}), encoding="utf-8")

    env = ctx.env.copy()
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    # 先把 LANG 清掉, 让 run-bun-featured.sh 从 settings.json 唯一推导
    env.pop("LANG", None)
    env.pop("LC_MESSAGES", None)
    env.pop("MOSSEN_UI_LANGUAGE", None)
    env.pop("MOSSENSRC_INTERACTIVE_LANGUAGE", None)

    fixture_cwd = ctx.root_dir / "project_root"
    fixture_cwd.mkdir(parents=True, exist_ok=True)

    proc = subprocess.run(
        [RUN_MOSSEN, "-p", "--tools", ""],
        input=CHINESE_PROMPT,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=str(fixture_cwd),
    )

    write_command_log(
        ctx,
        [RUN_MOSSEN, "-p", "--tools", ""],
        proc.stdout, proc.stderr, proc.returncode,
    )

    # 拉 session log 验真链路
    session_logs = list(ctx.home_dir.glob("**/projects/**/*.jsonl"))
    log_text = ""
    for log in session_logs:
        try:
            log_text += log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue

    # 强契约
    has_response = bool(proc.stdout.strip())
    user_prompt_in_log = "1+1" in log_text or "你好" in log_text
    assistant_reply_in_log = (
        '"role":"assistant"' in log_text or '"type":"assistant"' in log_text
    )

    # 弱契约 (信息性, 不入 ok 判断): reply 是中文还是英文
    reply_has_chinese = bool(CHINESE_CHAR_RE.search(proc.stdout))
    reply_has_english_letters = bool(re.search(r"[A-Za-z]", proc.stdout))

    ok = (
        proc.returncode == 0
        and has_response
        and len(session_logs) >= 1
        and user_prompt_in_log
        and assistant_reply_in_log
    )

    return {
        "name": "M11_2_chinese_input_in_english_mode_no_crash",
        "ok": ok,
        "exit_code": proc.returncode,
        "settings_language": "en",
        "prompt": CHINESE_PROMPT,
        "stdout_excerpt": proc.stdout[:300],
        "stderr_excerpt": proc.stderr[:300],
        "session_log_count": len(session_logs),
        "user_prompt_in_log": user_prompt_in_log,
        "assistant_reply_in_log": assistant_reply_in_log,
        "reply_has_chinese": reply_has_chinese,
        "reply_has_english_letters": reply_has_english_letters,
        "fixture_root": str(ctx.root_dir),
        "_ctx": ctx,
    }


def main() -> int:
    res = None
    ctx = None
    for attempt in range(3):  # transient LLM retry
        res = case_chinese_input_in_english_mode()
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
                    f"prompt_in_log={r.get('user_prompt_in_log')} "
                    f"assistant_in_log={r.get('assistant_reply_in_log')} "
                    f"reply_zh={r.get('reply_has_chinese')} "
                    f"reply_en={r.get('reply_has_english_letters')}"
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
            "M11.2 (P1 positive smoke): settings language=en + 中文 prompt → "
            "mossen 不应 crash / 不应空 reply / session log 真写。"
            "model 回复语言不强制 (zh 或 en 都接受), 重点验 runtime 健壮性。"
            "反测点偏理论 (无现网 mutation), 因 mossen 没硬 reject 异语言输入。"
        ),
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if all(r.get("ok") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
