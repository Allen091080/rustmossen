#!/usr/bin/env python3
"""W199 - terminal command output keeps a bounded stream tail."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_command_tail_line_items",
        "outputChunkCount",
        "observedOutputLines",
        "outputTailLineItems",
        "expandedOutputTailLineItems",
        "terminal_command_widget_accumulates_bounded_stream_tail_across_chunks",
    ]:
        require(events, token, "command stream tail renderer", failures)

    for token in [
        "terminal_command_stream_tail_buffer",
        "terminal_command_stream_chunk_accounting",
        "terminal_command_bounded_tail_preview",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w199_terminal_command_stream_tail_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render command stream tail buffer",
        "phase note",
        failures,
    )

    if failures:
        print("=== W199 terminal command stream tail smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w199_terminal_command_stream_tail_smoke: ok")


if __name__ == "__main__":
    main()
