#!/usr/bin/env python3
"""W173 - terminal-render edit-command bracketed paste smoke."""

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
        "EnableBracketedPaste",
        "DisableBracketedPaste",
        "bracketed_paste_enabled",
        "Event::Paste(text)",
        "TerminalRenderFrontendEvent::EditCommandPaste",
        "paste_edit_command_text",
        "terminal_render_normalize_pasted_edit_command_text",
        "maps_bracketed_paste_to_edit_command_paste_only_during_edit_capture",
        "terminal_approval_bridge_pastes_normalized_command_text",
    ]:
        require(repl, token, "terminal edit-command paste bridge", failures)

    for token in [
        "terminal_frontend_bracketed_paste_capture",
        "terminal_frontend_edit_command_paste",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w173_terminal_edit_command_paste_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render edit-command bracketed paste",
        "phase note",
        failures,
    )

    if failures:
        print("=== W173 terminal edit-command paste smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w173_terminal_edit_command_paste_smoke: ok")


if __name__ == "__main__":
    main()
