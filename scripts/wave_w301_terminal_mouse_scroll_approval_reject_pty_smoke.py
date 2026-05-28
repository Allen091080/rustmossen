#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w294_terminal_manual_scroll_approval_reject_pty_smoke import (
    run_manual_scroll_approval_reject_smoke,
)


def main() -> int:
    os.environ["MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_FIXTURE_NAME"] = (
        "W301_terminal_mouse_scroll_approval_reject_pty_smoke"
    )
    os.environ["MOSSEN_TERMINAL_APPROVAL_REJECT_PTY_MOUSE_SCROLL_AFTER_APPROVAL"] = "1"
    result = run_manual_scroll_approval_reject_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
