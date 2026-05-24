#!/usr/bin/env python3
"""W238 - frame-derived widget patches carry manual-scroll policy."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "render_patch_scroll_contract",
        "render_patch_operation_requires_manual_scroll_bypass",
        "render_patch_operation_is_noncritical_widget_update",
        '"hold_noncritical_widget_region_update"',
        '"bypass_for_critical_region_update"',
        '"update_widget_region"',
        "frame_patch_holds_noncritical_widget_region_update_while_manual_scroll_active",
        "frame_patch_bypasses_manual_scroll_hold_for_critical_error_region_update",
    ]:
        require(terminal, token, "terminal widget patch manual-scroll policy", failures)

    for token in [
        "terminal_noncritical_widget_manual_scroll_hold",
        "terminal_widget_patch_manual_scroll_policy",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w238_terminal_widget_patch_manual_scroll_policy_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render widget patch manual-scroll policy",
        "phase note",
        failures,
    )

    if failures:
        print("=== W238 terminal widget patch manual-scroll policy smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w238_terminal_widget_patch_manual_scroll_policy_smoke: ok")


if __name__ == "__main__":
    main()
