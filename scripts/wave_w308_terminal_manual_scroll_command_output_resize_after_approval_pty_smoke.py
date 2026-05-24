#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w295_terminal_manual_scroll_approval_approve_pty_smoke import (
    SENTINEL_PATH,
    run_manual_scroll_approval_approve_smoke,
)


def main() -> int:
    slow_command = (
        "sleep 0.35; "
        "printf 'TERMINAL_APPROVAL_APPROVE_COMMAND_%s\\n' W295; "
        "i=0; "
        "while [ \"$i\" -lt 80 ]; do "
        "printf 'TERMINAL_APPROVAL_RESIZE_COMMAND_SCROLL_W308_%03d\\n' \"$i\"; "
        "i=$((i + 1)); "
        "sleep 0.01; "
        "done; "
        f"touch {SENTINEL_PATH}; "
        "sleep 0.35"
    )
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_FIXTURE_NAME"] = (
        "W308_terminal_manual_scroll_command_output_resize_after_approval_pty_smoke"
    )
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND"] = slow_command
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE"] = (
        "1"
    )
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RESIZE_DURING_COMMAND_AFTER_APPROVE"
    ] = "1"
    os.environ.setdefault(
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_SCROLL_DELAY_SECS",
        "0.08",
    )
    os.environ.setdefault(
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_RESIZE_DELAY_SECS",
        "0.08",
    )
    result = run_manual_scroll_approval_approve_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
