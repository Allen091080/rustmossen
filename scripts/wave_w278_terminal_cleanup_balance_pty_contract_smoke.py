#!/usr/bin/env python3
"""W278 - PTY terminal cleanup balance is asserted across external render paths."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def require_script(script_name: str, failures: list[str]) -> None:
    script = (ROOT / "scripts" / script_name).read_text()
    for token in [
        "bracketed_enters = text.count(\"\\x1b[?2004h\")",
        "bracketed_leaves = text.count(\"\\x1b[?2004l\")",
        "bracketed_paste_balanced",
        "sync_update_balanced",
        "alt_screen_balanced",
        "no_repeated_fullscreen_clear",
    ]:
        require(script, token, script_name, failures)


def main() -> None:
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()

    failures: list[str] = []
    for script_name in [
        "wave_w274_terminal_oneshot_diagnostics_pty_smoke.py",
        "wave_w275_terminal_oneshot_manual_scroll_diagnostics_pty_smoke.py",
        "wave_w276_terminal_oneshot_resize_scroll_diagnostics_pty_smoke.py",
        "wave_w277_terminal_interrupt_diagnostics_pty_smoke.py",
        "wave_w279_terminal_slow_first_token_heartbeat_pty_smoke.py",
        "wave_w280_terminal_slow_first_token_interrupt_pty_smoke.py",
    ]:
        require_script(script_name, failures)

    for token in [
        "terminal_render_cleanup_balance_pty_contract",
        "terminal_render_external_process_completion_cleanup_balanced",
        "terminal_render_external_process_scroll_resize_cleanup_balanced",
        "terminal_render_external_process_interrupt_cleanup_balanced",
        "terminal_render_slow_first_token_interrupt_cleanup_balanced",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w278_terminal_cleanup_balance_pty_contract_smoke",
        "run_all registration",
        failures,
    )
    if failures:
        print("=== W278 terminal cleanup balance PTY contract smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w278_terminal_cleanup_balance_pty_contract_smoke: ok")


if __name__ == "__main__":
    main()
