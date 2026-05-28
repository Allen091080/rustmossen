#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME_STATUS = ROOT / "crates/mossen-agent/src/services/root/runtime_status.rs"
DIALOGUE = ROOT / "crates/mossen-agent/src/dialogue.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    runtime_status = RUNTIME_STATUS.read_text()
    dialogue = DIALOGUE.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "total_tool_calls_started",
        "total_tool_calls_completed",
        "total_tool_calls_failed",
        "total_tool_calls_denied",
        "total_permission_decisions",
        "permission_mode_decisions",
        "permission_gate_decisions",
        "permission_not_required_decisions",
        "record_tool_call_start",
        "record_tool_call_finish",
        "record_tool_permission_decision",
        "runtime_status_tracks_tool_and_permission_decisions",
    ):
        require(runtime_status, token, f"runtime tool/permission token {token}")

    for token in (
        "record_tool_call_start(tool_name)",
        "record_tool_permission_decision(",
        "permission_decision_label(decision)",
        'permission_source = if mode_decision.is_some()',
        '"permission_mode"',
        '"permission_gate"',
        '"not_required"',
        'record_tool_call_finish(tool_name, "denied")',
        'record_tool_call_finish(tool_name, "error")',
        '"completed"',
    ):
        require(dialogue, token, f"dialogue tool/permission token {token}")

    for token in (
        '"totalToolCallsStarted"',
        '"totalPermissionDecisions"',
        '"permissionModeDecisions"',
    ):
        require(structured_io, token, f"StructuredIO status assertion {token}")

    require(
        run_all,
        "wave_w133_runtime_tool_permission_status_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Runtime tool and permission status",
        "phase note",
    )

    print("wave_w133_runtime_tool_permission_status_smoke: ok")


if __name__ == "__main__":
    main()
