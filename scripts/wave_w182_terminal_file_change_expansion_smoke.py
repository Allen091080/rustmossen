#!/usr/bin/env python3
"""W182 - terminal renderer supports file-change expand/collapse controls."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "ToggleFileChangeExpansion",
        "toggle_file_change_widget_expanded",
        "terminal_file_change_update_mode",
        '"replace_expanded_file_summary"',
        '"fileChangeToggleKey"',
        '"f expand files"',
        "terminal_file_change_widget_toggle_expands_bounded_file_preview",
    ]:
        require(render, token, "file-change expansion model", failures)

    for token in [
        "TerminalRenderFrontendEvent::ToggleFileChangeExpansion",
        "KeyCode::Char('f')",
        "maps_widget_toggle_keys_to_frontend_events",
    ]:
        require(repl, token, "frontend file-change key bridge", failures)

    for token in [
        "terminal_file_change_expansion_controls",
        "terminal_file_change_expand_collapse",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w182_terminal_file_change_expansion_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render file-change expansion controls",
        "phase note",
        failures,
    )

    if failures:
        print("=== W182 terminal file-change expansion smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w182_terminal_file_change_expansion_smoke: ok")


if __name__ == "__main__":
    main()
