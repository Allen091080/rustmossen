#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDER_EVENTS = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    render_events = RENDER_EVENTS.read_text()
    repl = REPL.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "ActivateFocusedApprovalAction",
        "ActivateApprovalActionByKey",
        "activate_focused_approval_action",
        "activate_approval_action_by_key",
        "record_approval_action_intent",
        "approval_action_intent",
        '"pendingIntent"',
        '"enterSelectsFocused"',
        '"activationKeys"',
        "select: Enter or y/n/e/a",
        "decision bridge pending",
        "terminal_approval_action_activation_records_render_intent_without_resolving_decision",
    ):
        require(render_events, token, f"approval activation render token {token}")

    for token in (
        "ActivateFocusedApprovalAction",
        "ActivateApprovalActionByKey",
        "KeyCode::Enter",
        "KeyCode::Char('y')",
        "maps_approval_activation_keys_to_frontend_events",
    ):
        require(repl, token, f"approval activation frontend token {token}")

    for token in (
        '"terminal_approval_action_activation"',
        '"terminal_approval_enter_select"',
        '"terminal_approval_shortcut_actions"',
        '"terminal_approval_action_intent_model"',
    ):
        require(structured_io, token, f"status approval activation metadata {token}")

    require(
        run_all,
        "wave_w161_stream_json_terminal_approval_action_activation_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval action activation intent",
        "phase note",
    )

    print("wave_w161_stream_json_terminal_approval_action_activation_smoke: ok")


if __name__ == "__main__":
    main()
