#!/usr/bin/env python3
"""W174 - terminal-render interrupt cancels in-flight tool execution."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    tool_registry = (ROOT / "crates/mossen-agent/src/tool_registry.rs").read_text()
    dialogue = (ROOT / "crates/mossen-agent/src/dialogue.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "execute_with_cancel",
        "cancel.cancelled()",
        "Tool execution cancelled",
        "execute_with_cancel_drops_in_flight_tool_future",
        "DropFlag",
    ]:
        require(tool_registry, token, "tool registry cancellation boundary", failures)

    for token in [
        ".execute_with_cancel(tool_name, input, &state.tool_use_context, cancel)",
        "record_tool_call_finish(tool_name, \"cancelled\")",
        "TerminalReason::AbortedTools",
        "result = execute_mcp_tool(tool_name, input.clone())",
    ]:
        require(dialogue, token, "dialogue tool cancellation handoff", failures)

    require(
        structured,
        "terminal_frontend_interrupt_cancels_tool_execution",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w174_terminal_interrupt_tool_cancel_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render interrupt tool cancellation",
        "phase note",
        failures,
    )

    if failures:
        print("=== W174 terminal interrupt tool cancel smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w174_terminal_interrupt_tool_cancel_smoke: ok")


if __name__ == "__main__":
    main()
