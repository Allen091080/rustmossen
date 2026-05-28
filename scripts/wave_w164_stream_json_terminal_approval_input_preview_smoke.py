#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
RENDER_EVENTS = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    repl = REPL.read_text()
    render_events = RENDER_EVENTS.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "pending_permission_context",
        "emit_terminal_permission_request_items(&tool_name, &input)",
    ):
        require(repl, token, f"terminal approval input context token {token}")

    for token in (
        "STREAM_JSON_RENDER_APPROVAL_PREVIEW_MAX_LINES",
        "mark_terminal_permission_request_context",
        "terminal_permission_preview_lines",
        "terminal_permission_preview_value",
        "terminal_bash_permission_preview_lines",
        "terminal_approval_input_preview_value",
        '"inputPreview"',
        '"inputPreviewLines"',
        "terminal_permission_request_preview_shows_bounded_input_context",
    ):
        require(render_events, token, f"approval input preview token {token}")

    for token in (
        '"terminal_approval_input_preview"',
        '"terminal_approval_bounded_input_preview"',
    ):
        require(structured_io, token, f"status approval input preview metadata {token}")

    require(
        run_all,
        "wave_w164_stream_json_terminal_approval_input_preview_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval input preview",
        "phase note",
    )

    print("wave_w164_stream_json_terminal_approval_input_preview_smoke: ok")


if __name__ == "__main__":
    main()
