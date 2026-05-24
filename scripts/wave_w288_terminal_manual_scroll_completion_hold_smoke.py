#!/usr/bin/env python3
"""W288 - completion scrollback commits do not break manual scroll holds."""

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
        "render_patch_operation_is_noncritical_completion_update",
        '"hold_noncritical_scrollback_commit"',
        '"hold_noncritical_completion_update"',
        "draw_runtime_holds_noncritical_scrollback_commit_while_manual_scroll_is_active",
        "draw_runtime_holds_final_summary_completion_update_while_manual_scroll_is_active",
        '"approval" | "error"',
    ]:
        require(terminal, token, "terminal manual-scroll completion hold", failures)

    for token in [
        '"draw_runtime_noncritical_scrollback_hold".to_string()',
        '"terminal_completion_manual_scroll_hold".to_string()',
        "terminal_noncritical_scrollback_manual_scroll_hold",
        "terminal_render_noncritical_scrollback_manual_scroll_hold",
        "terminal_render_completion_manual_scroll_hold",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w288_terminal_manual_scroll_completion_hold_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render manual-scroll completion hold",
        "phase note",
        failures,
    )

    if failures:
        print("=== W288 terminal manual-scroll completion hold smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w288_terminal_manual_scroll_completion_hold_smoke: ok")


if __name__ == "__main__":
    main()
