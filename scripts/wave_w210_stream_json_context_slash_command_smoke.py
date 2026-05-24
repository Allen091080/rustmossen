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
        "fn slash_context_response",
        '"command": "context"',
        '"analysisDepth": "token_usage_snapshot"',
        '"messageLevelAnalysisIncluded": false',
        '"contextInputTokens"',
        '"effectiveWindowTokens"',
        '"autoCompactEligible"',
        '"messageContentRedacted": true',
        "slash_command_context_reports_token_window_snapshot",
        '"command":"/ctx"',
    ]:
        require(structured, needle, "structured context slash command")

    for needle in [
        "ResultKind::Context",
        '"slash.context"',
        "CommandStatus::Available",
        '"ctx".to_string()',
        '"breakdown".to_string()',
        "read-only token usage and context-window snapshot",
    ]:
        require(capabilities, needle, "context capability")

    require(
        run_all,
        "wave_w210_stream_json_context_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /context slash command bridge", "phase note")

    print("wave_w210_stream_json_context_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
