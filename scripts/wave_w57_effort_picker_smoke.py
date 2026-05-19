#!/usr/bin/env python3
"""W57 C1 — /effort interactive picker smoke.

Verifies the no-args /effort path opens an EffortPicker UI without
breaking the existing surface:
  - /effort current   → still shows current level (legacy 'show' behaviour)
  - /effort status    → same as current (alias)
  - /effort low|medium|high|max|auto|unset → still applies directly (no picker)
  - /effort           → NOW returns <EffortPicker .../> (was: showCurrentEffort)
  - /effort help|-h|--help → still prints usage

Static-only check; no terminal interaction required.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EFFORT_TSX = ROOT / "commands" / "effort" / "effort.tsx"
PICKER_TSX = ROOT / "commands" / "effort" / "EffortPicker.tsx"


def fail(msg: str) -> None:
    print(f"[FAIL] {msg}", file=sys.stderr)
    sys.exit(1)


def info(msg: str) -> None:
    print(msg)


def assert_picker_file_exists() -> str:
    if not PICKER_TSX.is_file():
        fail(f"missing file: {PICKER_TSX}")
    text = PICKER_TSX.read_text(encoding="utf-8")
    info(f"  EffortPicker.tsx: {len(text.splitlines())} lines")
    return text


def assert_picker_uses_dialog_and_select(text: str) -> None:
    if "from '../../components/design-system/Dialog.js'" not in text:
        fail("EffortPicker not built on design-system Dialog wrapper")
    if "from '../../components/CustomSelect/select.js'" not in text:
        fail("EffortPicker not using CustomSelect/select component")
    info("  picker chrome: Dialog + CustomSelect (matches W56 patterns)")


def assert_picker_options_complete(text: str) -> None:
    m = re.search(r"PICKER_ORDER[^=]*=\s*\[([^\]]+)\]", text)
    if not m:
        fail("PICKER_ORDER constant not found")
    items = re.findall(r"'([a-z]+)'", m.group(1))
    expected = ["auto", "low", "medium", "high", "max"]
    if items != expected:
        fail(f"PICKER_ORDER drift — got {items}, expected {expected}")
    info(f"  picker order: {items}")


def assert_max_disabled_when_unsupported(text: str) -> None:
    if "modelSupportsMaxEffort" not in text:
        fail("max-only-Opus-4.6 gate (modelSupportsMaxEffort) missing in picker")
    if "disabled" not in text:
        fail("max option is not visually disabled when unsupported")
    info("  max option: gated by modelSupportsMaxEffort + disabled flag")


def assert_picker_reuses_executeEffort(text: str) -> None:
    if "from './effort.js'" not in text or "executeEffort" not in text:
        fail("picker does not reuse executeEffort — risks duplicating write surface")
    info("  picker reuses executeEffort (single source of truth for write)")


def assert_router_dispatches_to_picker() -> None:
    text = EFFORT_TSX.read_text(encoding="utf-8")
    if "import { EffortPicker } from './EffortPicker.js'" not in text:
        fail("effort.tsx does not import EffortPicker")
    if "<EffortPicker" not in text:
        fail("effort.tsx does not render <EffortPicker .../>")
    # The legacy 'current' / 'status' surface must still work.
    if "args === 'current'" not in text or "args === 'status'" not in text:
        fail("effort.tsx no longer exposes /effort current / /effort status")
    # Help must still work (regression-prevention).
    if "COMMON_HELP_ARGS" not in text:
        fail("effort.tsx no longer exposes help — UX regression")
    info("  effort.tsx router: picker on no-args, legacy paths preserved")


def assert_no_args_no_longer_shows_current() -> None:
    """The behaviour change: empty-args used to fall through to
    ShowCurrentEffort. It must now route to EffortPicker. We assert by
    finding the no-args branch and confirming it returns <EffortPicker."""
    text = EFFORT_TSX.read_text(encoding="utf-8")
    m = re.search(r"if \(!args\) \{\s*return\s+(<\w+)", text)
    if not m:
        fail("could not locate the no-args branch in effort.tsx")
    rendered = m.group(1)
    if rendered != "<EffortPicker":
        fail(f"no-args branch renders {rendered}, expected <EffortPicker — UX regression")
    info("  no-args branch: returns <EffortPicker .../> (was ShowCurrentEffort)")


def assert_cancel_emits_localized_message() -> None:
    text = PICKER_TSX.read_text(encoding="utf-8")
    if "cancelled" not in text.lower() or "已取消" not in text:
        fail("picker cancel path missing en/zh message — i18n regression")
    info("  cancel: localized en+zh message")


def main() -> int:
    info("W57 C1 — /effort interactive picker smoke")
    info("=" * 60)

    text = assert_picker_file_exists()
    assert_picker_uses_dialog_and_select(text)
    assert_picker_options_complete(text)
    assert_max_disabled_when_unsupported(text)
    assert_picker_reuses_executeEffort(text)
    assert_router_dispatches_to_picker()
    assert_no_args_no_longer_shows_current()
    assert_cancel_emits_localized_message()

    info("")
    info("[PASS] W57 C1 — picker present, gated, and wired without breaking legacy")
    return 0


if __name__ == "__main__":
    sys.exit(main())
