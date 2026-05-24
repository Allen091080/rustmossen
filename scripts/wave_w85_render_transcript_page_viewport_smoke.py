#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    run_all = RUN_ALL.read_text()

    require(app, "fn transcript_page_scroll_rows(&self) -> usize", "transcript page helper")
    require(
        app,
        "self.scroll.scroll_up(self.transcript_page_scroll_rows())",
        "PageUp viewport-sized transcript scroll",
    )
    require(
        app,
        "self.scroll.scroll_down(self.transcript_page_scroll_rows())",
        "PageDown viewport-sized transcript scroll",
    )
    require(
        app,
        "transcript_page_keys_use_rendered_message_viewport_height",
        "transcript page key regression",
    )
    require(
        app,
        "not a fixed 10 rows",
        "fixed-scroll regression assertion",
    )
    require(
        run_all,
        "wave_w85_render_transcript_page_viewport_smoke.py",
        "run_all registration",
    )

    print("wave_w85_render_transcript_page_viewport_smoke: ok")


if __name__ == "__main__":
    main()
