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
        "fn slash_config_response",
        '"command": "config"',
        '"protocol": "stream_json"',
        '"flagSettingsPathRedacted"',
        '"flagSettingsInlineValuesRedacted": true',
        '"rawConfigIncluded": false',
        '"inlinePluginNamesIncluded": false',
        '"envValuesRedacted": true',
        "redacted_json_value_kind",
        "slash_command_config_returns_redacted_runtime_snapshot",
        '"command":"/settings"',
    ]:
        require(structured, needle, "structured config slash command")

    for needle in [
        "ResultKind::Config",
        '"slash.config"',
        "CommandStatus::Available",
        '"settings".to_string()',
        '"sources".to_string()',
        "redacted read-only session/config source snapshot",
    ]:
        require(capabilities, needle, "config capability")

    require(
        run_all,
        "wave_w211_stream_json_config_slash_command_smoke",
        "run_all registration",
    )
    require(phase, "2026-05-23 Stream-json /config slash command bridge", "phase note")

    print("wave_w211_stream_json_config_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
