#!/usr/bin/env python3
"""W237 - slash result event patches preserve manual scroll until release."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        '"preserveDuringManualScroll"',
        '"manualScrollPolicy"',
        '"hold_noncritical_top_region_update"',
        '"bypass_for_lifecycle_clear"',
        '"historyPolicy": "update_top_region"',
        '"historyPolicy": "clear_retired_region"',
    ]:
        require(bridge, token, "slash result event patch scroll contract", failures)

    for token in [
        "draw_runtime_holds_noncritical_top_region_patch_while_manual_scroll_is_active",
        "draw_runtime_bypasses_manual_scroll_hold_for_explicit_scroll_bypass",
        '"update_top_region" | "update_widget_region"',
        '"manualScrollBypass"',
    ]:
        require(terminal, token, "terminal runtime manual-scroll contract", failures)

    for token in [
        "slash_result_event_patch_manual_scroll_hold",
        "terminal_slash_result_event_patch_manual_scroll_hold",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w237_stream_json_slash_result_patch_manual_scroll_hold_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result patch manual-scroll hold",
        "phase note",
        failures,
    )

    if failures:
        print("=== W237 stream-json slash result patch manual-scroll hold smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w237_stream_json_slash_result_patch_manual_scroll_hold_smoke: ok")


if __name__ == "__main__":
    main()
