#!/usr/bin/env python3
"""W247 - terminal text writes are byte-budgeted before printing."""

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
        "STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES",
        "StreamJsonTerminalTextByteBudget",
        "terminal_draw_text_byte_budget",
        "terminal_draw_text_byte_budget_value",
        "terminal_draw_budget_text_for_write",
        "terminal_truncate_text_to_byte_budget",
        '"cap_terminal_text_bytes_before_terminal_writes"',
        '"terminalTextByteBudgeted"',
        "terminal_text_byte_budget_exceeded",
        "draw_executor_caps_terminal_text_bytes_before_terminal_writes",
    ]:
        require(terminal, token, "terminal text byte budget", failures)

    for token in [
        "terminal_text_byte_write_budget",
        "terminal_text_byte_budget_executor_enforced",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w247_terminal_text_byte_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render text byte write budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W247 terminal text byte budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w247_terminal_text_byte_budget_smoke: ok")


if __name__ == "__main__":
    main()
