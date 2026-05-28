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
        "FocusNextApprovalAction",
        "FocusPreviousApprovalAction",
        "focus_next_approval_action",
        "focus_previous_approval_action",
        "terminal_approval_action_model_value",
        "terminal_approval_action_specs",
        '"focusedAction"',
        '"focusKeys"',
        "Edit command",
        "Tab action:",
        "terminal_approval_action_focus_cycles_without_resolving_decision",
    ):
        require(render_events, token, f"approval action model token {token}")

    for token in (
        "FocusNextApprovalAction",
        "FocusPreviousApprovalAction",
        "KeyCode::Tab | KeyCode::Right",
        "KeyCode::BackTab | KeyCode::Left",
        "maps_approval_focus_keys_to_frontend_events",
    ):
        require(repl, token, f"approval focus frontend token {token}")

    for token in (
        '"terminal_approval_action_model"',
        '"terminal_approval_focus_navigation"',
        '"terminal_approval_edit_command_action"',
        '"terminal_approval_session_action"',
    ):
        require(structured_io, token, f"status approval action metadata {token}")

    require(
        run_all,
        "wave_w160_stream_json_terminal_approval_action_focus_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal approval action focus",
        "phase note",
    )

    print("wave_w160_stream_json_terminal_approval_action_focus_smoke: ok")


if __name__ == "__main__":
    main()
