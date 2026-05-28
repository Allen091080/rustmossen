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
        "async fn slash_doctor_response",
        '"command": "doctor"',
        '"analysisDepth": "runtime_health_snapshot"',
        '"externalChecksRun": false',
        '"networkChecksRun": false',
        '"slowChecksSkipped": true',
        '"synchronizedUpdateFailClosed"',
        '"serverDetailsIncluded": false',
        '"installPathsRedacted": true',
        "slash_command_doctor_returns_redacted_runtime_health_snapshot",
        '"command":"/doctor"',
    ]:
        require(structured, needle, "structured doctor slash command")

    for needle in [
        "ResultKind::Doctor",
        '"slash.doctor"',
        "CommandStatus::Available",
        '"render".to_string()',
        "redacted read-only stream-json runtime health snapshot",
    ]:
        require(capabilities, needle, "doctor capability")

    require(
        run_all,
        "wave_w212_stream_json_doctor_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /doctor slash command bridge", "phase note")

    print("wave_w212_stream_json_doctor_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
