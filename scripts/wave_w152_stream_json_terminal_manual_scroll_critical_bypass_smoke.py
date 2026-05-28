#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str) -> None:
    if token not in text:
        raise AssertionError(f"missing {label}: {token}")


def main() -> int:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured_io = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()

    for token in (
        "draw_plan_requires_manual_scroll_bypass",
        '"approval" | "error"',
        '"hold_noncritical_scrollback_commit"',
        '"hold_noncritical_completion_update"',
        "clear_retired",
        "draw_runtime_bypasses_manual_scroll_hold_for_blocking_approval",
        "draw_runtime_holds_noncritical_scrollback_commit_while_manual_scroll_is_active",
        "draw_runtime_holds_final_summary_completion_update_while_manual_scroll_is_active",
    ):
        require(renderer, token, f"manual-scroll critical bypass token {token}")

    for token in (
        '"draw_runtime_manual_scroll_critical_bypass": true',
        '"draw_runtime_noncritical_scrollback_hold".to_string()',
        '"terminal_completion_manual_scroll_hold".to_string()',
        '"manual_scroll_critical_draw_bypass"',
        '"terminal_noncritical_scrollback_manual_scroll_hold"',
        '"terminal_render_noncritical_scrollback_manual_scroll_hold"',
    ):
        require(structured_io, token, f"status manual-scroll bypass metadata {token}")

    require(
        run_all,
        "wave_w152_stream_json_terminal_manual_scroll_critical_bypass_smoke",
        "run_all registration",
    )
    print("wave_w152_stream_json_terminal_manual_scroll_critical_bypass_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
