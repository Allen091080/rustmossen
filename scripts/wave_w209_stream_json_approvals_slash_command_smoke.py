#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    structured = read("crates/mossen-cli/src/structured_io.rs")
    capabilities = read("crates/mossen-agent/src/services/root/slash_command_capabilities.rs")
    run_all = read("scripts/run_all_smoke.sh")
    phase = read("phases/03g-rendering-product-grade-plan.md")

    for needle in [
        "async fn slash_approvals_response",
        '"command": "approvals"',
        '"pendingCount"',
        '"rawPayloadsRedacted": true',
        '"inputsRedacted": true',
        '"actionControlSubtype": "terminal_approval_action"',
        "terminal_approval_pending_request_entry",
        "terminal_approval_action_options",
        "slash_command_approvals_reports_redacted_pending_state",
        '"command":"/approval-history"',
    ]:
        require(structured, needle, "structured approvals slash command")

    for needle in [
        "ResultKind::Approvals",
        '"slash.approvals"',
        '"approvals"',
        '"approval-history".to_string()',
        '"approval-log".to_string()',
        '"pending".to_string()',
        "redacted pending approval state",
    ]:
        require(capabilities, needle, "approvals capability")

    require(
        run_all,
        "wave_w209_stream_json_approvals_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /approvals slash command bridge", "phase note")

    print("wave_w209_stream_json_approvals_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
