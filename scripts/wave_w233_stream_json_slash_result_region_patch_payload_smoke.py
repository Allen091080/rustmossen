#!/usr/bin/env python3
"""W233 - slash result events carry anchored region patch hints."""

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
        "terminal_attach_slash_result_region_patch",
        "terminal_copy_slash_result_region_patch_payload",
        "terminal_slash_result_retire_region_patch_value",
        "slash_command_result_event_payload_carries_terminal_region_patch",
        '"terminalRegionPatch"',
        '"anchored_region_patch"',
        '"replaceWholeScreen": false',
        '"requiresFrameTopStackLayout": true',
        '"clear_region"',
        "secret-token-value",
    ]:
        require(bridge, token, "slash result event region patch payload", failures)

    for token in [
        "slash_result_event_region_patch_payload",
        "terminal_slash_result_event_region_patch_payload",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w233_stream_json_slash_result_region_patch_payload_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result event region patch payload",
        "phase note",
        failures,
    )

    if failures:
        print("=== W233 stream-json slash result region patch payload smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w233_stream_json_slash_result_region_patch_payload_smoke: ok")


if __name__ == "__main__":
    main()
