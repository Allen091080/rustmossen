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
        "fn slash_profile_response",
        '"command": "profile"',
        '"currentProfileName"',
        '"writesConfigFiles": false',
        '"apiKeysRedacted": true',
        '"baseUrlsRedacted": true',
        "set_session_active_profile",
        "clear_session_active_profile",
        "slash_command_profile_lists_and_switches_session_profile",
        '"command":"/profiles"',
    ]:
        require(structured, needle, "structured profile slash command")

    for needle in [
        "ResultKind::Profile",
        '"slash.profile"',
        "CommandStatus::Available",
        '"profiles".to_string()',
        '"use".to_string()',
        "active profile for the current session",
    ]:
        require(capabilities, needle, "profile capability")

    require(
        run_all,
        "wave_w214_stream_json_profile_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /profile slash command bridge", "phase note")

    print("wave_w214_stream_json_profile_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
