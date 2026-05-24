#!/usr/bin/env python3
"""W260 - terminal frontend dispatches only draw plans to runtime."""

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
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "emit_terminal_draw_plan_items_for_sdk_message",
        "emit_current_terminal_draw_plan_items",
        "emit_terminal_permission_request_draw_plan_items",
        "emit_terminal_widget_control_draw_plan_items",
        "emit_terminal_approval_bridge_status_draw_plan_items",
        "emit_terminal_approval_edit_command_draw_plan_items",
        "terminal_frontend_sdk_emit_returns_only_draw_plan_item",
        "terminal_frontend_permission_emit_returns_only_draw_plan_item",
    ]:
        require(events, token, "draw-plan-only event emitter", failures)

    for token in [
        "emit_terminal_draw_plan_items_for_sdk_message(msg)",
        "emit_terminal_permission_request_draw_plan_items",
        "emit_terminal_widget_control_draw_plan_items",
        "emit_terminal_approval_bridge_status_draw_plan_items",
        "emit_terminal_approval_edit_command_draw_plan_items",
    ]:
        require(repl, token, "terminal frontend draw-plan-only dispatch", failures)

    for token in [
        "emit_stream_items_for_sdk_message(msg)",
        "emit_terminal_permission_request_items(&tool_name, &input)",
        "emit_terminal_widget_control_items(",
        "emit_terminal_approval_bridge_status_items(",
        "emit_terminal_approval_edit_command_items(",
    ]:
        forbid(repl, token, "terminal frontend legacy full-item dispatch", failures)

    for token in [
        "terminal_frontend_draw_plan_only_dispatch",
        "terminal_frontend_omits_snapshot_frame_patch_dispatch",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w260_terminal_frontend_draw_plan_only_dispatch_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render frontend draw-plan-only dispatch",
        "phase note",
        failures,
    )

    if failures:
        print("=== W260 terminal frontend draw-plan-only dispatch smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w260_terminal_frontend_draw_plan_only_dispatch_smoke: ok")


if __name__ == "__main__":
    main()
