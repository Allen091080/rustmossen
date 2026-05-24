#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w144_stream_json_terminal_frontend_pty_smoke import (
    HEAD_MARKER,
    TAIL_MARKER,
    run_terminal_emit_pty_smoke,
)

TEST_ID = "W146_stream_json_terminal_scrollback_transcript_smoke"


def main() -> int:
    result = run_terminal_emit_pty_smoke(TEST_ID)
    pty_output = Path(result["artifacts"]["pty_output"])
    assertions_path = Path(result["artifacts"]["assertions"])
    text = pty_output.read_text(encoding="utf-8", errors="replace")

    transcript_index = text.rfind("assistant transcript")
    transcript_slice = text[transcript_index:] if transcript_index >= 0 else ""
    has_transcript_block = transcript_index >= 0
    transcript_has_head = HEAD_MARKER in transcript_slice
    transcript_has_tail = TAIL_MARKER in transcript_slice
    transcript_uses_normal_lines = "\r\n" in transcript_slice or "\n" in transcript_slice
    no_alt_screen = result["alt_enters"] == 0 and result["alt_leaves"] == 0
    no_full_clear = result["full_clears"] == 0

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_emit_commits_transcript_block",
                "expected": True,
                "actual": has_transcript_block,
                "passed": has_transcript_block,
                "evidence": "assistant transcript header present in PTY output",
            },
            {
                "name": "terminal_emit_transcript_contains_stream_head",
                "expected": True,
                "actual": transcript_has_head,
                "passed": transcript_has_head,
                "evidence": HEAD_MARKER,
            },
            {
                "name": "terminal_emit_transcript_contains_stream_tail",
                "expected": True,
                "actual": transcript_has_tail,
                "passed": transcript_has_tail,
                "evidence": TAIL_MARKER,
            },
            {
                "name": "terminal_emit_transcript_uses_normal_scrollback_lines",
                "expected": True,
                "actual": transcript_uses_normal_lines,
                "passed": transcript_uses_normal_lines,
                "evidence": "transcript block contains line breaks after header",
            },
            {
                "name": "terminal_emit_transcript_keeps_screen_mode_safe",
                "expected": True,
                "actual": no_alt_screen and no_full_clear,
                "passed": no_alt_screen and no_full_clear,
                "evidence": f"alt={result['alt_enters']}/{result['alt_leaves']} full_clears={result['full_clears']}",
            },
        ]
    )
    ok = bool(
        result["ok"]
        and has_transcript_block
        and transcript_has_head
        and transcript_has_tail
        and transcript_uses_normal_lines
        and no_alt_screen
        and no_full_clear
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "transcript_block": has_transcript_block,
            "transcript_head": transcript_has_head,
            "transcript_tail": transcript_has_tail,
            "transcript_uses_normal_lines": transcript_uses_normal_lines,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
