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
        "resolve_pending_permission_with_approval_action",
        "permission_decision_for_approval_action",
        "permission_decision_control_response",
        "notify_control_request_resolved",
        'TERMINAL_APPROVAL_ACTION_APPROVE_ONCE: &str = "approve_once"',
        'TERMINAL_APPROVAL_ACTION_REJECT: &str = "reject"',
        'TERMINAL_APPROVAL_ACTION_EDIT_COMMAND: &str = "edit_command"',
        'TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION: &str = "approve_for_session"',
        'behavior: "allow".to_string()',
        'behavior: "deny".to_string()',
        "requires updatedInput or command",
        "terminal_session_permission_updates_for_request",
        "terminal_session_allow_rule_update",
        '"updatedPermissions"',
        "ambiguous terminal approval action",
        '#[serde(rename_all = "camelCase")]',
    ):
        require(structured_io, token, f"approval decision bridge token {token}")

    for token in (
        '"terminal_approval_decision_bridge"',
        '"terminal_approval_decision_bridge_fail_closed"',
        '"terminal_approval_approve_once_bridge"',
        '"terminal_approval_reject_bridge"',
        '"terminal_approval_session_rule_bridge"',
        '"terminal_approval_session_rule_updates"',
    ):
        require(structured_io, token, f"status decision bridge metadata {token}")

    for test_name in (
        "terminal_approval_action_bridge_approve_once_resolves_pending_permission",
        "terminal_approval_action_bridge_reject_resolves_pending_permission_and_callback",
        "terminal_approval_action_bridge_approve_for_session_returns_rule_update",
        "terminal_approval_action_bridge_edit_command_requires_updated_input",
        "terminal_approval_action_bridge_multiple_pending_permissions_fail_closed",
        "terminal_approval_action_bridge_returns_none_without_pending_permission",
    ):
        require(structured_io, test_name, f"approval bridge test {test_name}")

    require(
        run_all,
        "wave_w162_stream_json_terminal_approval_decision_bridge_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval decision bridge",
        "phase note",
    )

    print("wave_w162_stream_json_terminal_approval_decision_bridge_smoke: ok")


if __name__ == "__main__":
    main()
