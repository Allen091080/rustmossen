#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
MESSAGES = ROOT / "crates/mossen-tui/src/widgets/messages.rs"
RENDER_CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    messages = MESSAGES.read_text()
    render_contract = RENDER_CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(app, "message_content_area: Option<Rect>", "last transcript viewport area")
    require(app, "fn ensure_focused_message_visible", "focused message viewport guard")
    require(
        app,
        "MessagesWidget::content_row_range_for_source_index_from_transcript_with_cache_and_glyphs",
        "focused source row range lookup",
    )
    require(
        messages,
        "content_row_range_for_source_index_from_transcript_with_cache_and_glyphs",
        "messages row range helper",
    )
    require(
        render_contract,
        "app_render_contract_keyboard_focus_scroll_owns_viewport",
        "keyboard focus render contract",
    )
    require(
        render_contract,
        "async append must not steal a keyboard-focused history viewport",
        "append preservation assertion",
    )
    require(
        run_all,
        "wave_w83_render_keyboard_focus_scroll_smoke.py",
        "run_all registration",
    )

    print("wave_w83_render_keyboard_focus_scroll_smoke: ok")


if __name__ == "__main__":
    main()
