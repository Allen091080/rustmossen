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
    pre_head = text[:head_pos] if head_pos >= 0 else text
    waiting_line_count = pre_head.count("waiting for model stream")
    no_redundant_waiting_redraw = waiting_line_count == 1
    heartbeat_replace_active_visible = "Thinking 0s" in pre_head and waiting_line_count == 1
    ok = bool(result["ok"] and no_redundant_waiting_redraw and heartbeat_replace_active_visible)

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_metadata_does_not_rewrite_waiting_activity",
                "expected": 1,
                "actual": waiting_line_count,
                "passed": no_redundant_waiting_redraw,
                "evidence": pre_head[:700],
            },
            {
                "name": "terminal_heartbeat_replace_active_initial_status_visible",
                "expected": True,
                "actual": heartbeat_replace_active_visible,
                "passed": heartbeat_replace_active_visible,
                "evidence": pre_head[:700],
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "waiting_activity_line_count_before_head": waiting_line_count,
            "metadata_redundant_waiting_redraw": not no_redundant_waiting_redraw,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
