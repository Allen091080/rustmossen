#!/usr/bin/env python3
"""W231 - slash result render events expose region and lifecycle contracts."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_enrich_region_contract_event_value",
        "terminal_slash_result_region_contract_value",
        "terminal_slash_result_retire_region_contract_value",
        "terminal_event_retires_slash_result",
        "terminal_payload_retires_region",
        "slash_command_result_event_payload_carries_terminal_region_contract",
        "slash_result_lifecycle_event_payload_carries_retire_contract",
        '"terminalRegion"',
        '"terminalRetireRegions"',
        '"retireRegionIds"',
        '"replace_slash_result"',
        '"clear_retired"',
    ]:
        require(bridge, token, "slash result event region contract", failures)

    for token in [
        "slash_result_event_region_contract",
        "slash_result_lifecycle_event_retire_contract",
        "terminal_slash_result_event_region_contract",
        "terminal_slash_result_lifecycle_event_retire_contract",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w231_stream_json_slash_result_region_contract_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result event region contract",
        "phase note",
        failures,
    )

    if failures:
        print("=== W231 stream-json slash result region contract smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w231_stream_json_slash_result_region_contract_smoke: ok")


if __name__ == "__main__":
    main()
