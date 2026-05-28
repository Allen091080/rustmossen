#!/usr/bin/env python3
from __future__ import annotations

import json
import re
from pathlib import Path

from wave_w279_terminal_slow_first_token_heartbeat_pty_smoke import (
    HEAD_MARKER,
    run_slow_first_token_heartbeat_smoke,
)

ROW_WRITE_RE = re.compile(r"\x1b\[(\d+);1H\x1b\[2K([^\x1b]*)")


def main() -> int:
    result = run_slow_first_token_heartbeat_smoke()
    pty_output = Path(result["artifacts"]["pty_output"])
    assertions_path = Path(result["artifacts"]["assertions"])
    text = pty_output.read_text(encoding="utf-8", errors="replace")

    content_rows: list[int] = []
    for row, line in ROW_WRITE_RE.findall(text):
        if HEAD_MARKER in line or "terminal-heartbeat-row-" in line:
            content_rows.append(int(row))

    first_content_row = content_rows[0] if content_rows else None
    stable_first_frame = first_content_row == 20
    no_growth_rows = bool(content_rows) and min(content_rows) == 20 and max(content_rows) == 23
    ok = bool(result["ok"] and stable_first_frame and no_growth_rows)

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_assistant_activity_first_content_starts_at_reserved_top_row",
                "expected": 20,
                "actual": first_content_row,
                "passed": stable_first_frame,
                "evidence": content_rows[:16],
            },
            {
                "name": "terminal_assistant_activity_rows_do_not_grow_during_stream",
                "expected": {"min": 20, "max": 23},
                "actual": {
                    "min": min(content_rows) if content_rows else None,
                    "max": max(content_rows) if content_rows else None,
                },
                "passed": no_growth_rows,
                "evidence": content_rows[:24],
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "assistant_activity_first_content_row": first_content_row,
            "assistant_activity_unique_content_rows": sorted(set(content_rows)),
            "assistant_activity_row_growth_detected": not no_growth_rows,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
