#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

from wave_w279_terminal_slow_first_token_heartbeat_pty_smoke import (
    HEAD_MARKER,
    run_slow_first_token_heartbeat_smoke,
)


def main() -> int:
    result = run_slow_first_token_heartbeat_smoke()
    pty_output = Path(result["artifacts"]["pty_output"])
    assertions_path = Path(result["artifacts"]["assertions"])
    text = pty_output.read_text(encoding="utf-8", errors="replace")

    head_pos = text.find(HEAD_MARKER)
    assistant_summary_pos = text.find("assistant text:")
    byte_summary_pos = text.find("bytes: 29")
    content_visible = head_pos >= 0
    text_before_byte_summary = content_visible and (
        assistant_summary_pos < 0 or head_pos < assistant_summary_pos
    )
    no_first_token_byte_summary = byte_summary_pos < 0 or head_pos < byte_summary_pos
    ok = bool(result["ok"] and content_visible and text_before_byte_summary and no_first_token_byte_summary)

    evidence_start = max(0, head_pos - 350) if head_pos >= 0 else 0
    evidence = text[evidence_start : evidence_start + 900]
    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_assistant_activity_shows_text_before_byte_summary",
                "expected": True,
                "actual": text_before_byte_summary,
                "passed": text_before_byte_summary,
                "evidence": evidence,
            },
            {
                "name": "terminal_assistant_activity_first_token_omits_byte_summary",
                "expected": True,
                "actual": no_first_token_byte_summary,
                "passed": no_first_token_byte_summary,
                "evidence": evidence,
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "head_marker_pos": head_pos,
            "assistant_summary_pos": assistant_summary_pos,
            "byte_summary_pos": byte_summary_pos,
            "assistant_text_visible_before_byte_summary": text_before_byte_summary,
            "first_token_byte_summary_visible_before_text": not no_first_token_byte_summary,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
