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
        "let stream_visible_schedule_before = match &msg",
        "pre-delta render schedule snapshot",
    )
    require(
        app,
        "self.render_throttled_dirty_at = throttled_dirty_at;",
        "no-visible delta schedule restore",
    )
    require(app, "if text.is_empty() {\n                        return false;", "empty text delta guard")
    require(
        app,
        "if m.thinking != thinking || m.content != content",
        "visible transcript change comparison",
    )
    require(
        app,
        "if thinking.is_empty() {\n                        return false;",
        "empty thinking delta guard",
    )
    require(
        app,
        "no_visible_streaming_delta_does_not_schedule_frame",
        "no-visible streaming delta regression",
    )
    require(app, "no-visible parser boundary", "parser boundary assertion")
    require(
        run_all,
        "wave_w101_render_stream_no_visible_dirty_smoke.py",
        "run_all registration",
    )
    require(phase, "no-visible streaming delta dirty guard", "phase record")

    print("wave_w101_render_stream_no_visible_dirty_smoke: ok")


if __name__ == "__main__":
    main()
