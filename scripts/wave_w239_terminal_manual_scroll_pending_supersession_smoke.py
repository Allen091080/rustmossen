#!/usr/bin/env python3
"""W239 - manual-scroll held widget patches supersede stale pending draws."""

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
        '"manualScrollPendingPolicy"',
        '"replace_pending_with_latest"',
        '"bypass_pending_hold"',
        "draw_runtime_replaces_manual_scroll_held_widget_patch_with_latest_sequence",
        "dropped_pending_count",
        "manual_scroll_preserved",
    ]:
        require(terminal, token, "terminal pending supersession contract", failures)

    for token in [
        "terminal_manual_scroll_pending_supersession",
        "terminal_widget_patch_pending_supersession_policy",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w239_terminal_manual_scroll_pending_supersession_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render manual-scroll pending supersession",
        "phase note",
        failures,
    )

    if failures:
        print("=== W239 terminal manual-scroll pending supersession smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w239_terminal_manual_scroll_pending_supersession_smoke: ok")


if __name__ == "__main__":
    main()
