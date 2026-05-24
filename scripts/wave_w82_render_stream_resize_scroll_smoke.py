#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDER_CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    render_contract = RENDER_CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(
        render_contract,
        "app_render_contract_streaming_resize_mouse_scroll_stays_usable",
        "streaming resize mouse-scroll contract",
    )
    require(
        render_contract,
        "MouseEventKind::Down(MouseButton::Left)",
        "mouse scrollbar top click",
    )
    require(
        render_contract,
        "MouseEventKind::Drag(MouseButton::Left)",
        "mouse scrollbar drag after resize",
    )
    require(
        render_contract,
        "let resized_manual = render_app(&mut app, 72, 16);",
        "resize render pass while manually scrolled",
    )
    require(
        render_contract,
        "W82-tail-after-drag",
        "post-resize appended stream tail anchor",
    )
    require(
        render_contract,
        "resize plus appended streaming text must not steal manual scroll",
        "manual scroll preservation assertion",
    )
    require(
        run_all,
        "wave_w82_render_stream_resize_scroll_smoke.py",
        "run_all registration",
    )

    print("wave_w82_render_stream_resize_scroll_smoke: ok")


if __name__ == "__main__":
    main()
