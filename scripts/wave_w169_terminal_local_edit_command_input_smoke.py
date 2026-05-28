#!/usr/bin/env python3
"""W169 - local terminal edit-command input smoke."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    types = (ROOT / "crates/mossen-agent/src/types.rs").read_text()
    dialogue = (ROOT / "crates/mossen-agent/src/dialogue.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "AllowWithUpdatedInput",
        "updated_input",
        "decision.updated_input().cloned()",
    ]:
        require(types + dialogue, token, "agent updated input execution", failures)

    for token in [
        "terminal_render_frontend_event_from_crossterm_with_edit_capture",
        "EditCommandInputChar",
        "EditCommandBackspace",
        "EditCommandSubmit",
        "EditCommandCancel",
        "begin_edit_command",
        "submit_edited_command",
        "terminal_render_updated_input_for_edited_command",
        "PermissionDecision::AllowWithUpdatedInput",
        "terminal_approval_bridge_submits_edited_command_updated_input",
        "terminal_approval_bridge_edit_command_empty_stays_pending",
    ]:
        require(repl, token, "local edit command bridge", failures)

    for token in [
        "emit_terminal_approval_edit_command_items",
        "mark_approval_edit_command_status",
        "edit command: {command}",
        "edit: type command, Enter submits, Esc cancels",
        "terminal_approval_edit_command_status_renders_inline_editor",
    ]:
        require(events, token, "render inline editor", failures)

    for token in [
        "terminal_approval_local_edit_command_input",
        "terminal_approval_local_edit_command_submit",
        "permission_decision_updated_input_execution",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w169_terminal_local_edit_command_input_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Local terminal edit-command input bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W169 local terminal edit-command input smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w169_terminal_local_edit_command_input_smoke: ok")


if __name__ == "__main__":
    main()
