#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "handle_terminal_approval_action_control_request",
        "terminal_approval_action_id_from_request",
        "terminal_approval_edited_input_from_request",
        "resolve_pending_permission_with_approval_action_input",
        "terminal_edit_command_updated_input_for_request",
        '"terminal_approval_action"',
        '"terminal_approval_action_result"',
        '"edit_command"',
        '"updatedInput"',
        "user_modified: Some(true)",
        '"userModified": true',
        "terminal_approval_action_bridge_edit_command_returns_updated_input",
        "terminal_approval_action_control_request_edit_command_submits_updated_input",
        "terminal_approval_action_bridge_edit_command_requires_updated_input",
    ):
        require(structured_io, token, f"edit command bridge token {token}")

    for token in (
        '"terminal_approval_action_control_request"',
        '"terminal_approval_edit_command_bridge"',
        '"terminal_approval_edit_command_updated_input"',
    ):
        require(structured_io, token, f"edit command status metadata {token}")

    require(
        run_all,
        "wave_w168_stream_json_terminal_edit_command_bridge_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal edit-command approval bridge",
        "phase note",
    )

    print("wave_w168_stream_json_terminal_edit_command_bridge_smoke: ok")


if __name__ == "__main__":
    main()
