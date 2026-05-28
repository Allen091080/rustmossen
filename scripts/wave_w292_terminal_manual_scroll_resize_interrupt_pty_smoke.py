#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from wave_w277_terminal_interrupt_diagnostics_pty_smoke import run_terminal_interrupt_diagnostics_smoke


def main() -> int:
    os.environ["MOSSEN_TERMINAL_INTERRUPT_DIAG_PTY_FIXTURE_NAME"] = (
        "W292_terminal_manual_scroll_resize_interrupt_pty_smoke"
    )
    os.environ["MOSSEN_TERMINAL_INTERRUPT_DIAG_PTY_RESIZE_DURING_HOLD"] = "1"
    os.environ.setdefault("MOSSEN_TERMINAL_INTERRUPT_DIAG_PTY_FINAL_COLS", "118")
    result = run_terminal_interrupt_diagnostics_smoke()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
