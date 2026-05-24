#!/usr/bin/env python3
"""W204 - stream-json /model slash command is wired."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    caps = (
        ROOT / "crates/mossen-agent/src/services/root/slash_command_capabilities.rs"
    ).read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        '"model" => match slash_model_response(&args)',
        "fn slash_model_response",
        "fn slash_model_summary_response",
        "set_main_loop_model_override(Some(requested_model))",
        "set_main_loop_model_override(None)",
        "slash_command_model_reports_and_updates_override",
    ]:
        require(structured, token, "model slash handler", failures)

    for token in [
        '"model"',
        '"mutationSupported"',
        '"switchAppliesToNextTurn"',
        '"availableAliases"',
        '"modelStringsAvailable"',
    ]:
        require(structured, token, "model payload", failures)

    for token in [
        "structured_io.rs:slash_command/model",
        "SideEffect::SwitchesSessionModel",
        '"set".to_string()',
        '"reset".to_string()',
    ]:
        require(caps, token, "capability source", failures)

    require(
        run_all,
        "wave_w204_stream_json_model_slash_command_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json /model slash command bridge",
        "phase note",
        failures,
    )

    if failures:
        print("=== W204 stream-json /model slash command smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w204_stream_json_model_slash_command_smoke: ok")


if __name__ == "__main__":
    main()
