#!/usr/bin/env python3
"""W257 - scrollback soft-wrap sanitizes controls while applying budgets."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def forbid(text: str, token: str, label: str, failures: list[str]) -> None:
    if token in text:
        failures.append(f"{label}: forbidden {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_draw_scrollback_soft_wrap_streaming_sanitizer_value",
        "terminal_consume_terminal_control_grapheme",
        "terminal_consume_escape_sequence_graphemes",
        "terminal_consume_csi_sequence_graphemes",
        "terminal_consume_string_control_sequence_graphemes",
        "terminal_take_soft_wrap_current_line",
        "terminal_pop_last_grapheme_from_materialized_lines",
        '"strip_terminal_controls_while_budgeting_soft_wrap"',
        '"scrollbackSoftWrapStreamingSanitizer"',
        "terminal_soft_wrap_streaming_sanitizer_handles_controls_with_budget",
    ]:
        require(terminal, token, "streaming soft-wrap sanitizer", failures)

    forbid(
        terminal,
        "let (line, control_sequence_stripped_count, inline_control_normalized_count) =\n        terminal_sanitize_terminal_control_text(line);\n    let mut lines = Vec::new();",
        "budgeted soft-wrap full-line sanitize guard",
        failures,
    )

    for token in [
        "terminal_scrollback_soft_wrap_streaming_sanitizer",
        "terminal_soft_wrap_sanitize_without_full_line_clone",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w257_terminal_scrollback_soft_wrap_streaming_sanitizer_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render scrollback soft-wrap streaming sanitizer",
        "phase note",
        failures,
    )

    if failures:
        print("=== W257 terminal scrollback soft-wrap streaming sanitizer smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w257_terminal_scrollback_soft_wrap_streaming_sanitizer_smoke: ok")


if __name__ == "__main__":
    main()
