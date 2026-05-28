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
        "terminal_session_permission_updates_for_request",
        "terminal_session_permission_updates_from_suggestions",
        "terminal_session_permission_update_from_suggestion",
        "terminal_session_allow_rule_update",
        "terminal_session_rule_content_for_input",
        "terminal_permission_rule_text",
        '"destination": "session"',
        '"behavior": "allow"',
        '"type": "addRules"',
        '"terminal_approval_action"',
        "TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION",
        "terminal_approval_action_bridge_approve_for_session_returns_rule_update",
    ):
        require(structured_io, token, f"session rule bridge token {token}")

    for token in (
        '"terminal_approval_session_rule_bridge"',
        '"terminal_approval_session_rule_updates"',
    ):
        require(structured_io, token, f"status session rule metadata {token}")

    require(
        run_all,
        "wave_w165_stream_json_terminal_approval_session_rule_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval session rule bridge",
        "phase note",
    )

    print("wave_w165_stream_json_terminal_approval_session_rule_smoke: ok")


if __name__ == "__main__":
    main()
