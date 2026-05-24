#!/usr/bin/env python3
"""W184 - terminal renderer supports bounded error detail expansion."""

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
        "ToggleErrorExpansion",
        "toggle_error_widget_expanded",
        "terminal_error_update_mode",
        '"replace_error_details"',
        '"errorToggleKey"',
        '"x expand error"',
        "terminal_error_widget_toggle_expands_bounded_details",
    ]:
        require(render, token, "error detail expansion model", failures)

    for token in [
        "TerminalRenderFrontendEvent::ToggleErrorExpansion",
        "KeyCode::Char('x')",
        "maps_widget_toggle_keys_to_frontend_events",
    ]:
        require(repl, token, "frontend error key bridge", failures)

    for token in [
        "terminal_error_expansion_controls",
        "terminal_error_detail_preview",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w184_terminal_error_detail_expansion_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render error detail expansion",
        "phase note",
        failures,
    )

    if failures:
        print("=== W184 terminal error detail expansion smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w184_terminal_error_detail_expansion_smoke: ok")


if __name__ == "__main__":
    main()
