#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    require(
        app,
        "fn autosave_render_session_snapshot_best_effort(&mut self)",
        "best-effort render snapshot autosave helper",
    )
    require(
        app,
        "async fn run_event_loop_with_bus(",
        "testable render event-loop boundary",
    )
    require(
        app,
        "terminal draw failed after render snapshot autosave attempt",
        "draw-error recovery error context",
    )
    require(
        app,
        "struct FailingDrawBackend",
        "draw-failure backend regression fixture",
    )
    require(
        app,
        "run_autosaves_render_snapshot_when_terminal_draw_fails",
        "draw-failure autosave regression",
    )
    require(
        run_all,
        "wave_w103_render_loop_draw_error_autosave_smoke.py",
        "run_all registration",
    )
    require(phase, "render-loop draw-error snapshot recovery", "phase record")

    print("wave_w103_render_loop_draw_error_autosave_smoke: ok")


if __name__ == "__main__":
    main()
