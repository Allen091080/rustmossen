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

    require(app, "let transcript_page_key = !self.active_modal.is_open()", "page-key guard")
    require(
        app,
        "before_scroll != (self.scroll.offset, self.scroll.sticky)",
        "dirty only when page key changes scroll",
    )
    require(
        app,
        "non_scrollable_transcript_page_key_preserves_sticky_without_dirty_frame",
        "non-scrollable page key regression",
    )
    require(
        app,
        "no-op transcript PageUp should not schedule a redraw",
        "PageUp no-op dirty assertion",
    )
    require(
        app,
        "no-op transcript PageDown should not schedule a redraw",
        "PageDown no-op dirty assertion",
    )
    require(
        run_all,
        "wave_w87_render_page_key_noop_dirty_smoke.py",
        "run_all registration",
    )

    print("wave_w87_render_page_key_noop_dirty_smoke: ok")


if __name__ == "__main__":
    main()
