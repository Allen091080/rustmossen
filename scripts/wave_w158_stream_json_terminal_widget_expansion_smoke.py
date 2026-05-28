#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDER_EVENTS = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    render_events = RENDER_EVENTS.read_text()
    repl = REPL.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "StreamJsonTerminalWidgetControl",
        "emit_terminal_widget_control_items",
        "toggle_command_widget_expanded",
        "toggle_diff_widget_expanded",
        '"expandedPreviewLineItems"',
        '"expandedFilePreviewLines"',
        '"expandedDiffPreviewLines"',
        "replace_expanded_preview",
        "terminal_command_widget_toggle_expands_bounded_preview_lines",
        "terminal_diff_widget_toggle_expands_bounded_file_and_hunk_preview",
    ):
        require(render_events, token, f"widget expansion token {token}")

    for token in (
        "ToggleCommandExpansion",
        "ToggleDiffExpansion",
        "StreamJsonTerminalWidgetControl::ToggleCommandExpansion",
        "StreamJsonTerminalWidgetControl::ToggleDiffExpansion",
        "maps_widget_toggle_keys_to_frontend_events",
    ):
        require(repl, token, f"frontend expansion token {token}")

    for token in (
        '"terminal_widget_expand_controls"',
        '"terminal_command_expand_collapse"',
        '"terminal_diff_expand_collapse"',
        '"terminal_expanded_preview_budgets"',
        '"terminal_expansion_immediate_redraw"',
    ):
        require(structured_io, token, f"status expansion metadata {token}")

    require(
        run_all,
        "wave_w158_stream_json_terminal_widget_expansion_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal widget expansion controls",
        "phase note",
    )

    print("wave_w158_stream_json_terminal_widget_expansion_smoke: ok")


if __name__ == "__main__":
    main()
