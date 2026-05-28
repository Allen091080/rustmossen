#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
LAYOUT = ROOT / "crates/mossen-tui/src/layout.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    layout = LAYOUT.read_text()
    run_all = RUN_ALL.read_text()

    require(layout, "pub fn set_viewport_height", "viewport resize hook")
    require(
        layout,
        "self.offset = self\n                .offset\n                .min(self.total_items.saturating_sub(self.visible_count));",
        "manual scroll clamp on viewport resize",
    )
    require(
        layout,
        "viewport_resize_clamps_manual_scroll_without_restoring_sticky",
        "layout resize clamp regression test",
    )
    require(
        app,
        "resize_event_clamps_manual_scroll_without_restoring_sticky",
        "app resize event regression test",
    )
    require(
        run_all,
        "wave_w81_render_scroll_resize_smoke.py",
        "run_all registration",
    )

    print("wave_w81_render_scroll_resize_smoke: ok")


if __name__ == "__main__":
    main()
