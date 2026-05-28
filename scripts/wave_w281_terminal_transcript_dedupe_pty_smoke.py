#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

from wave_w279_terminal_slow_first_token_heartbeat_pty_smoke import (
    HEAD_MARKER,
    TAIL_MARKER,
    run_slow_first_token_heartbeat_smoke,
)


def main() -> int:
    result = run_slow_first_token_heartbeat_smoke()
    pty_output = Path(result["artifacts"]["pty_output"])
    assertions_path = Path(result["artifacts"]["assertions"])
    text = pty_output.read_text(encoding="utf-8", errors="replace")

    transcript_header_count = text.count("assistant transcript")
    transcript_index = text.rfind("assistant transcript")
    transcript_slice = text[transcript_index:] if transcript_index >= 0 else ""
    transcript_head_count = transcript_slice.count(HEAD_MARKER)
    transcript_tail_count = transcript_slice.count(TAIL_MARKER)
    transcript_first_row_count = transcript_slice.count(
        "terminal-heartbeat-row-000: slow first token stayed visible."
    )
    ok = bool(
        result["ok"]
        and transcript_header_count == 1
        and transcript_head_count == 1
        and transcript_tail_count == 1
        and transcript_first_row_count == 1
    )

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_transcript_header_appended_once",
                "expected": 1,
                "actual": transcript_header_count,
                "passed": transcript_header_count == 1,
                "evidence": "assistant transcript header count",
            },
            {
                "name": "terminal_transcript_head_marker_not_duplicated",
                "expected": 1,
                "actual": transcript_head_count,
                "passed": transcript_head_count == 1,
                "evidence": HEAD_MARKER,
            },
            {
                "name": "terminal_transcript_tail_marker_not_duplicated",
                "expected": 1,
                "actual": transcript_tail_count,
                "passed": transcript_tail_count == 1,
                "evidence": TAIL_MARKER,
            },
            {
                "name": "terminal_transcript_stream_row_not_duplicated",
                "expected": 1,
                "actual": transcript_first_row_count,
                "passed": transcript_first_row_count == 1,
                "evidence": "terminal-heartbeat-row-000",
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "transcript_header_count": transcript_header_count,
            "transcript_head_count": transcript_head_count,
            "transcript_tail_count": transcript_tail_count,
            "transcript_first_row_count": transcript_first_row_count,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
