#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    renderer = RENDERER.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "pub struct StreamJsonTerminalDrawExecutor",
        "pub struct StreamJsonTerminalViewport",
        "pub struct StreamJsonTerminalDrawExecutionReport",
        "pub fn apply_draw_plan",
        "render_draw_plan_to_terminal_bytes",
        "terminal::BeginSynchronizedUpdate",
        "terminal::EndSynchronizedUpdate",
        "cursor::SavePosition",
        "cursor::RestorePosition",
        "cursor::MoveTo(0, row)",
        "terminal::Clear(ClearType::CurrentLine)",
        "terminal_draw_bounded_line",
        "resolve_terminal_draw_row",
        "draw_executor_writes_synchronized_region_patch_without_full_clear",
        "draw_executor_bounds_lines_to_viewport_width_without_newlines",
        "draw_executor_drops_superseded_sequence_on_reused_executor",
    ):
        require(renderer, token, f"draw executor token {token}")

    for token in (
        '"draw_executor": true',
        '"draw_executor_backend": "crossterm"',
        '"synchronized_update": true',
        '"absolute_row_moves": true',
        '"line_wrap_guard": true',
        '"no_newline_writes": true',
    ):
        require(structured_io, token, f"status draw executor metadata {token}")

    require(
        run_all,
        "wave_w141_stream_json_terminal_draw_executor_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal draw executor",
        "phase note",
    )

    print("wave_w141_stream_json_terminal_draw_executor_smoke: ok")


if __name__ == "__main__":
    main()
