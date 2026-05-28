#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDER_EVENTS = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    render_events = RENDER_EVENTS.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "terminal_command_preview_line_items",
        "terminal_diff_file_preview_lines",
        "terminal_diff_preview_lines",
        '"previewLineItems"',
        '"filePreviewLines"',
        '"diffPreviewLines"',
        '"omittedFileCount"',
        "terminal_frame_includes_command_preview_without_log_wall",
        "terminal_frame_includes_diff_file_preview_while_collapsed",
    ):
        require(render_events, token, f"command/diff preview token {token}")

    for token in (
        '"terminal_command_preview_lines"',
        '"terminal_command_log_collapse_metadata"',
        '"terminal_diff_file_summary_preview"',
        '"terminal_diff_hunk_preview"',
        '"terminal_diff_collapsed_preview"',
    ):
        require(structured_io, token, f"status preview metadata {token}")

    require(
        run_all,
        "wave_w157_stream_json_terminal_command_diff_preview_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal command and diff previews",
        "phase note",
    )

    print("wave_w157_stream_json_terminal_command_diff_preview_smoke: ok")


if __name__ == "__main__":
    main()
