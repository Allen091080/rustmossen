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
    seeded_model_visible = "terminal-heartb" in pre_head
    model_slot_not_unknown = " | unknown | mode:" not in pre_head
    ok = bool(result["ok"] and seeded_model_visible and model_slot_not_unknown)

    payload = json.loads(assertions_path.read_text(encoding="utf-8"))
    payload["assertions"].extend(
        [
            {
                "name": "terminal_initial_status_uses_seeded_model",
                "expected": True,
                "actual": seeded_model_visible,
                "passed": seeded_model_visible,
                "evidence": pre_head[:500],
            },
            {
                "name": "terminal_initial_status_model_slot_not_unknown",
                "expected": True,
                "actual": model_slot_not_unknown,
                "passed": model_slot_not_unknown,
                "evidence": pre_head[:500],
            },
        ]
    )
    payload["status"] = "passed" if ok else "failed"
    assertions_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))

    result.update(
        {
            "ok": ok,
            "seeded_model_visible_before_head": seeded_model_visible,
            "model_slot_unknown_before_head": not model_slot_not_unknown,
        }
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
