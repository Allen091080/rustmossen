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
    heartbeat_visible = "waiting for model stream" in pre_head
    no_empty_activity = "No active render activity" not in pre_head
    model_metadata_visible = "terminal-heartb" in pre_head
    ok = bool(result["ok"] and heartbeat_visible and no_empty_activity and model_metadata_visible)

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_heartbeat_survives_metadata_before_first_token",
                "expected": True,
                "actual": heartbeat_visible,
                "passed": heartbeat_visible,
                "evidence": pre_head[-500:],
            },
            {
                "name": "terminal_metadata_does_not_render_empty_activity_before_first_token",
                "expected": True,
                "actual": no_empty_activity,
                "passed": no_empty_activity,
                "evidence": pre_head[-500:],
            },
            {
                "name": "terminal_metadata_status_update_was_seen_before_first_token",
                "expected": True,
                "actual": model_metadata_visible,
                "passed": model_metadata_visible,
                "evidence": pre_head[-500:],
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "heartbeat_visible_before_head": heartbeat_visible,
            "empty_activity_before_head": not no_empty_activity,
            "model_metadata_visible_before_head": model_metadata_visible,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
