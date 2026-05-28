#!/usr/bin/env python3
"""W320 - external PTY rendering must not allow fullscreen clears."""

from __future__ import annotations

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]

PTY_SCRIPTS = [
    "wave_w104_render_pty_live_streaming_soak.py",
    "wave_w105_render_pty_resize_manual_scroll_soak.py",
    "wave_w106_render_pty_mouse_scroll_soak.py",
    "wave_w274_terminal_oneshot_diagnostics_pty_smoke.py",
    "wave_w275_terminal_oneshot_manual_scroll_diagnostics_pty_smoke.py",
    "wave_w276_terminal_oneshot_resize_scroll_diagnostics_pty_smoke.py",
    "wave_w277_terminal_interrupt_diagnostics_pty_smoke.py",
    "wave_w279_terminal_slow_first_token_heartbeat_pty_smoke.py",
    "wave_w280_terminal_slow_first_token_interrupt_pty_smoke.py",
    "wave_w289_terminal_manual_scroll_tail_hold_pty_smoke.py",
    "wave_w293_terminal_manual_scroll_approval_bypass_pty_smoke.py",
    "wave_w294_terminal_manual_scroll_approval_reject_pty_smoke.py",
    "wave_w295_terminal_manual_scroll_approval_approve_pty_smoke.py",
    "wave_w296_terminal_manual_scroll_approval_edit_command_pty_smoke.py",
    "wave_w297_terminal_manual_scroll_approval_always_allow_pty_smoke.py",
    "wave_w298_terminal_manual_scroll_approval_edit_cancel_pty_smoke.py",
]


def require(condition: bool, label: str, failures: list[str]) -> None:
    if not condition:
        failures.append(label)


def require_text(text: str, token: str, label: str, failures: list[str]) -> None:
    require(token in text, f"{label}: missing {token!r}", failures)


def main() -> int:
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()

    failures: list[str] = []
    for script_name in PTY_SCRIPTS:
        script = (ROOT / "scripts" / script_name).read_text()
        require_text(
            script,
            'text.count("\\x1b[2J") + text.count("\\x1b[3J")',
            script_name,
            failures,
        )
        require_text(
            script,
            "no_repeated_fullscreen_clear",
            script_name,
            failures,
        )
        require_text(
            script,
            "full_clears == 0",
            script_name,
            failures,
        )
        require(
            "full_clears <= 2" not in script,
            f"{script_name}: legacy fullscreen-clear allowance still present",
            failures,
        )

    for token in [
        "terminal_render_external_process_no_fullscreen_clear",
        "terminal_render_external_process_no_fullscreen_clear_pty_contract_smoke",
        "terminal_render_product_external_pty_no_fullscreen_clear_w104_w320",
    ]:
        require_text(structured, token, "status metadata", failures)

    require_text(
        run_all,
        "wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke",
        "run_all registration",
        failures,
    )
    if failures:
        print("=== W320 terminal no-fullscreen-clear external PTY contract smoke ===")
        for failure in failures:
            print(f"- {failure}")
        return 1
    print("wave_w320_terminal_no_fullscreen_clear_external_pty_contract_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
