#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w295_terminal_manual_scroll_approval_approve_pty_smoke import (
    COMMAND_STARTED_SENTINEL_PATH,
    SENTINEL_PATH,
    run_manual_scroll_approval_approve_smoke,
)


def main() -> int:
    live_tail_command = (
        ": 'TERMINAL_APPROVAL_APPROVE_COMMAND_%s'; "
        f"touch {COMMAND_STARTED_SENTINEL_PATH}; "
        "sleep 0.45; "
        "printf 'TERMINAL_APPROVAL_APPROVE_COMMAND_%s\\n' W295; "
        "for idx in $(seq 0 79); do "
        "printf 'TERMINAL_APPROVAL_COMMAND_RELEASE_W314_%03d\\n' \"$idx\"; "
        "done; "
        f"touch {SENTINEL_PATH}; "
        "sleep 0.2"
    )
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_FIXTURE_NAME"] = (
        "W314_terminal_manual_scroll_command_live_tail_release_after_approval_pty_smoke"
    )
    os.environ["MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND"] = live_tail_command
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_SCROLL_DURING_COMMAND_AFTER_APPROVE"
    ] = "1"
    os.environ[
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_DURING_COMMAND_AFTER_APPROVE"
    ] = "1"
    os.environ.setdefault(
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_SCROLL_DELAY_SECS",
        "0.08",
    )
    os.environ.setdefault(
        "MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_COMMAND_RELEASE_DELAY_SECS",
        "0.14",
    )
    result = run_manual_scroll_approval_approve_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
