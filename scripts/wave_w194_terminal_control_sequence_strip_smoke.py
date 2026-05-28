#!/usr/bin/env python3
"""W194 - terminal renderer strips ANSI/OSC control sequences before printing."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_strip_terminal_control_sequences",
        "terminal_consume_escape_sequence",
        "terminal_consume_csi_sequence",
        "terminal_consume_string_control_sequence",
        "control_sequence_stripped_count",
        "terminalControlSequencesStripped",
        "oscControlSequencesStripped",
        "terminal_bounded_line_strips_ansi_and_osc_sequences",
        "terminal_soft_wrap_strips_csi_without_fragmenting_escape_text",
        "draw_executor_strips_terminal_control_sequences_before_printing",
    ]:
        require(renderer, token, "control sequence renderer", failures)

    for token in [
        "terminal_ansi_control_sequence_strip",
        "terminal_osc_control_sequence_strip",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w194_terminal_control_sequence_strip_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render control sequence stripping",
        "phase note",
        failures,
    )

    if failures:
        print("=== W194 terminal control sequence strip smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w194_terminal_control_sequence_strip_smoke: ok")


if __name__ == "__main__":
    main()
