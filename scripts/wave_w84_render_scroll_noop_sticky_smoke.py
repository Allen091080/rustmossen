#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LAYOUT = ROOT / "crates/mossen-tui/src/layout.rs"
APP = ROOT / "crates/mossen-tui/src/app.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    layout = LAYOUT.read_text()
    app = APP.read_text()
    run_all = RUN_ALL.read_text()

    require(layout, "fn max_scroll_offset(&self) -> usize", "scroll range helper")
    require(
        layout,
        "non_scrollable_scroll_up_preserves_sticky_bottom",
        "layout non-scrollable sticky regression",
    )
    require(
        layout,
        "sticky_scroll_up_uses_live_bottom_offset",
        "sticky live-bottom offset regression",
    )
    require(
        layout,
        "if n == 0 || max_offset == 0",
        "no-op scroll guard",
    )
    require(
        app,
        "non_scrollable_transcript_wheel_preserves_sticky_without_dirty_frame",
        "app non-scrollable wheel regression",
    )
    require(
        app,
        "wheel input on a short transcript must keep future output anchored to the live tail",
        "sticky preservation assertion",
    )
    require(
        run_all,
        "wave_w84_render_scroll_noop_sticky_smoke.py",
        "run_all registration",
    )

    print("wave_w84_render_scroll_noop_sticky_smoke: ok")


if __name__ == "__main__":
    main()
