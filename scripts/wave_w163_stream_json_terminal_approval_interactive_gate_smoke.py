#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TYPES = ROOT / "crates/mossen-agent/src/types.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
RENDER_EVENTS = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    types = TYPES.read_text()
    repl = REPL.read_text()
    render_events = RENDER_EVENTS.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "InteractiveGate::new(permission_request_tx)",
        "permission_requests_open",
        "terminal_render_handle_permission_request",
        "TerminalRenderApprovalBridge",
        "TerminalRenderPendingPermission",
        "submit_action",
        "PermissionDecision::AllowAlways",
        'TERMINAL_APPROVAL_ACTION_EDIT_COMMAND =>',
        'bridge_status: "unsupported"',
        'bridge_status: "no_pending_permission"',
        "terminal_approval_bridge_submits_allow_reject_and_session_decisions",
        "terminal_approval_bridge_keeps_edit_command_fail_closed",
        "terminal_approval_bridge_reports_no_pending_permission",
    ):
        require(repl, token, f"terminal approval interactive gate token {token}")

    for token in (
        "interactive_gate_session_rule_key",
        "interactive_gate_shell_command_rule",
        "interactive_gate_rule_text",
        "interactive_gate_allow_always_is_scoped_to_exact_shell_command",
    ):
        require(types, token, f"interactive gate scoped rule token {token}")

    for token in (
        "emit_terminal_permission_request_items",
        "pending_terminal_approval_action_id",
        "emit_terminal_approval_bridge_status_items",
        "mark_approval_action_bridge_status",
        '"terminal_permission_request"',
        '"bridgeStatus"',
        "terminal_approval_bridge_status_retires_blocking_region_after_submit",
    ):
        require(render_events, token, f"render bridge status token {token}")

    for token in (
        '"terminal_approval_interactive_gate_bridge"',
        '"terminal_approval_local_decision_submit"',
        '"terminal_approval_allow_always_session_bridge"',
        '"terminal_approval_edit_command_fail_closed"',
    ):
        require(structured_io, token, f"status interactive gate metadata {token}")

    require(
        run_all,
        "wave_w163_stream_json_terminal_approval_interactive_gate_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval interactive gate bridge",
        "phase note",
    )

    print("wave_w163_stream_json_terminal_approval_interactive_gate_smoke: ok")


if __name__ == "__main__":
    main()
