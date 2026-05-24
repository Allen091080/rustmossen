#!/usr/bin/env python3
"""W188 - terminal renderer keeps native mouse scroll by default."""

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
        "TERMINAL_RENDER_CAPTURE_MOUSE_ENV",
        "MOSSEN_TERMINAL_RENDER_CAPTURE_MOUSE",
        "terminal_render_should_capture_mouse",
        "if terminal_render_should_capture_mouse()",
        "terminal_render_mouse_capture_defaults_off_for_native_scroll",
        "terminal_render_mouse_capture_can_be_enabled_by_env",
        "EnableMouseCapture",
        "DisableMouseCapture",
    ]:
        require(repl, token, "native mouse scroll input policy", failures)

    for token in [
        "terminal_frontend_native_mouse_scroll",
        "terminal_frontend_mouse_capture_opt_in",
        "terminal_frontend_mouse_capture_default_off",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w188_terminal_native_mouse_scroll_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render native mouse scroll default",
        "phase note",
        failures,
    )

    if failures:
        print("=== W188 terminal native mouse scroll smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w188_terminal_native_mouse_scroll_smoke: ok")


if __name__ == "__main__":
    main()
