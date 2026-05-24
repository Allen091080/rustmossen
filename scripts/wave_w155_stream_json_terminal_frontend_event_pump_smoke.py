#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    repl = REPL.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "terminal_render_spawn_frontend_event_pump",
        "TerminalRenderFrontendEvent",
        "terminal_render_frontend_event_from_crossterm",
        "terminal_render_handle_frontend_event",
        "set_manual_scroll_active(true)",
        "set_manual_scroll_active(false)",
        "Event::Resize",
        "MouseEventKind::ScrollUp",
        "KeyCode::PageUp",
        "KeyCode::End",
        "terminal_render_frontend_event_tests",
    ):
        require(repl, token, f"terminal frontend event pump token {token}")

    for token in (
        '"terminal_frontend_event_pump"',
        '"terminal_frontend_resize_events"',
        '"terminal_frontend_manual_scroll_controls"',
    ):
        require(structured_io, token, f"status terminal frontend event token {token}")

    require(
        run_all,
        "wave_w155_stream_json_terminal_frontend_event_pump_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal frontend event pump",
        "phase note",
    )

    print("wave_w155_stream_json_terminal_frontend_event_pump_smoke: ok")


if __name__ == "__main__":
    main()
