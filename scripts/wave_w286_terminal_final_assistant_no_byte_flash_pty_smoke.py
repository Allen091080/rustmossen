#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

from wave_w279_terminal_slow_first_token_heartbeat_pty_smoke import (
    run_slow_first_token_heartbeat_smoke,
)


def main() -> int:
    result = run_slow_first_token_heartbeat_smoke()
    pty_output = Path(result["artifacts"]["pty_output"])
    assertions_path = Path(result["artifacts"]["assertions"])
    text = pty_output.read_text(encoding="utf-8", errors="replace")

    no_assistant_byte_summary = "assistant text:" not in text
    no_first_token_byte_counter = "bytes: 29" not in text
    transcript_still_committed = "assistant transcript" in text
    ok = bool(
        result["ok"]
        and no_assistant_byte_summary
        and no_first_token_byte_counter
        and transcript_still_committed
    )

    pos = text.find("assistant transcript")
    evidence_start = max(0, pos - 700) if pos >= 0 else 0
    evidence = text[evidence_start : evidence_start + 1200]
    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_final_assistant_does_not_flash_byte_summary",
                "expected": True,
                "actual": no_assistant_byte_summary,
                "passed": no_assistant_byte_summary,
                "evidence": evidence,
            },
            {
                "name": "terminal_final_assistant_preserves_transcript_commit",
                "expected": True,
                "actual": transcript_still_committed,
                "passed": transcript_still_committed,
                "evidence": evidence,
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "assistant_byte_summary_visible": not no_assistant_byte_summary,
            "first_token_byte_counter_visible": not no_first_token_byte_counter,
            "transcript_still_committed": transcript_still_committed,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
