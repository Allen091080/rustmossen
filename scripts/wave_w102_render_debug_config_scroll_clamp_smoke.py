#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
KEYBINDING = ROOT / "crates/mossen-tui/tests/keybinding_smoke.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    keybinding = KEYBINDING.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    require(
        app,
        "fn scroll_max(&self, viewport_rows: usize) -> usize",
        "debug-config viewport-aware scroll max",
    )
    require(
        app,
        "fn visible_scroll(&self, viewport_rows: usize) -> usize",
        "debug-config render-time stale scroll clamp",
    )
    require(
        app,
        "let visible_scroll = state.visible_scroll(viewport_rows);",
        "debug-config dialog uses clamped visible scroll",
    )
    require(
        app,
        "debug_config_scroll_clamps_to_rendered_viewport",
        "debug-config scroll clamp unit regression",
    )
    require(
        keybinding,
        "End should clamp to the last rendered debug-config viewport",
        "debug-config keybinding bottom clamp regression",
    )
    require(
        keybinding,
        "PageDown at the bottom should not overscroll debug-config",
        "debug-config no overscroll keybinding regression",
    )
    require(
        run_all,
        "wave_w102_render_debug_config_scroll_clamp_smoke.py",
        "run_all registration",
    )
    require(phase, "debug-config viewport scroll clamp", "phase record")

    print("wave_w102_render_debug_config_scroll_clamp_smoke: ok")


if __name__ == "__main__":
    main()
