#!/usr/bin/env python3
"""W170 - terminal-render raw/mouse input capture smoke."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "TerminalRenderInputCaptureGuard",
        "terminal_render_enable_input_capture",
        "mossen_utils::early_input::stop_capturing_early_input()",
        "enable_raw_mode()",
        "EnableMouseCapture",
        "DisableMouseCapture",
        "terminal_input_capture_guard",
        "terminal_event_pump_guard = if terminal_input_capture_guard.is_some()",
        "let mut permission_requests_open = terminal_events_open",
        "drop(terminal_event_pump_guard);",
        "drop(terminal_input_capture_guard);",
    ]:
        require(repl, token, "terminal-render input capture", failures)

    for token in [
        "terminal_frontend_raw_mode_capture",
        "terminal_frontend_mouse_capture",
        "terminal_frontend_early_input_isolation",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w170_terminal_render_input_capture_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render input capture lifecycle",
        "phase note",
        failures,
    )

    if failures:
        print("=== W170 terminal-render input capture smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w170_terminal_render_input_capture_smoke: ok")


if __name__ == "__main__":
    main()
